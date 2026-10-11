//! Optional stylesheet selection, copying, page links, and output reservations
//! (R5.5-R5.7, R8.11, R9.6-R9.7, R10).

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use tempfile::TempDir;

const CSS: &[u8] = b"/* custom theme */\r\n:root { --ohm-bg: #f5f0e8; }\r\n";

fn build_with_stylesheet(
    dir: &Path,
    out: &Path,
    stylesheet: &Path,
    extra: &[&str],
) -> common::RunResult {
    let mut args = vec![
        "--out",
        out.to_str().unwrap(),
        "--stylesheet",
        stylesheet.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);
    args.push(dir.to_str().unwrap());
    common::run(args)
}

fn stylesheet_hrefs(html: &str) -> Vec<&str> {
    html.split("<link ")
        .skip(1)
        .filter_map(|rest| {
            let tag = rest.split('>').next().unwrap();
            if common::attr_values(tag, "rel") == ["stylesheet"] {
                common::attr_values(tag, "href").first().copied()
            } else {
                None
            }
        })
        .collect()
}

fn assert_custom_links(html: &str, prefix: &str) {
    let default = format!("{prefix}assets/ohimark.css");
    let custom = format!("{prefix}assets/custom.css");
    assert_eq!(stylesheet_hrefs(html), [default.as_str(), custom.as_str()]);

    let default_position = html.find(&format!("href=\"{default}\"")).unwrap();
    let rest = &html[default_position..];
    let following = rest[rest.find('>').unwrap() + 1..].trim_start();
    assert!(
        following.starts_with("<link "),
        "the custom stylesheet must immediately follow the bundled stylesheet"
    );
    let next_tag = following.split('>').next().unwrap();
    assert_eq!(common::attr_values(next_tag, "rel"), ["stylesheet"]);
    assert_eq!(common::attr_values(next_tag, "href"), [custom.as_str()]);
}

fn source_dir(base: &TempDir) -> PathBuf {
    let dir = base.path().join("sources");
    std::fs::create_dir(&dir).expect("create sources");
    dir
}

/// R8.11: without the option, pages link only the bundled minimal base stylesheet.
#[test]
fn r8_11_without_stylesheet_has_only_base_assets() {
    let base = TempDir::new().expect("create base");
    let dir = source_dir(&base);
    common::write_tree(
        &dir,
        &[("README.md", "# Home\n"), ("docs/guide.md", "# Guide\n")],
    );
    let out = base.path().join("site");
    let result = common::run([dir.to_str().unwrap(), "--out", out.to_str().unwrap()]);
    assert!(result.success(), "build failed: {}", result.stderr);

    assert_eq!(
        std::fs::read(out.join("assets/ohimark.css")).unwrap(),
        ohimark::theme::STYLE_CSS.as_bytes()
    );
    assert_eq!(
        std::fs::read(out.join("assets/ohimark.js")).unwrap(),
        ohimark::theme::APP_JS.as_bytes()
    );
    assert!(!out.join("assets/custom.css").exists());
    for (page, href) in [
        ("index.html", "assets/ohimark.css"),
        ("docs/guide.html", "../assets/ohimark.css"),
    ] {
        assert_eq!(
            stylesheet_hrefs(&common::read_built(&out.join(page))),
            [href]
        );
    }
}

