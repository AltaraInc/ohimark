//! Output assembly: stage the whole site outside `DIR`, copy the assets its
//! pages actually reference, and move the result into `--out` only once it
//! is complete (R10.2).

use std::collections::BTreeSet;

use camino::{Utf8Path, Utf8PathBuf};

use crate::cli::Config;
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

    write_pages(site, rendered, staging_root)?;
    write_theme_assets(staging_root)?;
    copy_referenced_assets(config, rendered, staging_root, &mut warnings)?;
    move_into_place(config, staging_root)?;

    Ok(warnings)
}

/// Write each page's complete document (page shell wrapped around its
/// rendered body) to its target path under `staging_root`.
fn write_pages(site: &Site, rendered: &[Rendered], staging_root: &Utf8Path) -> Result<()> {
    for (page, page_rendered) in site.pages.iter().zip(rendered.iter()) {
        let html = theme::shell(page, site, &page_rendered.html, &page_rendered.outline);
        let dest = staging_root.join(page.target.as_path());
        write_file(&dest, html.as_bytes())?;
    }
    Ok(())
}

/// Write the bundled stylesheet and script (R8.7: the only ones the site
/// loads).
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
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Io {
            stage: "output",
            source,
        })?;
    }
    std::fs::write(path, contents).map_err(|source| Error::Io {
        stage: "output",
        source,
    })
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
