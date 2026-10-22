//! Output assembly: stage the whole site outside `DIR`, copy the assets its
//! pages actually reference, and move the result into `--out` only once it
//! is complete (R10.2).

use std::collections::BTreeSet;

use camino::{Utf8Path, Utf8PathBuf};

use crate::cli::{Config, HtmlPage, PresetTheme};
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
    validate_html_page_targets(config, site, rendered)?;
    validate_stylesheet_targets(config, site, rendered)?;
    // Read the stylesheet once before staging. Explicit HTML is streamed into
    // staging one file at a time; failure of either selected input is fatal.
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

    write_pages(
        site,
        rendered,
        staging_root,
        config.theme.is_some(),
        stylesheet_bytes.is_some(),
        config.max_group_size,
    )?;
    write_theme_assets(staging_root)?;
    if let Some(preset) = config.theme {
        write_preset_stylesheet(staging_root, preset)?;
    }
    if let (Some(stylesheet), Some(bytes)) = (&config.stylesheet, &stylesheet_bytes) {
        write_custom_stylesheet(staging_root, stylesheet, bytes)?;
    }
    copy_html_pages(config, staging_root)?;
    copy_referenced_assets(config, rendered, staging_root, &mut warnings)?;
    move_into_place(config, staging_root)?;

    Ok(warnings)
}

/// Cache casefolding once per candidate while preserving its display spelling.
struct OutputPath<'a> {
    path: &'a Utf8Path,
    folded: Utf8PathBuf,
}

impl<'a> OutputPath<'a> {
    fn new(path: &'a Utf8Path) -> Self {
        Self {
            path,
            folded: links::fold(path.as_str()).into(),
        }
    }

    fn conflicts_with(&self, other: &Self) -> bool {
        self.folded.starts_with(&other.folded) || other.folded.starts_with(&self.folded)
    }
}

fn html_page_conflict(page: &HtmlPage, other: &Utf8Path, description: &str) -> Error {
    Error::Emit(format!(
        "--endpoint `{}` produces `{}`, which conflicts with {description} `{other}`",
        page.source, page.target
    ))
}

/// Explicit files must not overwrite any output actually used by this build.
/// Only an asset with the same source and exact target can share its copy.
fn validate_html_page_targets(config: &Config, site: &Site, rendered: &[Rendered]) -> Result<()> {
    if config.html_pages.is_empty() {
        return Ok(());
    }

    let mut reserved = vec![theme::STYLE_PATH, theme::SCRIPT_PATH];
    if config.theme.is_some() {
        reserved.push(theme::THEME_STYLE_PATH);
    }
    if config.stylesheet.is_some() {
        reserved.push(theme::CUSTOM_STYLE_PATH);
    }
    let reserved: Vec<OutputPath<'_>> = reserved
        .into_iter()
        .map(|path| OutputPath::new(Utf8Path::new(path)))
        .collect();
    let assets: BTreeSet<&SourceRel> = rendered
        .iter()
        .flat_map(|page| page.assets.iter())
        .collect();
    let assets: Vec<OutputPath<'_>> = assets
        .into_iter()
        .map(|asset| OutputPath::new(asset.as_path()))
        .collect();
    let pages: Vec<OutputPath<'_>> = site
        .pages
        .iter()
        .map(|page| OutputPath::new(page.target.as_path()))
        .collect();
    let html_pages: Vec<OutputPath<'_>> = config
        .html_pages
        .iter()
        .map(|page| OutputPath::new(page.target.as_path()))
        .collect();

    for (index, (html_page, target)) in config.html_pages.iter().zip(&html_pages).enumerate() {
        for (page, other) in site.pages.iter().zip(&pages) {
            if target.conflicts_with(other) {
                return Err(html_page_conflict(
                    html_page,
                    other.path,
                    &format!("Markdown page `{}` output", page.source),
                ));
            }
        }
        for (page, other) in config.html_pages[..index].iter().zip(&html_pages[..index]) {
            if target.conflicts_with(other) {
                return Err(html_page_conflict(
                    html_page,
                    other.path,
                    &format!("--endpoint `{}` output", page.source),
                ));
            }
        }
        for asset in &assets {
            if !target.conflicts_with(asset) {
                continue;
            }
            if target.path == asset.path
                && std::fs::canonicalize(config.dir.join(asset.path))
                    .is_ok_and(|source| source == html_page.source.as_std_path())
            {
                continue;
            }
            return Err(html_page_conflict(
                html_page,
                asset.path,
                "referenced asset",
            ));
        }
        for path in &reserved {
            if target.conflicts_with(path) {
                return Err(html_page_conflict(
                    html_page,
                    path.path,
                    "reserved site output",
                ));
            }
        }
    }
    Ok(())
}