/// R5.5, R8.11, R10.1: bytes survive unchanged, and every output depth (including
/// a relocated home and a synthetic TODO page) loads the override after the base.
#[test]
fn r5_5_and_r8_11_exact_copy_and_ordered_relative_links_on_every_page() {
    for root in ["README.md", "docs/home.md"] {
        let base = TempDir::new().expect("create base");
        let dir = source_dir(&base);
        common::write_tree(
            &dir,
            &[
                (root, "# Home\n\n- [ ] home task\n"),
                ("docs/guide.md", "# Guide\n"),
                ("docs/deeper/topic.md", "# Topic\n\n- [ ] nested task\n"),
            ],
        );
        let stylesheet = base.path().join("theme.css");
        std::fs::write(&stylesheet, CSS).unwrap();
        let before = common::snapshot_dir(&dir);
        let out = base.path().join("site");
        let result = build_with_stylesheet(&dir, &out, &stylesheet, &["--root", root, "--todo"]);
        assert!(result.success(), "root {root}: {}", result.stderr);

        assert_eq!(std::fs::read(out.join("assets/custom.css")).unwrap(), CSS);
        assert_eq!(
            std::fs::read(out.join("assets/ohimark.css")).unwrap(),
            ohimark::theme::STYLE_CSS.as_bytes()
        );
        assert_eq!(
            std::fs::read(out.join("assets/ohimark.js")).unwrap(),
            ohimark::theme::APP_JS.as_bytes()
        );
        let pages: Vec<_> = common::relative_paths(&out)
            .into_iter()
            .filter(|rel| rel.ends_with(".html"))
            .collect();
        assert_eq!(
            pages,
            [
                "TODO.html",
                "docs/deeper/topic.html",
                "docs/guide.html",
                "index.html"
            ]
        );
        for page in pages {
            let html = common::read_built(&out.join(&page));
            assert_custom_links(&html, &"../".repeat(page.matches('/').count()));
            for path in [&dir, &out, &stylesheet] {
                assert!(
                    !html.contains(path.to_str().unwrap()),
                    "{page} leaks absolute path {}",
                    path.display()
                );
            }
            for attr in ["href", "src"] {
                for value in common::attr_values(&html, attr) {
                    assert!(!value.starts_with('/'), "{page}: absolute {attr} {value}");
                }
            }
        }
        assert_eq!(common::snapshot_dir(&dir), before, "sources changed");
        assert_eq!(
            std::fs::read(&stylesheet).unwrap(),
            CSS,
            "CSS source changed"
        );
    }
}

