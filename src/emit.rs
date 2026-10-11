//! Output assembly: stage the whole site outside `DIR`, copy the assets its
//! pages actually reference, and move the result into `--out` only once it
//! is complete (R10.2).

use std::collections::BTreeSet;

use camino::{Utf8Path, Utf8PathBuf};

use crate::cli::Config;
use crate::links;
use crate::model::{Error, Result, Site, SourceRel, Warnings};
use crate::render::Rendered;
use crate::theme;

/// Assemble the whole site outside `DIR`, then move it to `config.out` — and
/// only then, so a failed build never leaves a half-written output directory
/// (R10.2). `rendered` is parallel to `site.pages`.
///
/// A referenced asset that has vanished since discovery is a warning, not a
/// hard error (R5.2); the warnings raised while copying are returned rather
/// than dropped.
pub fn emit(config: &Config, site: &Site, rendered: &[Rendered]) -> Result<Warnings> {
    let mut warnings = Warnings::new();
    validate_stylesheet_targets(config, site, rendered)?;
    // Read once before staging: a selected file disappearing is a fatal error,
    // unlike an ordinary missing asset, and no output has been written yet.
    let stylesheet_bytes = config
        .stylesheet
        .as_deref()
        .map(|stylesheet| {
            std::fs::read(stylesheet).map_err(|source| {
                Error::Emit(format!(
                    "--stylesheet `{stylesheet}` could not be read: {source}"
                ))
            })
        })
        .transpose()?;

    // R10.1: staging never happens under DIR. R10.3: this guard's Drop
    // removes the directory on every exit path — success, error, or panic —
    // because it is a local variable that stays alive for the whole
    // function body.
    let staging = tempfile::TempDir::new().map_err(|source| Error::Io {
        stage: "output",
        source,
    })?;
    let staging_root = Utf8Path::from_path(staging.path())
        .ok_or_else(|| Error::Emit("temporary staging path is not UTF-8".to_string()))?;

    write_pages(site, rendered, staging_root, stylesheet_bytes.is_some())?;
    write_theme_assets(staging_root)?;
    if let (Some(stylesheet), Some(bytes)) = (&config.stylesheet, &stylesheet_bytes) {
        write_custom_stylesheet(staging_root, stylesheet, bytes)?;
    }
    copy_referenced_assets(config, rendered, staging_root, &mut warnings)?;
    move_into_place(config, staging_root)?;

    Ok(warnings)
}

/// Write each page's complete document (page shell wrapped around its
/// rendered body) to its target path under `staging_root`.
fn write_pages(
    site: &Site,
    rendered: &[Rendered],
    staging_root: &Utf8Path,
    include_stylesheet: bool,
) -> Result<()> {
    for (page, page_rendered) in site.pages.iter().zip(rendered.iter()) {
        let html = theme::shell_with_stylesheets(
            page,
            site,
            &page_rendered.html,
            &page_rendered.outline,
            include_stylesheet,
        );
        let dest = staging_root.join(page.target.as_path());
        write_file(&dest, html.as_bytes())?;
    }
    Ok(())
}

/// Write the bundled stylesheet and script (R8.7).
fn write_theme_assets(staging_root: &Utf8Path) -> Result<()> {
    write_file(
        &staging_root.join(theme::STYLE_PATH),
        theme::STYLE_CSS.as_bytes(),
    )?;
    write_file(
        &staging_root.join(theme::SCRIPT_PATH),
        theme::APP_JS.as_bytes(),
    )?;
    Ok(())
}

fn write_custom_stylesheet(
    staging_root: &Utf8Path,
    stylesheet: &Utf8Path,
    bytes: &[u8],
) -> Result<()> {
    write_file_io(&staging_root.join(theme::CUSTOM_STYLE_PATH), bytes).map_err(|source| {
        Error::Emit(format!(
            "--stylesheet `{stylesheet}` could not be written to `{}`: {source}",
            theme::CUSTOM_STYLE_PATH
        ))
    })
}

/// Reserve the custom stylesheet only when it has been explicitly selected.
/// Files cannot occupy their parent directories or descendants either. Check
/// only outputs actually used by this build, before any staged files are written.
fn validate_stylesheet_targets(config: &Config, site: &Site, rendered: &[Rendered]) -> Result<()> {
    if let Some(stylesheet) = &config.stylesheet {
        validate_reserved_style_targets(
            site,
            rendered,
            Utf8Path::new(theme::CUSTOM_STYLE_PATH),
            &format!("--stylesheet `{stylesheet}`"),
        )?;
    }
    Ok(())
}

