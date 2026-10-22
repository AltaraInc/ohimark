//! `clap` argument definitions and validation into a `Config` (R9).

use camino::{Utf8Path, Utf8PathBuf};
use clap::{ArgAction, Parser};

use crate::model::{Error, Result, TargetPath};

/// Maximum visible immediate rows per sidebar directory group by default.
pub const DEFAULT_MAX_GROUP_SIZE: u32 = 10;

/// A built-in stylesheet layered over the site's minimal base styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum PresetTheme {
    /// A white-and-charcoal palette with blue accents.
    Default,
    /// Warm paper, serif typography, and red accents.
    Bookish,
}

/// The raw command-line surface:
/// `ohimark --out OUTDIR [--root FILENAME] [--max-group-size N] [--theme default|bookish] [--stylesheet FILE] [--endpoint TITLE PATH]... [--exclude GLOB]... [--todo] DIR` (R9.1).
///
/// Fields accept arbitrary OS strings (not `Utf8PathBuf`) so that a non-UTF-8
/// argument surfaces as a clear `Error::Config` from [`Args::into_config`]
/// rather than an opaque `clap` parse failure.
#[derive(Debug, Parser)]
#[command(
    name = "ohimark",
    version,
    about = "Generate a static HTML site from a directory of Markdown.",
    // R9.6: for scalar options, a repeated occurrence overrides the
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

    /// Maximum visible immediate rows per sidebar directory group before scrolling.
    #[arg(
        long = "max-group-size",
        value_name = "N",
        default_value_t = DEFAULT_MAX_GROUP_SIZE,
        value_parser = clap::value_parser!(u32).range(1..),
        help = "Maximum visible immediate rows per sidebar directory group before scrolling"
    )]
    pub max_group_size: u32,

    /// An optional built-in theme, loaded after the minimal base styles.
    #[arg(
        long = "theme",
        value_name = "THEME",
        value_enum,
        help = "Built-in theme to load after the base styles (none selected by default)"
    )]
    pub theme: Option<PresetTheme>,

    /// An optional stylesheet, relative to the working directory or absolute.
    /// Loaded after the base and selected theme so its rules can override them.
    #[arg(
        long = "stylesheet",
        value_name = "FILE",
        help = "Custom stylesheet to load after the base and any selected theme (relative to the working directory or absolute)"
    )]
    pub stylesheet: Option<std::path::PathBuf>,

    /// Additional files copied unchanged and linked from the main sidebar.
    #[arg(
        long = "endpoint",
        value_names = ["TITLE", "PATH"],
        num_args = 2,
        action = ArgAction::Append,
        help = "Add a sidebar link with TITLE and copy the local HTML PATH unchanged (repeatable)"
    )]
    pub html_pages: Vec<std::ffi::OsString>,

    /// A glob to exclude from discovery, matched against `DIR`-relative paths.
    /// Repeatable; every occurrence accumulates (R9.6).
    #[arg(
        long = "exclude",
        value_name = "GLOB",
        action = ArgAction::Append,
        help = "Glob to exclude from discovery, relative to DIR (repeatable)"
    )]
    pub excludes: Vec<String>,

    /// Collect every unchecked task-list item into the root-level TODO page.
    /// Off by default (R12.1).
    #[arg(
        long = "todo",
        action = ArgAction::SetTrue,
        help = "Collect every unchecked task-list item into the root TODO page, in the HTML output only"
    )]
    pub todo: bool,
}

