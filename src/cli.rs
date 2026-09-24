//! `clap` argument definitions and validation into a `Config` (R9).

use camino::{Utf8Path, Utf8PathBuf};
use clap::{ArgAction, Parser};

use crate::model::{Error, Result};

/// The raw command-line surface:
/// `ohimark --out OUTDIR [--root FILENAME] [--exclude GLOB]... DIR` (R9.1).
///
/// Fields accept arbitrary OS strings (not `Utf8PathBuf`) so that a non-UTF-8
/// argument surfaces as a clear `Error::Config` from [`Args::into_config`]
/// rather than an opaque `clap` parse failure.
#[derive(Debug, Parser)]
#[command(
    name = "ohimark",
    version,
    about = "Generate a static HTML site from a directory of Markdown.",
    // R9.6: for `--root` and `--out`, a repeated occurrence overrides the
    // previous one instead of `clap`'s default `ArgumentConflict` error.
    args_override_self = true
)]
pub struct Args {
    /// The directory of Markdown sources to build.
    #[arg(value_name = "DIR", help = "Directory of Markdown sources to build")]
    pub dir: std::path::PathBuf,

    /// The directory to write the built site into.
    ///
    /// Created if it does not exist; must be empty if it does (R9.3, R9.4).
    #[arg(
        short = 'o',
        long = "out",
        value_name = "OUTDIR",
        help = "Directory to write the built site into (created if absent, must be empty if present)"
    )]
    pub out: std::path::PathBuf,

    /// The site's home file, as a path relative to `DIR`. Defaults to `README.md`.
    #[arg(
        short = 'r',
        long = "root",
        value_name = "FILENAME",
        default_value = "README.md",
        help = "Home file, relative to DIR, emitted as index.html [default: README.md]"
    )]
    pub root: std::path::PathBuf,

    /// A glob to exclude from discovery, matched against `DIR`-relative paths.
    /// Repeatable; every occurrence accumulates (R9.6).
    #[arg(
        long = "exclude",
        value_name = "GLOB",
        action = ArgAction::Append,
        help = "Glob to exclude from discovery, relative to DIR (repeatable)"
    )]
    pub excludes: Vec<String>,
}

/// A validated, normalised configuration.
#[derive(Clone, Debug)]
pub struct Config {
    /// The positional root directory, canonicalised to an absolute path.
    pub dir: Utf8PathBuf,
    /// The output directory, absolute. Created by `emit`, not here.
    pub out: Utf8PathBuf,
    /// The `--root` file as a `DIR`-relative path. Defaults to `README.md`.
    pub root: Utf8PathBuf,
    /// Repeatable `--exclude` globs, in the order given.
    pub excludes: Vec<String>,
    /// `Some(rel)` when `out` lies inside `dir`; the `DIR`-relative path of the
    /// output directory, which discovery must skip (R1.9).
    pub out_inside_dir: Option<Utf8PathBuf>,
}

impl Args {
    /// Validate and normalise. Every failure is `Error::Config` (R10.4).
    pub fn into_config(self) -> Result<Config> {
        let dir = validate_dir(&self.dir)?;
        let out = normalize_absolute(&resolve_against_cwd(to_utf8(&self.out, "OUTDIR")?)?);
        validate_out(&out)?;
        reject_out_swallows_dir(&out, &dir)?;
        let out_inside_dir = compute_out_inside_dir(&out, &dir);
        let root = resolve_root(&to_utf8(&self.root, "--root")?)?;

        Ok(Config {
            dir,
            out,
            root,
            excludes: self.excludes,
            out_inside_dir,
        })
    }
}

/// Convert an OS path argument to UTF-8, or a clear `Error::Config` (R9 preamble).
fn to_utf8(path: &std::path::Path, what: &str) -> Result<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(path.to_path_buf())
        .map_err(|p| Error::Config(format!("{what} `{}` is not valid UTF-8", p.display())))
}

