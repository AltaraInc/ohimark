//! Built-in presets, stylesheet layering, and conditional output reservations.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

const DEFAULT: &[u8] = include_bytes!("../themes/default.css");
const BOOKISH: &[u8] = include_bytes!("../themes/bookish.css");
const CUSTOM: &[u8] = b"/* user override */\r\nbody { color: red; }\xff\n";

fn source_dir(base: &TempDir) -> PathBuf {
    let dir = base.path().join("sources");
    std::fs::create_dir(&dir).unwrap();
    dir
}

fn build(dir: &Path, out: &Path, extra: &[&str]) -> common::RunResult {
    let mut args = vec![dir.to_str().unwrap(), "--out", out.to_str().unwrap()];
    args.extend_from_slice(extra);
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

fn assert_links(html: &str, prefix: &str, theme: bool, custom: bool) {
    let mut expected = vec![format!("{prefix}assets/ohimark.css")];
    if theme {
        expected.push(format!("{prefix}assets/theme.css"));
    }
    if custom {
        expected.push(format!("{prefix}assets/custom.css"));
    }
    assert_eq!(stylesheet_hrefs(html), expected);
}

#[test]
fn standalone_executable_embeds_both_presets_without_runtime_theme_files() {
    let base = TempDir::new().unwrap();
    let dir = source_dir(&base);
    common::write_tree(&dir, &[("README.md", "# Home\n")]);
    let before = common::snapshot_dir(&dir);
    let cwd = base.path().join("unrelated working directory");
    std::fs::create_dir(&cwd).unwrap();
    let executable = base.path().join("standalone-ohimark");
    std::fs::copy(env!("CARGO_BIN_EXE_ohimark"), &executable).unwrap();

    for (name, expected, before_dir) in [("default", DEFAULT, true), ("bookish", BOOKISH, false)] {
        let out = base.path().join(name);
        let mut args = vec!["--out", out.to_str().unwrap()];
        if before_dir {
            args.extend(["--theme", name, dir.to_str().unwrap()]);
        } else {
            args.extend([dir.to_str().unwrap(), "--theme", name]);
        }
        let result = Command::new(&executable)
            .current_dir(&cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            std::fs::read(out.join("assets/theme.css")).unwrap(),
            expected
        );
        assert_eq!(
            common::relative_paths(&out),
            [
                "assets/ohimark.css",
                "assets/ohimark.js",
                "assets/theme.css",
                "index.html"
            ]
        );
        assert_links(
            &common::read_built(&out.join("index.html")),
            "",
            true,
            false,
        );
    }
    assert!(common::is_absent_or_empty(&cwd));
    assert!(!base.path().join("themes").exists());
    assert_eq!(common::snapshot_dir(&dir), before);
}

#[test]
fn preset_then_custom_links_are_relative_on_nested_relocated_and_todo_pages() {
    for (root, name, expected) in [
        ("README.md", "default", DEFAULT),
        ("docs/home.md", "bookish", BOOKISH),
    ] {
        let base = TempDir::new().unwrap();
        let dir = source_dir(&base);
        common::write_tree(
            &dir,
            &[
                (root, "# Home\n\n- [ ] home task\n"),
                ("docs/guide.md", "# Guide\n"),
                ("docs/deeper/topic.md", "# Topic\n\n- [ ] nested task\n"),
            ],
        );
        let stylesheet = base.path().join("custom override.css");
        std::fs::write(&stylesheet, CUSTOM).unwrap();
        let before = common::snapshot_dir(&dir);
        let out = base.path().join("site");
        let result = build(
            &dir,
            &out,
            &[
                "--root",
                root,
                "--todo",
                "--theme",
                name,
                "--stylesheet",
                stylesheet.to_str().unwrap(),
            ],
        );
        assert!(result.success(), "{}", result.stderr);
        assert_eq!(
            std::fs::read(out.join("assets/theme.css")).unwrap(),
            expected
        );
        assert_eq!(
            std::fs::read(out.join("assets/custom.css")).unwrap(),
            CUSTOM
        );
        assert_eq!(
            std::fs::read(out.join("assets/ohimark.css")).unwrap(),
            ohimark::theme::STYLE_CSS.as_bytes()
        );
        let pages: Vec<_> = common::relative_paths(&out)
            .into_iter()
            .filter(|path| path.ends_with(".html"))
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
            assert_links(&html, &"../".repeat(page.matches('/').count()), true, true);
            for path in [&dir, &out, &stylesheet] {
                assert!(
                    !html.contains(path.to_str().unwrap()),
                    "{page} leaks {}",
                    path.display()
                );
            }
            for attr in ["href", "src"] {
                for value in common::attr_values(&html, attr) {
                    assert!(!value.starts_with('/'), "{page}: absolute {attr} {value}");
                }
            }
        }
        assert_eq!(common::snapshot_dir(&dir), before);
        assert_eq!(std::fs::read(stylesheet).unwrap(), CUSTOM);
    }
}