/// An explicitly selected HTML page, copied without rendering or rewriting.
#[derive(Clone, Debug)]
pub struct HtmlPage {
    /// The label displayed in the main sidebar.
    pub title: String,
    /// The input file, canonicalised to an absolute path.
    pub source: Utf8PathBuf,
    /// The file's path within the emitted site.
    pub target: TargetPath,
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
    /// Maximum visible immediate rows per sidebar directory group before scrolling.
    pub max_group_size: u32,
    /// The optional `--theme` preset; `None` selects no built-in preset.
    pub theme: Option<PresetTheme>,
    /// The optional `--stylesheet` file, canonicalised to an absolute path.
    pub stylesheet: Option<Utf8PathBuf>,
    /// Explicit HTML pages, in command-line order.
    pub html_pages: Vec<HtmlPage>,
    /// Repeatable `--exclude` globs, in the order given.
    pub excludes: Vec<String>,
    /// `Some(rel)` when `out` lies inside `dir`; the `DIR`-relative path of the
    /// output directory, which discovery must skip (R1.9).
    pub out_inside_dir: Option<Utf8PathBuf>,
    /// `--todo`: collect open tasks into the root-level TODO page (R12).
    pub todo: bool,
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
        let stylesheet = self
            .stylesheet
            .as_deref()
            .map(validate_stylesheet)
            .transpose()?;
        let html_pages = validate_html_pages(self.html_pages, &dir)?;