fn validate_reserved_style_targets(
    site: &Site,
    rendered: &[Rendered],
    reserved: &Utf8Path,
    selection: &str,
) -> Result<()> {
    let reject_conflict = |path: &Utf8Path, kind: &str| {
        let folded = links::fold(path.as_str());
        let candidate = Utf8Path::new(&folded);
        if candidate.starts_with(reserved) || reserved.starts_with(candidate) {
            Err(Error::Emit(format!(
                "{selection} reserves `{reserved}`, which conflicts with {kind} `{path}`"
            )))
        } else {
            Ok(())
        }
    };
    for page in &site.pages {
        reject_conflict(page.target.as_path(), "page output")?;
    }
    for page_rendered in rendered {
        for asset in &page_rendered.assets {
            reject_conflict(asset.as_path(), "referenced asset")?;
        }
    }
    Ok(())
}

/// Copy every asset referenced by any rendered page, each exactly once
/// (R5.4), in deterministic sorted order (R11.3).
fn copy_referenced_assets(
    config: &Config,
    rendered: &[Rendered],
    staging_root: &Utf8Path,
    warnings: &mut Warnings,
) -> Result<()> {
    let mut assets: BTreeSet<&SourceRel> = BTreeSet::new();
    for page_rendered in rendered {
        for asset in &page_rendered.assets {
            assets.insert(asset);
        }
    }

    for asset in assets {
        let src = config.dir.join(asset.as_path());
        let dest = staging_root.join(asset.as_path());
        match std::fs::read(&src) {
            Ok(bytes) => write_file(&dest, &bytes)?,
            Err(_) => {
                // Vanished since discovery/render: warn, do not fail the
                // build (R5.2).
                warnings.warn(format!("asset not found, skipping copy: {asset}"));
            }
        }
    }

    Ok(())
}

/// Write `contents` to `path`, creating parent directories as needed.
fn write_file(path: &Utf8Path, contents: &[u8]) -> Result<()> {
    write_file_io(path, contents).map_err(|source| Error::Io {
        stage: "output",
        source,
    })
}

fn write_file_io(path: &Utf8Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)
}

/// Move the staged site into place. `cli` already guaranteed `config.out` is
/// either absent or empty, so it is safe to remove here before the rename.
///
/// R10.2: on any failure in this function, `config.out` is left absent or
/// empty — nothing is written into it incrementally.
fn move_into_place(config: &Config, staging_root: &Utf8Path) -> Result<()> {
    let out_parent = config
        .out
        .parent()
        .map(Utf8Path::to_path_buf)
        .unwrap_or_else(|| Utf8PathBuf::from("."));
    std::fs::create_dir_all(&out_parent).map_err(|source| Error::Io {
        stage: "output",
        source,
    })?;

    if config.out.exists() {
        std::fs::remove_dir(&config.out).map_err(|source| Error::Io {
            stage: "output",
            source,
        })?;
    }

    // The staging directory lives in the system temp dir, so a plain rename
    // ordinarily crosses filesystems (ErrorKind::CrossesDevices) — the
    // common case here, not the exception. Fall back to a recursive copy on
    // any rename failure rather than trying to distinguish the reason.
    if std::fs::rename(staging_root, &config.out).is_ok() {
        return Ok(());
    }

    // No scratch directory here: `--out` may point inside `DIR` (a
    // gitignored `DIR/site` is the ordinary layout R1.9 exists for), and
    // R10.1 permits only `config.out` itself to be written under `DIR` — a
    // sibling scratch directory next to it would violate that. So the copy
    // fallback writes straight into `config.out` and is not itself atomic;
    // R10.2 only requires that a failed build leave `config.out` absent or
    // empty, so on any error here `config.out` is removed again rather than
    // left partially written.
    if let Err(source) = copy_dir_recursive(staging_root, &config.out) {
        let _ = std::fs::remove_dir_all(&config.out);
        return Err(Error::Io {
            stage: "output",
            source,
        });
    }

    Ok(())
}