/// R9.2: `DIR` must exist, be a directory, and is canonicalised.
fn validate_dir(dir: &std::path::Path) -> Result<Utf8PathBuf> {
    let dir = to_utf8(dir, "DIR")?;
    let metadata = std::fs::metadata(dir.as_std_path())
        .map_err(|_| Error::Config(format!("DIR `{dir}` does not exist")))?;
    if !metadata.is_dir() {
        return Err(Error::Config(format!("DIR `{dir}` is not a directory")));
    }
    let canonical = std::fs::canonicalize(dir.as_std_path())
        .map_err(|source| Error::Config(format!("DIR `{dir}` could not be resolved: {source}")))?;
    Utf8PathBuf::from_path_buf(canonical)
        .map_err(|p| Error::Config(format!("DIR `{}` is not valid UTF-8", p.display())))
}

/// Join a relative path to the current working directory; leave an absolute one alone.
fn resolve_against_cwd(path: Utf8PathBuf) -> Result<Utf8PathBuf> {
    if path.is_absolute() {
        return Ok(path);
    }
    let cwd = std::env::current_dir().map_err(|source| {
        Error::Config(format!("could not read the current directory: {source}"))
    })?;
    let cwd = Utf8PathBuf::from_path_buf(cwd).map_err(|p| {
        Error::Config(format!(
            "current directory `{}` is not valid UTF-8",
            p.display()
        ))
    })?;
    Ok(cwd.join(path))
}

/// Lexically collapse `.` and `..` components of an absolute POSIX path.
/// Never touches the filesystem: `out` may not exist yet.
fn normalize_absolute(path: &Utf8Path) -> Utf8PathBuf {
    let mut stack: Vec<&str> = Vec::new();
    for segment in path.as_str().split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            other => stack.push(other),
        }
    }
    Utf8PathBuf::from(format!("/{}", stack.join("/")))
}

/// R9.3 / R9.4: an existing `out` must be a directory, and empty.
fn validate_out(out: &Utf8Path) -> Result<()> {
    let metadata = match std::fs::metadata(out.as_std_path()) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(()), // does not exist: `emit` creates it.
    };
    if !metadata.is_dir() {
        return Err(Error::Config(format!(
            "--out `{out}` exists and is not a directory"
        )));
    }
    let mut entries = std::fs::read_dir(out.as_std_path())
        .map_err(|source| Error::Config(format!("--out `{out}` could not be read: {source}")))?;
    if entries.next().is_some() {
        return Err(Error::Config(format!(
            "--out `{out}` exists and is not empty"
        )));
    }
    Ok(())
}

/// `out` must not be `dir` itself, nor an ancestor of it (it would swallow the input).
fn reject_out_swallows_dir(out: &Utf8Path, dir: &Utf8Path) -> Result<()> {
    if dir.starts_with(out) {
        return Err(Error::Config(format!(
            "--out `{out}` must not be DIR `{dir}` or an ancestor of it"
        )));
    }
    Ok(())
}

/// R1.9: `Some(rel)` when `out` lies strictly inside `dir`.
fn compute_out_inside_dir(out: &Utf8Path, dir: &Utf8Path) -> Option<Utf8PathBuf> {
    out.strip_prefix(dir).ok().map(Utf8Path::to_path_buf)
}

/// R2.1: resolve `--root` to a `DIR`-relative, forward-slash path. Pure: no
/// filesystem access — existence is `plan`'s concern (R2.2), so the error
/// there names the right stage.
fn resolve_root(root: &Utf8Path) -> Result<Utf8PathBuf> {
    if root.is_absolute() {
        return Err(Error::Config(format!(
            "--root `{root}` must be relative to DIR, not absolute"
        )));
    }
    normalize_relative(root.as_str())
        .ok_or_else(|| Error::Config(format!("--root `{root}` escapes DIR")))
}