/// R9.7: stylesheet paths resolve against the subprocess working directory,
/// can be outside DIR, and accept spaces and an arbitrary extension.
#[test]
fn r9_7_stylesheet_accepts_cwd_relative_and_absolute_paths_outside_dir() {
    let base = TempDir::new().expect("create base");
    let dir = source_dir(&base);
    common::write_tree(&dir, &[("README.md", "# Home\n")]);
    let cwd = base.path().join("working directory");
    std::fs::create_dir(&cwd).unwrap();
    let stylesheet = cwd.join("user theme.skin");
    std::fs::write(&stylesheet, CSS).unwrap();
    let before = common::snapshot_dir(&dir);

    for (selected, out_arg) in [
        ("user theme.skin", "../relative-site"),
        (stylesheet.to_str().unwrap(), "../absolute-site"),
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_ohimark"))
            .current_dir(&cwd)
            .args(["../sources", "--stylesheet", selected, "--out", out_arg])
            .output()
            .expect("run ohimark from fixture working directory");
        assert!(
            result.status.success(),
            "build failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let out = cwd.join(out_arg);
        assert_eq!(std::fs::read(out.join("assets/custom.css")).unwrap(), CSS);
        assert_custom_links(&common::read_built(&out.join("index.html")), "");
    }
    assert_eq!(common::snapshot_dir(&dir), before, "sources changed");
    assert_eq!(std::fs::read(stylesheet).unwrap(), CSS);
}

/// R5.5: selecting a stylesheet explicitly bypasses discovery filters and
/// does not require a reference from Markdown.
#[test]
fn r5_5_explicit_ignored_excluded_and_unreferenced_stylesheets_are_copied() {
    let base = TempDir::new().expect("create base");
    let dir = source_dir(&base);
    common::write_tree(
        &dir,
        &[("README.md", "# Home\n"), (".gitignore", "ignored.css\n")],
    );
    for name in ["ignored.css", "excluded.css", "unreferenced.css"] {
        common::write_file(&dir, name, CSS);
    }
    let before = common::snapshot_dir(&dir);
    for name in ["ignored.css", "excluded.css", "unreferenced.css"] {
        let out = base.path().join(format!("site-{name}"));
        let result =
            build_with_stylesheet(&dir, &out, &dir.join(name), &["--exclude", "excluded.css"]);
        assert!(result.success(), "selected {name}: {}", result.stderr);
        assert_eq!(std::fs::read(out.join("assets/custom.css")).unwrap(), CSS);
        for source in ["ignored.css", "excluded.css", "unreferenced.css"] {
            assert!(!out.join(source).exists(), "unreferenced {source} copied");
        }
    }
    assert_eq!(common::snapshot_dir(&dir), before, "sources changed");
}

/// R9.6: only the last scalar value is validated and copied.
#[test]
fn r9_6_last_stylesheet_wins_even_when_first_path_is_missing() {
    let base = TempDir::new().expect("create base");
    let dir = source_dir(&base);
    common::write_tree(&dir, &[("README.md", "# Home\n")]);
    let stylesheet = base.path().join("theme.css");
    std::fs::write(&stylesheet, CSS).unwrap();
    let missing = base.path().join("missing.css");
    let out = base.path().join("site");
    let result = common::run([
        dir.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--stylesheet",
        missing.to_str().unwrap(),
        "--stylesheet",
        stylesheet.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);
    assert_eq!(std::fs::read(out.join("assets/custom.css")).unwrap(), CSS);
    assert!(!missing.exists());
}

/// R9.7, R10: missing files and directories fail configuration without output.
#[test]
fn r9_7_invalid_stylesheet_paths_name_the_option_and_path() {
    let base = TempDir::new().expect("create base");
    let dir = source_dir(&base);
    common::write_tree(&dir, &[("README.md", "# Home\n")]);
    let before = common::snapshot_dir(&dir);
    let directory = base.path().join("theme-directory");
    std::fs::create_dir(&directory).unwrap();
    for (index, stylesheet) in [base.path().join("missing.css"), directory]
        .into_iter()
        .enumerate()
    {
        let out = base.path().join(format!("site-{index}"));
        if index == 1 {
            std::fs::create_dir(&out).unwrap();
        }
        let result = build_with_stylesheet(&dir, &out, &stylesheet, &[]);
        assert!(!result.success(), "invalid stylesheet accepted");
        assert!(
            result.stderr.contains("configuration:"),
            "{}",
            result.stderr
        );
        assert!(result.stderr.contains("--stylesheet"), "{}", result.stderr);
        assert!(
            result.stderr.contains(stylesheet.to_str().unwrap()),
            "{}",
            result.stderr
        );
        assert!(common::is_absent_or_empty(&out), "partial output survived");
        assert_eq!(common::snapshot_dir(&dir), before, "sources changed");
    }
}

/// R5.5, R9.7, R10: after configuration canonicalizes the stylesheet, a
/// disappeared file is a fatal output error and never changes the sources.
#[test]
fn r5_5_stylesheet_disappearing_after_validation_is_an_output_error() {
    let base = TempDir::new().expect("create base");
    let dir = source_dir(&base);
    common::write_tree(&dir, &[("README.md", "# Home\n")]);
    let stylesheet = dir.join("theme.css");
    std::fs::write(&stylesheet, CSS).unwrap();
    let out = base.path().join("site");
    let config = ohimark::Args::try_parse_from([
        "ohimark",
        dir.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--stylesheet",
        stylesheet.to_str().unwrap(),
    ])
    .unwrap()
    .into_config()
    .unwrap();
    let canonical = std::fs::canonicalize(&stylesheet).unwrap();
    assert_eq!(config.stylesheet.as_ref().unwrap().as_std_path(), canonical);
    std::fs::remove_file(&stylesheet).unwrap();
    let before = common::snapshot_dir(&dir);

    let error = ohimark::build(&config).expect_err("removed stylesheet must fail");
    assert!(matches!(&error, ohimark::Error::Emit(_)), "{error}");
    let message = error.to_string();
    assert!(message.contains("output:"), "{message}");
    assert!(message.contains("--stylesheet"), "{message}");
    assert!(message.contains(canonical.to_str().unwrap()), "{message}");
    assert!(common::is_absent_or_empty(&out), "partial output survived");
    assert_eq!(common::snapshot_dir(&dir), before, "sources changed");
}

/// R5.6: CSS imports and URLs are preserved verbatim; only Markdown's
/// referenced assets are copied, independently of CSS-only dependencies.
#[test]
fn r5_6_stylesheet_dependencies_are_neither_rewritten_nor_copied() {
    let base = TempDir::new().expect("create base");
    let dir = source_dir(&base);
    common::write_tree(
        &dir,
        &[
            ("README.md", "# Home\n\n![Markdown image](markdown.png)\n"),
            ("other.css", "body { color: blue; }\n"),
        ],
    );
    let css = b"@import \"other.css\";\r\nbody { background: url('picture.png'); }\r\n@font-face { src: url(font.woff2); }\r\n.remote { background: url('https://example.invalid/remote.png'); }\r\n";
    common::write_file(&dir, "theme.css", css);
    common::write_file(&dir, "picture.png", b"CSS-only image");
    common::write_file(&dir, "font.woff2", b"CSS-only font");
    common::write_file(&dir, "markdown.png", b"Markdown image");
    let before = common::snapshot_dir(&dir);
    let out = base.path().join("site");
    let result = build_with_stylesheet(&dir, &out, &dir.join("theme.css"), &[]);
    assert!(result.success(), "build failed: {}", result.stderr);
    assert_eq!(std::fs::read(out.join("assets/custom.css")).unwrap(), css);
    assert_eq!(
        std::fs::read(out.join("markdown.png")).unwrap(),
        b"Markdown image"
    );
    assert_eq!(
        common::relative_paths(&out),
        [
            "assets/custom.css",
            "assets/ohimark.css",
            "assets/ohimark.js",
            "index.html",
            "markdown.png"
        ],
        "CSS-only dependencies must not be copied anywhere in the output"
    );
    assert_eq!(common::snapshot_dir(&dir), before, "sources changed");
}

/// R5.7, R10: casefolded equality and component-prefix conflicts against
/// referenced assets and emitted pages fail before any output survives.
#[test]
fn r5_7_reserved_stylesheet_conflicts_fail_cleanly() {
    for (index, (source, emitted, home)) in [
        (
            "Assets/CUSTOM.CSS",
            "Assets/CUSTOM.CSS",
            "# Home\n\n[Asset](Assets/CUSTOM.CSS)\n",
        ),
        (
            "assets/custom.css/icons/logo.bin",
            "assets/custom.css/icons/logo.bin",
            "# Home\n\n[Asset](assets/custom.css/icons/logo.bin)\n",
        ),
        (
            "assets/CUSTOM.css/guide.md",
            "assets/CUSTOM.css/guide.html",
            "# Home\n",
        ),
        ("assets", "assets", "# Home\n\n[Asset](assets)\n"),
    ]
    .into_iter()
    .enumerate()
    {
        let base = TempDir::new().expect("create base");
        let dir = source_dir(&base);
        common::write_tree(&dir, &[("README.md", home), (source, "# Collision\n")]);
        let stylesheet = base.path().join("theme.css");
        std::fs::write(&stylesheet, CSS).unwrap();
        let before = common::snapshot_dir(&dir);
        let out = base.path().join("site");
        if index % 2 == 1 {
            std::fs::create_dir(&out).unwrap();
        }
        let tmp_home = TempDir::new().expect("create private tmp home");
        let result = common::run_with_envs(
            [
                dir.to_str().unwrap(),
                "--out",
                out.to_str().unwrap(),
                "--stylesheet",
                stylesheet.to_str().unwrap(),
            ],
            &[("TMPDIR", tmp_home.path().to_str().unwrap())],
        );
        assert!(!result.success(), "collision {source} accepted");
        for expected in ["output:", "--stylesheet", "assets/custom.css", emitted] {
            assert!(result.stderr.contains(expected), "{}", result.stderr);
        }
        assert!(
            result.stderr.contains("theme.css"),
            "stylesheet path absent: {}",
            result.stderr
        );
        assert!(common::is_absent_or_empty(&out), "partial output survived");
        assert!(
            common::is_absent_or_empty(tmp_home.path()),
            "staging directory survived"
        );
        assert_eq!(common::snapshot_dir(&dir), before, "sources changed");
        assert_eq!(std::fs::read(stylesheet).unwrap(), CSS);
    }
}

/// R5.7: an unreferenced non-Markdown asset does not reserve an output target, and
/// similarly prefixed names separated without a slash remain valid.
#[test]
fn r5_7_unreferenced_conflicts_and_component_prefix_near_matches_are_allowed() {
    for unreferenced in ["assets/CUSTOM.CSS", "assets/CUSTOM.css/unused.bin"] {
        let base = TempDir::new().expect("create base");
        let dir = source_dir(&base);
        common::write_tree(
            &dir,
            &[
                (
                    "README.md",
                    "# Home\n\n[Asset](assets/custom.css-other.bin)\n",
                ),
                ("assets/custom.css-other/guide.md", "# Guide\n"),
            ],
        );
        common::write_file(&dir, unreferenced, b"unreferenced collision");
        common::write_file(&dir, "assets/custom.css-other.bin", b"referenced asset");
        let stylesheet = base.path().join("theme.css");
        std::fs::write(&stylesheet, CSS).unwrap();
        let before = common::snapshot_dir(&dir);
        let out = base.path().join("site");
        let result = build_with_stylesheet(&dir, &out, &stylesheet, &[]);
        assert!(result.success(), "{unreferenced}: {}", result.stderr);
        assert_eq!(std::fs::read(out.join("assets/custom.css")).unwrap(), CSS);
        assert_eq!(
            std::fs::read(out.join("assets/custom.css-other.bin")).unwrap(),
            b"referenced asset"
        );
        assert!(out.join("assets/custom.css-other/guide.html").is_file());
        assert!(
            !common::relative_paths(&out)
                .iter()
                .any(|p| p == unreferenced),
            "unreferenced source copied"
        );
        assert_eq!(common::snapshot_dir(&dir), before, "sources changed");
    }
}

/// R5.7, R8.11: the custom target is reserved only with the option present.
#[test]
fn r5_7_without_option_custom_named_assets_and_pages_are_allowed() {
    for (source, emitted, home) in [
        (
            "assets/custom.css",
            "assets/custom.css",
            "# Home\n\n[Asset](assets/custom.css)\n",
        ),
        (
            "assets/custom.css/icons/logo.bin",
            "assets/custom.css/icons/logo.bin",
            "# Home\n\n[Asset](assets/custom.css/icons/logo.bin)\n",
        ),
        (
            "assets/custom.css/guide.md",
            "assets/custom.css/guide.html",
            "# Home\n",
        ),
    ] {
        let base = TempDir::new().expect("create base");
        let dir = source_dir(&base);
        common::write_tree(&dir, &[("README.md", home), (source, "# Custom source\n")]);
        let out = base.path().join("site");
        let result = common::run([dir.to_str().unwrap(), "--out", out.to_str().unwrap()]);
        assert!(result.success(), "{source}: {}", result.stderr);
        assert!(out.join(emitted).is_file(), "{emitted} absent");
        assert_eq!(
            stylesheet_hrefs(&common::read_built(&out.join("index.html"))),
            ["assets/ohimark.css"]
        );
        if !source.ends_with(".md") {
            assert_eq!(
                std::fs::read(out.join(emitted)).unwrap(),
                b"# Custom source\n"
            );
        }
    }
}