/// Recursively copy every regular file and directory under `src` into
/// `dest`, byte-identically.
fn copy_dir_recursive(src: &Utf8Path, dest: &Utf8Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let src_path = src.join(name.as_ref());
        let dest_path = dest.join(name.as_ref());
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else if file_type.is_file() {
            std::fs::copy(&src_path, &dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Page, SourceKind, TargetPath};

    fn config() -> Config {
        Config {
            dir: "/input".into(),
            out: "/output".into(),
            root: "README.md".into(),
            stylesheet: Some("/theme.css".into()),
            excludes: Vec::new(),
            out_inside_dir: None,
            todo: false,
        }
    }

    fn site(target: &str) -> Site {
        Site {
            pages: vec![Page {
                source: SourceRel::new("README.md"),
                target: TargetPath::new(target),
                title: "Home".into(),
                is_root: true,
            }],
            ..Site::default()
        }
    }

    fn rendered(assets: &[&str]) -> Rendered {
        Rendered {
            html: "<p>Home</p>".into(),
            outline: Vec::new(),
            assets: assets.iter().map(|path| SourceRel::new(*path)).collect(),
            warnings: Warnings::new(),
        }
    }

    #[test]
    fn stylesheet_reservation_rejects_casefolded_asset_and_prefix_conflicts() {
        let config = config();
        let site = site("index.html");
        for asset in [
            "assets/custom.css",
            "ASSETS/CUSTOM.CSS",
            "assets",
            "ASSETS",
            "Assets/Custom.Css/image.png",
        ] {
            let error = validate_stylesheet_targets(&config, &site, &[rendered(&[asset])])
                .unwrap_err()
                .to_string();
            assert!(error.starts_with("output:"), "{error}");
            assert!(error.contains("/theme.css"), "{error}");
            assert!(error.contains("assets/custom.css"), "{error}");
            assert!(error.contains(asset), "{error}");
            assert!(error.contains("referenced asset"), "{error}");
        }
    }

    #[test]
    fn stylesheet_reservation_rejects_pages_inside_reserved_file() {
        let config = config();
        let site = site("ASSETS/CUSTOM.CSS/guide.html");
        let error = validate_stylesheet_targets(&config, &site, &[rendered(&[])])
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("page output `ASSETS/CUSTOM.CSS/guide.html`"),
            "{error}"
        );
    }

    #[test]
    fn stylesheet_reservation_uses_component_boundaries_and_only_emitted_assets() {
        let config = config();
        let mut site = site("assets/custom.css-extra/guide.html");
        let source = SourceRel::new("assets/custom.css");
        site.by_source
            .insert(source.folded(), SourceKind::Asset(source));
        let page_rendered = rendered(&[
            "assets/custom.css.backup",
            "assets-extra/custom.css",
            "assets/theme.css",
        ]);
        assert!(validate_stylesheet_targets(&config, &site, &[page_rendered]).is_ok());
    }

    #[test]
    fn stylesheet_read_failure_preserves_absent_and_empty_output() {
        let fixture = tempfile::tempdir().unwrap();
        let fixture = Utf8Path::from_path(fixture.path()).unwrap();
        let missing = fixture.join("removed.css");
        for existing_out in [false, true] {
            let mut config = config();
            config.stylesheet = Some(missing.clone());
            config.out = fixture.join(format!("site-{existing_out}"));
            if existing_out {
                std::fs::create_dir(&config.out).unwrap();
            }
            let error = emit(&config, &site("index.html"), &[rendered(&[])])
                .unwrap_err()
                .to_string();
            assert!(error.starts_with("output:"), "{error}");
            assert!(error.contains(missing.as_str()), "{error}");
            assert!(error.contains("could not be read"), "{error}");
            if existing_out {
                assert_eq!(std::fs::read_dir(&config.out).unwrap().count(), 0);
            } else {
                assert!(!config.out.exists());
            }
        }
    }

    #[test]
    fn custom_stylesheet_write_is_byte_identical_and_failure_names_source() {
        let fixture = tempfile::tempdir().unwrap();
        let staging = Utf8Path::from_path(fixture.path()).unwrap();
        let source = Utf8Path::new("/input/custom-theme.css");
        let bytes = b"/* original bytes */\r\nbody { color: purple; }\n\xff";
        write_custom_stylesheet(staging, source, bytes).unwrap();
        assert_eq!(
            std::fs::read(staging.join(theme::CUSTOM_STYLE_PATH)).unwrap(),
            bytes
        );

        std::fs::remove_dir_all(staging.join("assets")).unwrap();
        std::fs::write(staging.join("assets"), b"file blocking the directory").unwrap();
        let error = write_custom_stylesheet(staging, source, bytes)
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("output:"), "{error}");
        assert!(error.contains(source.as_str()), "{error}");
        assert!(error.contains(theme::CUSTOM_STYLE_PATH), "{error}");
        assert!(error.contains("could not be written"), "{error}");
    }
}