#[test]
fn no_options_emit_only_the_native_base_assets() {
    let base = TempDir::new().unwrap();
    let dir = source_dir(&base);
    common::write_tree(
        &dir,
        &[("README.md", "# Home\n"), ("docs/guide.md", "# Guide\n")],
    );
    let out = base.path().join("site");
    let result = build(&dir, &out, &[]);
    assert!(result.success(), "{}", result.stderr);
    assert_eq!(
        common::relative_paths(&out),
        [
            "assets/ohimark.css",
            "assets/ohimark.js",
            "docs/guide.html",
            "index.html"
        ]
    );
    assert_links(
        &common::read_built(&out.join("index.html")),
        "",
        false,
        false,
    );
    assert_links(
        &common::read_built(&out.join("docs/guide.html")),
        "../",
        false,
        false,
    );
}

#[test]
fn repeated_theme_selection_uses_the_last_value() {
    let base = TempDir::new().unwrap();
    let dir = source_dir(&base);
    common::write_tree(&dir, &[("README.md", "# Home\n")]);
    for (first, last, expected) in [
        ("bookish", "default", DEFAULT),
        ("default", "bookish", BOOKISH),
    ] {
        let out = base.path().join(last);
        let result = build(&dir, &out, &["--theme", first, "--theme", last]);
        assert!(result.success(), "{}", result.stderr);
        assert_eq!(
            std::fs::read(out.join("assets/theme.css")).unwrap(),
            expected
        );
    }
}

#[test]
fn unknown_uppercase_and_missing_theme_values_fail_without_output() {
    let base = TempDir::new().unwrap();
    let dir = source_dir(&base);
    common::write_tree(&dir, &[("README.md", "# Home\n")]);
    let before = common::snapshot_dir(&dir);
    for (index, extra) in [
        &["--theme", "unknown"][..],
        &["--theme", "Bookish"],
        &["--theme"],
    ]
    .into_iter()
    .enumerate()
    {
        let out = base.path().join(format!("site-{index}"));
        let result = build(&dir, &out, extra);
        assert!(!result.success(), "invalid theme accepted");
        assert!(result.stderr.contains("--theme"), "{}", result.stderr);
        assert!(
            result.stderr.contains("default") && result.stderr.contains("bookish"),
            "{}",
            result.stderr
        );
        assert!(common::is_absent_or_empty(&out));
    }
    assert_eq!(common::snapshot_dir(&dir), before);
}