/// Lexically resolve `rel` against an empty (`DIR`) base. `None` if a leading
/// `..` survives, i.e. `rel` escapes above `DIR`.
fn normalize_relative(rel: &str) -> Option<Utf8PathBuf> {
    let mut stack: Vec<&str> = Vec::new();
    for segment in rel.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                stack.pop()?;
            }
            other => stack.push(other),
        }
    }
    Some(Utf8PathBuf::from(stack.join("/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r9_1_dir_and_out_are_required() {
        assert!(Args::try_parse_from(["ohimark", "--out", "site"]).is_err());
        assert!(Args::try_parse_from(["ohimark", "dir"]).is_err());
    }

    #[test]
    fn r9_6_flags_accepted_before_or_after_positional() {
        let before = Args::try_parse_from(["ohimark", "--out", "site", "dir"]).unwrap();
        assert_eq!(before.dir, std::path::PathBuf::from("dir"));
        assert_eq!(before.out, std::path::PathBuf::from("site"));

        let after = Args::try_parse_from(["ohimark", "dir", "--out", "site"]).unwrap();
        assert_eq!(after.dir, std::path::PathBuf::from("dir"));
        assert_eq!(after.out, std::path::PathBuf::from("site"));
    }

    #[test]
    fn r9_6_exclude_accumulates() {
        let args = Args::try_parse_from([
            "ohimark",
            "dir",
            "--out",
            "site",
            "--exclude",
            "a/*",
            "--exclude",
            "b/*",
        ])
        .unwrap();
        assert_eq!(args.excludes, vec!["a/*".to_string(), "b/*".to_string()]);
    }

    #[test]
    fn r9_6_last_root_and_out_win() {
        let args = Args::try_parse_from([
            "ohimark", "dir", "--out", "first", "--out", "second", "--root", "a.md", "--root",
            "b.md",
        ])
        .unwrap();
        assert_eq!(args.out, std::path::PathBuf::from("second"));
        assert_eq!(args.root, std::path::PathBuf::from("b.md"));
    }

    #[test]
    fn r9_default_root_is_readme() {
        let args = Args::try_parse_from(["ohimark", "dir", "--out", "site"]).unwrap();
        assert_eq!(args.root, std::path::PathBuf::from("README.md"));
    }

    #[test]
    fn r2_1_root_escaping_dir_is_rejected() {
        assert!(resolve_root(Utf8Path::new("../outside.md")).is_err());
        assert!(resolve_root(Utf8Path::new("a/../../outside.md")).is_err());
    }

    #[test]
    fn r2_1_absolute_root_is_rejected() {
        assert!(resolve_root(Utf8Path::new("/etc/passwd")).is_err());
    }

    #[test]
    fn r2_1_root_within_dir_is_normalised() {
        let resolved = resolve_root(Utf8Path::new("docs/./home.md")).unwrap();
        assert_eq!(resolved.as_str(), "docs/home.md");
    }

    #[test]
    fn out_inside_dir_is_detected() {
        let dir = Utf8Path::new("/a/b");
        let out = Utf8Path::new("/a/b/site");
        assert_eq!(
            compute_out_inside_dir(out, dir),
            Some(Utf8PathBuf::from("site"))
        );
        assert_eq!(compute_out_inside_dir(Utf8Path::new("/a/site"), dir), None);
    }

    #[test]
    fn out_swallowing_dir_is_rejected() {
        let dir = Utf8Path::new("/a/b");
        assert!(reject_out_swallows_dir(Utf8Path::new("/a"), dir).is_err());
        assert!(reject_out_swallows_dir(Utf8Path::new("/a/b"), dir).is_err());
        assert!(reject_out_swallows_dir(Utf8Path::new("/a/b/site"), dir).is_ok());
    }

    #[test]
    fn normalize_absolute_collapses_dot_and_dotdot() {
        assert_eq!(
            normalize_absolute(Utf8Path::new("/a/./b/../c")),
            Utf8PathBuf::from("/a/c")
        );
    }
}