/// Copy one explicit input at a time without retaining its contents in memory.
/// A failed copy is confined to staging, which is removed before returning.
fn copy_html_pages(config: &Config, staging_root: &Utf8Path) -> Result<()> {
    for page in &config.html_pages {
        let mut input = std::fs::File::open(&page.source).map_err(|source| {
            Error::Emit(format!(
                "--endpoint `{}` for target `{}` could not be read: {source}",
                page.source, page.target
            ))
        })?;
        let dest = staging_root.join(page.target.as_path());
        let mut output = (|| {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::File::create(dest)
        })()
        .map_err(|source| {
            Error::Emit(format!(
                "--endpoint `{}` could not be written to `{}`: {source}",
                page.source, page.target
            ))
        })?;
        std::io::copy(&mut input, &mut output).map_err(|source| {
            Error::Emit(format!(
                "--endpoint `{}` could not be copied to `{}`: {source}",
                page.source, page.target
            ))
        })?;
    }
    Ok(())
}

/// Write each page's complete document (page shell wrapped around its
/// rendered body) to its target path under `staging_root`.
fn write_pages(
    site: &Site,
    rendered: &[Rendered],
    staging_root: &Utf8Path,
    include_theme: bool,
    include_stylesheet: bool,
    max_group_size: u32,
) -> Result<()> {
    for (page, page_rendered) in site.pages.iter().zip(rendered.iter()) {
        let html = theme::shell_with_stylesheets(
            page,
            site,
            &page_rendered.html,
            &page_rendered.outline,
            include_theme,
            include_stylesheet,
            max_group_size,
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

fn preset_stylesheet(preset: PresetTheme) -> (&'static str, &'static str) {
    match preset {
        PresetTheme::Default => ("default", theme::DEFAULT_THEME_CSS),
        PresetTheme::Bookish => ("bookish", theme::BOOKISH_THEME_CSS),
    }
}

fn write_preset_stylesheet(staging_root: &Utf8Path, preset: PresetTheme) -> Result<()> {
    let (name, stylesheet) = preset_stylesheet(preset);
    write_file_io(
        &staging_root.join(theme::THEME_STYLE_PATH),
        stylesheet.as_bytes(),
    )
    .map_err(|source| {
        Error::Emit(format!(
            "--theme `{name}` could not be written to `{}`: {source}",
            theme::THEME_STYLE_PATH
        ))
    })
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

/// Reserve optional stylesheets only when each has been explicitly selected.
/// Files cannot occupy their parent directories or descendants either. Check
/// only outputs actually used by this build, before any staged files are written.
fn validate_stylesheet_targets(config: &Config, site: &Site, rendered: &[Rendered]) -> Result<()> {
    if let Some(preset) = config.theme {
        let (name, _) = preset_stylesheet(preset);
        validate_reserved_style_targets(
            site,
            rendered,
            Utf8Path::new(theme::THEME_STYLE_PATH),
            &format!("--theme `{name}`"),
        )?;
    }
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
        // Target validation already verified the source is identical; the
        // explicit selection was already streamed with fatal error handling.
        if config
            .html_pages
            .iter()
            .any(|page| page.target.as_path() == asset.as_path())
        {
            continue;
        }
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
            max_group_size: crate::cli::DEFAULT_MAX_GROUP_SIZE,
            theme: None,
            stylesheet: Some("/theme.css".into()),
            html_pages: Vec::new(),
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
                times: Default::default(),
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
    fn preset_reservation_rejects_casefolded_asset_prefixes_and_page_conflicts() {
        let mut config = config();
        config.stylesheet = None;
        config.theme = Some(PresetTheme::Bookish);
        let root = site("index.html");
        for asset in [
            "assets/theme.css",
            "ASSETS/THEME.CSS",
            "assets",
            "ASSETS",
            "Assets/Theme.Css/image.png",
        ] {
            let error = validate_stylesheet_targets(&config, &root, &[rendered(&[asset])])
                .unwrap_err()
                .to_string();
            assert!(error.starts_with("output:"), "{error}");
            assert!(error.contains("--theme `bookish`"), "{error}");
            assert!(error.contains("assets/theme.css"), "{error}");
            assert!(error.contains(asset), "{error}");
        }
        let error = validate_stylesheet_targets(
            &config,
            &site("ASSETS/THEME.CSS/guide.html"),
            &[rendered(&[])],
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("page output `ASSETS/THEME.CSS/guide.html`"),
            "{error}"
        );
        assert!(error.contains("--theme `bookish`"), "{error}");
    }

    #[test]
    fn stylesheet_reservation_uses_component_boundaries_and_only_emitted_assets() {
        let mut config = config();
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

        config.stylesheet = None;
        config.theme = Some(PresetTheme::Default);
        let source = SourceRel::new("assets/theme.css");
        site.by_source
            .insert(source.folded(), SourceKind::Asset(source));
        assert!(
            validate_stylesheet_targets(
                &config,
                &site,
                &[rendered(&[
                    "assets/custom.css",
                    "assets/theme.css.backup",
                    "assets-extra/theme.css",
                ])]
            )
            .is_ok()
        );
        config.stylesheet = Some("/theme.css".into());
        for (asset, option) in [
            ("assets/custom.css", "--stylesheet"),
            ("assets/theme.css", "--theme"),
        ] {
            let error = validate_stylesheet_targets(&config, &site, &[rendered(&[asset])])
                .unwrap_err()
                .to_string();
            assert!(error.contains(option), "{error}");
            assert!(error.contains(asset), "{error}");
        }
    }

    #[test]
    fn stylesheet_read_failure_preserves_absent_and_empty_output() {
        let fixture = tempfile::tempdir().unwrap();
        let fixture = Utf8Path::from_path(fixture.path()).unwrap();
        let missing = fixture.join("removed.css");
        for (existing_out, preset) in [
            (false, None),
            (true, None),
            (false, Some(PresetTheme::Default)),
            (true, Some(PresetTheme::Default)),
        ] {
            let mut config = config();
            config.theme = preset;
            config.stylesheet = Some(missing.clone());
            config.out = fixture.join(format!("site-{existing_out}-{}", preset.is_some()));
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
    fn vanished_html_page_is_fatal_and_preserves_absent_and_empty_output() {
        let fixture = tempfile::tempdir().unwrap();
        let fixture = Utf8Path::from_path(fixture.path()).unwrap();
        let source = fixture.join("selected.html");
        std::fs::write(&source, b"<p>Selected</p>").unwrap();
        let source = Utf8PathBuf::from_path_buf(std::fs::canonicalize(&source).unwrap()).unwrap();
        let mut config = config();
        config.stylesheet = None;
        config.html_pages.push(HtmlPage {
            title: "Selected".into(),
            source: source.clone(),
            target: TargetPath::new("selected.html"),
        });
        std::fs::remove_file(&source).unwrap();

        for existing_out in [false, true] {
            config.out = fixture.join(format!("site-{existing_out}"));
            if existing_out {
                std::fs::create_dir(&config.out).unwrap();
            }
            let error = emit(&config, &site("index.html"), &[rendered(&[])])
                .unwrap_err()
                .to_string();
            assert!(error.starts_with("output:"), "{error}");
            assert!(error.contains("--endpoint"), "{error}");
            assert!(error.contains(source.as_str()), "{error}");
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