#[test]
fn theme_output_collisions_fail_before_staging_and_preserve_sources() {
    for (index, (source, emitted, home)) in [
        (
            "assets/theme.css",
            "assets/theme.css",
            "# Home\n\n[Asset](assets/theme.css)\n",
        ),
        (
            "Assets/THEME.CSS",
            "Assets/THEME.CSS",
            "# Home\n\n[Asset](Assets/THEME.CSS)\n",
        ),
        ("assets", "assets", "# Home\n\n[Asset](assets)\n"),
        (
            "assets/theme.css/icons/logo.bin",
            "assets/theme.css/icons/logo.bin",
            "# Home\n\n[Asset](assets/theme.css/icons/logo.bin)\n",
        ),
        (
            "assets/THEME.css/guide.md",
            "assets/THEME.css/guide.html",
            "# Home\n",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let base = TempDir::new().unwrap();
        let dir = source_dir(&base);
        common::write_tree(&dir, &[("README.md", home), (source, "# Collision\n")]);
        let before = common::snapshot_dir(&dir);
        let out = base.path().join("site");
        if index % 2 == 1 {
            std::fs::create_dir(&out).unwrap();
        }
        let tmp = TempDir::new().unwrap();
        let result = common::run_with_envs(
            [
                dir.to_str().unwrap(),
                "--out",
                out.to_str().unwrap(),
                "--theme",
                "bookish",
            ],
            &[("TMPDIR", tmp.path().to_str().unwrap())],
        );
        assert!(!result.success(), "collision {source} accepted");
        for expected in ["output:", "--theme", "bookish", "assets/theme.css", emitted] {
            assert!(result.stderr.contains(expected), "{}", result.stderr);
        }
        assert!(common::is_absent_or_empty(&out), "partial output survived");
        assert!(common::is_absent_or_empty(tmp.path()), "staging survived");
        assert_eq!(common::snapshot_dir(&dir), before);
    }
}

#[test]
fn unselected_theme_target_allows_regular_assets_and_pages() {
    for (source, emitted, home) in [
        (
            "assets/theme.css",
            "assets/theme.css",
            "# Home\n\n[Asset](assets/theme.css)\n",
        ),
        (
            "assets/theme.css/logo.bin",
            "assets/theme.css/logo.bin",
            "# Home\n\n[Asset](assets/theme.css/logo.bin)\n",
        ),
        (
            "assets/theme.css/guide.md",
            "assets/theme.css/guide.html",
            "# Home\n",
        ),
    ] {
        let base = TempDir::new().unwrap();
        let dir = source_dir(&base);
        common::write_tree(&dir, &[("README.md", home), (source, "# User source\n")]);
        let out = base.path().join("site");
        let result = build(&dir, &out, &[]);
        assert!(result.success(), "{source}: {}", result.stderr);
        assert!(out.join(emitted).is_file());
        if !source.ends_with(".md") {
            assert_eq!(
                std::fs::read(out.join(emitted)).unwrap(),
                b"# User source\n"
            );
        }
        assert_links(
            &common::read_built(&out.join("index.html")),
            "",
            false,
            false,
        );
    }
}

#[test]
fn theme_reservation_ignores_unreferenced_assets_and_component_prefix_near_matches() {
    for unreferenced in ["assets/THEME.CSS", "assets/theme.css/unused.bin"] {
        let base = TempDir::new().unwrap();
        let dir = source_dir(&base);
        common::write_tree(
            &dir,
            &[
                (
                    "README.md",
                    "# Home\n\n[Asset](assets/theme.css-other.bin)\n",
                ),
                ("assets/theme.css-other/guide.md", "# Guide\n"),
            ],
        );
        common::write_file(&dir, unreferenced, b"unreferenced");
        common::write_file(&dir, "assets/theme.css-other.bin", b"referenced");
        let out = base.path().join("site");
        let result = build(&dir, &out, &["--theme", "default"]);
        assert!(result.success(), "{unreferenced}: {}", result.stderr);
        assert_eq!(
            std::fs::read(out.join("assets/theme.css")).unwrap(),
            DEFAULT
        );
        assert_eq!(
            std::fs::read(out.join("assets/theme.css-other.bin")).unwrap(),
            b"referenced"
        );
        assert!(out.join("assets/theme.css-other/guide.html").is_file());
        if unreferenced.to_lowercase() != "assets/theme.css" {
            assert!(!out.join(unreferenced).exists());
        }
    }
}

#[test]
fn selecting_both_styles_reserves_both_output_targets() {
    for (reserved, option) in [
        ("assets/theme.css", "--theme"),
        ("assets/custom.css", "--stylesheet"),
    ] {
        let base = TempDir::new().unwrap();
        let dir = source_dir(&base);
        let home = format!("# Home\n\n[Asset]({reserved})\n");
        common::write_tree(&dir, &[("README.md", &home), (reserved, "user asset")]);
        let stylesheet = base.path().join("custom.css");
        std::fs::write(&stylesheet, CUSTOM).unwrap();
        let out = base.path().join("site");
        let result = build(
            &dir,
            &out,
            &[
                "--theme",
                "bookish",
                "--stylesheet",
                stylesheet.to_str().unwrap(),
            ],
        );
        assert!(!result.success(), "collision {reserved} accepted");
        for expected in ["output:", reserved, option] {
            assert!(result.stderr.contains(expected), "{}", result.stderr);
        }
        assert!(common::is_absent_or_empty(&out));
    }
}

#[test]
fn selected_theme_does_not_hide_a_missing_custom_stylesheet() {
    let base = TempDir::new().unwrap();
    let dir = source_dir(&base);
    common::write_tree(&dir, &[("README.md", "# Home\n")]);
    let missing = base.path().join("missing.css");
    let out = base.path().join("site");
    let result = build(
        &dir,
        &out,
        &[
            "--theme",
            "default",
            "--stylesheet",
            missing.to_str().unwrap(),
        ],
    );
    assert!(!result.success());
    for expected in ["configuration:", "--stylesheet", missing.to_str().unwrap()] {
        assert!(result.stderr.contains(expected), "{}", result.stderr);
    }
    assert!(common::is_absent_or_empty(&out));
}