        Ok(Config {
            dir,
            out,
            root,
            max_group_size: self.max_group_size,
            theme: self.theme,
            stylesheet,
            html_pages,
            excludes: self.excludes,
            out_inside_dir,
            todo: self.todo,
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

/// An explicitly selected stylesheet can live anywhere, independently of discovery.
fn validate_stylesheet(path: &std::path::Path) -> Result<Utf8PathBuf> {
    validate_selected_file(path, "--stylesheet")
}

/// Explicit files are independent of discovery and must resolve to regular files.
fn validate_selected_file(path: &std::path::Path, option: &str) -> Result<Utf8PathBuf> {
    let path = to_utf8(path, option)?;
    let absolute = resolve_against_cwd(path.clone()).map_err(|source| {
        Error::Config(format!("{option} `{path}` could not be resolved: {source}"))
    })?;
    let metadata = std::fs::metadata(&absolute).map_err(|source| {
        Error::Config(format!("{option} `{path}` could not be read: {source}"))
    })?;
    if !metadata.is_file() {
        return Err(Error::Config(format!(
            "{option} `{path}` is not a regular file"
        )));
    }
    let canonical = std::fs::canonicalize(&absolute).map_err(|source| {
        Error::Config(format!("{option} `{path}` could not be resolved: {source}"))
    })?;
    Utf8PathBuf::from_path_buf(canonical).map_err(|canonical| {
        Error::Config(format!(
            "{option} `{path}` resolves to `{}`, which is not valid UTF-8",
            canonical.display()
        ))
    })
}

fn validate_html_pages(values: Vec<std::ffi::OsString>, dir: &Utf8Path) -> Result<Vec<HtmlPage>> {
    let (pairs, remainder) = values.as_chunks::<2>();
    let mut pages = Vec::with_capacity(pairs.len());
    for pair in pairs {
        let path = std::path::Path::new(&pair[1]);
        let title = pair[0].to_str().ok_or_else(|| {
            Error::Config(format!(
                "--endpoint `{}` has a title that is not valid UTF-8",
                path.display()
            ))
        })?;
        if title.trim().is_empty() {
            return Err(Error::Config(format!(
                "--endpoint `{}` requires a nonempty title",
                path.display()
            )));
        }
        let supplied = to_utf8(path, "--endpoint")?;
        let source = validate_selected_file(path, "--endpoint")?;
        let target = match source.strip_prefix(dir) {
            Ok(relative) => relative.to_path_buf(),
            Err(_) => Utf8PathBuf::from(supplied.file_name().ok_or_else(|| {
                Error::Config(format!("--endpoint `{supplied}` has no filename"))
            })?),
        };
        pages.push(HtmlPage {
            title: title.to_owned(),
            source,
            target: TargetPath::new(target),
        });
    }
    if !remainder.is_empty() {
        return Err(Error::Config(
            "--endpoint requires both TITLE and PATH".to_owned(),
        ));
    }
    Ok(pages)
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
    fn endpoint_is_optional_and_repeatable() {
        use std::ffi::OsString;

        let args = Args::try_parse_from(["ohimark", "dir", "--out", "site"]).unwrap();
        assert!(args.html_pages.is_empty());

        for arguments in [
            vec![
                "ohimark",
                "--endpoint",
                "Report",
                "report.html",
                "dir",
                "--out",
                "site",
            ],
            vec![
                "ohimark",
                "dir",
                "--out",
                "site",
                "--endpoint",
                "Report",
                "report.html",
            ],
        ] {
            let args = Args::try_parse_from(arguments).unwrap();
            assert_eq!(
                args.html_pages,
                ["Report", "report.html"].map(OsString::from)
            );
        }

        let args = Args::try_parse_from([
            "ohimark",
            "--endpoint",
            "First",
            "first.html",
            "dir",
            "--endpoint",
            "Second",
            "second.html",
            "--out",
            "site",
            "--endpoint",
            "Third",
            "third.html",
        ])
        .unwrap();
        assert_eq!(
            args.html_pages,
            [
                "First",
                "first.html",
                "Second",
                "second.html",
                "Third",
                "third.html"
            ]
            .map(OsString::from)
        );

        for arguments in [
            vec!["ohimark", "dir", "--out", "site", "--endpoint"],
            vec!["ohimark", "dir", "--out", "site", "--endpoint", "Title"],
            vec!["ohimark", "--endpoint", "Title", "--out", "site", "dir"],
        ] {
            assert!(Args::try_parse_from(arguments).is_err());
        }
    }

    #[test]
    fn stylesheet_is_optional_and_last_value_wins_in_either_position() {
        let default = Args::try_parse_from(["ohimark", "dir", "--out", "site"]).unwrap();
        assert_eq!(default.stylesheet, None);
        assert_eq!(default.theme, None);
        for arguments in [
            vec![
                "ohimark",
                "--stylesheet",
                "theme.css",
                "dir",
                "--out",
                "site",
            ],
            vec![
                "ohimark",
                "dir",
                "--out",
                "site",
                "--stylesheet",
                "theme.css",
            ],
            vec![
                "ohimark",
                "--stylesheet",
                "old.css",
                "dir",
                "--out",
                "site",
                "--stylesheet",
                "theme.css",
            ],
        ] {
            let args = Args::try_parse_from(arguments).unwrap();
            assert_eq!(args.stylesheet, Some(std::path::PathBuf::from("theme.css")));
        }
    }

    #[test]
    fn preset_theme_accepts_known_values_and_last_value_with_custom_stylesheet() {
        for (name, preset) in [
            ("default", PresetTheme::Default),
            ("bookish", PresetTheme::Bookish),
        ] {
            let args =
                Args::try_parse_from(["ohimark", "--theme", name, "dir", "--out", "site"]).unwrap();
            assert_eq!(args.theme, Some(preset));
        }
        let args = Args::try_parse_from([
            "ohimark",
            "--theme",
            "default",
            "dir",
            "--out",
            "site",
            "--theme",
            "bookish",
            "--stylesheet",
            "overrides.css",
        ])
        .unwrap();
        assert_eq!(args.theme, Some(PresetTheme::Bookish));
        assert_eq!(args.stylesheet, Some("overrides.css".into()));
        assert!(
            Args::try_parse_from(["ohimark", "dir", "--out", "site", "--theme", "unknown",])
                .is_err()
        );
    }

    #[test]
    fn stylesheet_relative_to_cwd_is_canonicalized_without_changing_cwd() {
        let cwd = std::env::current_dir().unwrap();
        let fixture = tempfile::tempdir_in(&cwd).unwrap();
        let stylesheet = fixture.path().join("theme");
        std::fs::write(&stylesheet, b":root { --ohm-accent: blue; }").unwrap();
        let relative = stylesheet.strip_prefix(&cwd).unwrap();
        let validated = validate_stylesheet(relative).unwrap();
        assert_eq!(
            validated.as_std_path(),
            std::fs::canonicalize(&stylesheet).unwrap()
        );
        assert_eq!(std::env::current_dir().unwrap(), cwd);
    }

    #[test]
    fn absolute_stylesheet_outside_dir_is_independent_of_exclusions() {
        let fixture = tempfile::tempdir().unwrap();
        let dir = fixture.path().join("docs");
        std::fs::create_dir(&dir).unwrap();
        let stylesheet = fixture.path().join("theme");
        std::fs::write(&stylesheet, b"body { color: purple; }").unwrap();
        let args = Args {
            dir,
            out: fixture.path().join("site"),
            root: "README.md".into(),
            max_group_size: DEFAULT_MAX_GROUP_SIZE,
            theme: Some(PresetTheme::Bookish),
            stylesheet: Some(stylesheet.clone()),
            html_pages: Vec::new(),
            excludes: vec!["**".into()],
            todo: false,
        };
        let config = args.into_config().unwrap();
        assert_eq!(config.theme, Some(PresetTheme::Bookish));
        let validated = config.stylesheet.unwrap();
        assert_eq!(
            validated.as_std_path(),
            std::fs::canonicalize(stylesheet).unwrap()
        );
        assert!(!validated.starts_with(config.dir));
    }

    #[test]
    fn stylesheet_missing_or_non_regular_paths_name_the_option_and_path() {
        let fixture = tempfile::tempdir().unwrap();
        let missing = fixture.path().join("missing.css");
        for path in [missing.as_path(), fixture.path()] {
            let error = validate_stylesheet(path).unwrap_err().to_string();
            assert!(error.starts_with("configuration:"), "{error}");
            assert!(error.contains("--stylesheet"), "{error}");
            assert!(error.contains(path.to_str().unwrap()), "{error}");
        }
        let error = validate_stylesheet(fixture.path()).unwrap_err().to_string();
        assert!(error.contains("not a regular file"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn stylesheet_requires_utf8_for_argument() {
        use std::os::unix::ffi::OsStringExt;

        let fixture = tempfile::tempdir().unwrap();
        let non_utf8 = fixture
            .path()
            .join(std::ffi::OsString::from_vec(b"theme-\xff.css".to_vec()));
        let error = validate_stylesheet(&non_utf8).unwrap_err().to_string();
        assert!(error.contains("--stylesheet"), "{error}");
        assert!(error.contains("not valid UTF-8"), "{error}");
    }

    // The macOS sandbox cannot create the non-UTF-8 filename this fixture needs.
    #[cfg(target_os = "linux")]
    #[test]
    fn stylesheet_requires_utf8_for_canonical_target() {
        use std::os::unix::ffi::OsStringExt;
        use std::os::unix::fs::symlink;

        let fixture = tempfile::tempdir().unwrap();
        let non_utf8 = fixture
            .path()
            .join(std::ffi::OsString::from_vec(b"theme-\xff.css".to_vec()));
        std::fs::write(&non_utf8, b"body {}").unwrap();
        let link = fixture.path().join("theme.css");
        symlink(&non_utf8, &link).unwrap();
        let error = validate_stylesheet(&link).unwrap_err().to_string();
        assert!(error.contains(link.to_str().unwrap()), "{error}");
        assert!(error.contains("resolves to"), "{error}");
        assert!(error.contains("not valid UTF-8"), "{error}");
    }

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
    fn r12_1_todo_is_off_by_default() {
        let args = Args::try_parse_from(["ohimark", "dir", "--out", "site"]).unwrap();
        assert!(!args.todo);
    }

    #[test]
    fn r12_1_todo_flag_turns_collection_on() {
        let args = Args::try_parse_from(["ohimark", "--todo", "dir", "--out", "site"]).unwrap();
        assert!(args.todo);
        let after = Args::try_parse_from(["ohimark", "dir", "--out", "site", "--todo"]).unwrap();
        assert!(after.todo);
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
