//! Explicit standalone HTML inputs, sidebar links, and output reservations (R13).

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use tempfile::TempDir;

fn source_dir(base: &TempDir) -> PathBuf {
    let dir = base.path().join("sources");
    std::fs::create_dir(&dir).expect("create sources");
    common::write_tree(&dir, &[("README.md", "# Home\n")]);
    dir
}

fn build(dir: &Path, out: &Path, extra: &[&str]) -> common::RunResult {
    let mut args = vec![dir.to_str().unwrap(), "--out", out.to_str().unwrap()];
    args.extend_from_slice(extra);
    common::run(args)
}

fn sidebar_nav(html: &str) -> &str {
    let open = "<nav class=\"ohm-nav\" aria-label=\"Site navigation\">";
    let start = html.find(open).expect("site navigation exists") + open.len();
    let rest = &html[start..];
    &rest[..rest.find("</nav>").expect("site navigation closes")]
}

fn assert_rejected(dir: &Path, out: &Path, extra: &[&str]) {
    let before = common::snapshot_dir(dir);
    let result = build(dir, out, extra);
    assert!(!result.success(), "expected failure for {extra:?}");
    assert!(!result.stderr.is_empty(), "failure must explain the error");
    assert!(
        common::is_absent_or_empty(out),
        "failure left output for {extra:?}"
    );
    assert_eq!(before, common::snapshot_dir(dir), "inputs changed");
}

/// R9.6, R13.1-R13.3: pairs accumulate on either side of DIR, and nested
/// HTML sources become top-level links after all Markdown navigation.
#[test]
fn repeated_pairs_add_top_level_links_in_cli_order_with_escaped_titles() {
    let base = TempDir::new().expect("create base");
    let dir = source_dir(&base);
    common::write_tree(
        &dir,
        &[
            ("a.md", "# A page\n"),
            ("docs/guide.md", "# Guide\n"),
            ("reports/Stats.HTML", "<h1>Source title is ignored</h1>\n"),
            ("other.html", "<p>Second HTML page</p>\n"),
        ],
    );
    let first = dir.join("reports/Stats.HTML");
    let second = dir.join("other.html");
    let out = base.path().join("site");
    let result = common::run([
        "--endpoint",
        "Stats <em> & \"quotes\" 'é'",
        first.to_str().unwrap(),
        dir.to_str().unwrap(),
        "--endpoint",
        "Second page",
        second.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    let index = common::read_built(&out.join("index.html"));
    let nav = sidebar_nav(&index);
    let first_link = "<li class=\"ohm-nav-item\"><a class=\"ohm-nav-link\" href=\"reports/Stats.HTML\">Stats &lt;em&gt; &amp; &quot;quotes&quot; &#39;é&#39;</a></li>";
    let second_link = "<li class=\"ohm-nav-item\"><a class=\"ohm-nav-link\" href=\"other.html\">Second page</a></li>";
    assert!(
        nav.contains(&format!("</details></li>{first_link}{second_link}</ul>")),
        "HTML pages must be top-level siblings in CLI order after Markdown: {nav}"
    );
    assert_eq!(nav.matches("reports/Stats.HTML").count(), 1, "{nav}");
    assert!(!nav.contains("Source title is ignored"), "{nav}");
    assert_eq!(
        std::fs::read(out.join("reports/Stats.HTML")).unwrap(),
        std::fs::read(&first).unwrap()
    );
    assert_eq!(
        std::fs::read(out.join("other.html")).unwrap(),
        std::fs::read(&second).unwrap()
    );
}

/// R13.2-R13.3: keep internal source paths and case, encode filename URL
/// characters once, and compute links from each emitted page's directory.
#[test]
fn sidebar_links_are_encoded_and_relative_from_nested_and_relocated_pages() {
    for root in ["README.md", "docs/home.md"] {
        let base = TempDir::new().expect("create base");
        let dir = source_dir(&base);
        common::write_tree(
            &dir,
            &[
                (root, "# Home\n\n- [ ] Markdown task\n"),
                ("docs/deeper/topic.md", "# Topic\n"),
                ("reports/Café #1? 50%.HTML", "<p>Report</p>\n"),
            ],
        );
        let html = dir.join("reports/Café #1? 50%.HTML");
        let out = base.path().join("site");
        let result = build(
            &dir,
            &out,
            &[
                "--root",
                root,
                "--todo",
                "--endpoint",
                "Report",
                html.to_str().unwrap(),
            ],
        );
        assert!(result.success(), "root {root}: {}", result.stderr);
        assert!(out.join("reports/Café #1? 50%.HTML").is_file());

        for (page, prefix) in [
            ("index.html", ""),
            ("docs/deeper/topic.html", "../../"),
            ("TODO.html", ""),
        ] {
            let built = common::read_built(&out.join(page));
            let nav = sidebar_nav(&built);
            let expected = format!("{prefix}reports/Caf%C3%A9%20%231%3F%2050%25.HTML");
            assert!(
                common::attr_values(nav, "href").contains(&expected.as_str()),
                "root {root}, page {page}: {nav}"
            );
        }
    }
}

/// R13.1-R13.2: explicit selection bypasses discovery and resolves relative
/// paths from the process working directory, including files outside DIR.
#[test]
fn cwd_relative_external_ignored_and_excluded_sources_are_copied() {
    let base = TempDir::new().expect("create base");
    let dir = source_dir(&base);
    common::write_tree(
        &dir,
        &[
            (".gitignore", "hidden/\n"),
            ("hidden/ignored.html", "<p>Ignored</p>\n"),
            ("drop.html", "<p>Excluded</p>\n"),
        ],
    );
    common::write_file(base.path(), "external/no-extension", b"opaque bytes\0\xff");
    let before = common::snapshot_dir(base.path());
    let result = Command::new(env!("CARGO_BIN_EXE_ohimark"))
        .current_dir(base.path())
        .args([
            "sources",
            "--out",
            "site",
            "--exclude",
            "drop.html",
            "--endpoint",
            "Ignored",
            "sources/hidden/ignored.html",
            "--endpoint",
            "Excluded",
            "sources/drop.html",
            "--endpoint",
            "External",
            "external/no-extension",
        ])
        .output()
        .expect("run from fixture working directory");
    assert!(
        result.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let out = base.path().join("site");
    for (target, source) in [
        ("hidden/ignored.html", "sources/hidden/ignored.html"),
        ("drop.html", "sources/drop.html"),
        ("no-extension", "external/no-extension"),
    ] {
        assert_eq!(
            std::fs::read(out.join(target)).unwrap(),
            before[source],
            "{source} must be copied to {target}"
        );
        assert_eq!(
            std::fs::read(base.path().join(source)).unwrap(),
            before[source],
            "selected input must be untouched"
        );
    }
    let index = common::read_built(&out.join("index.html"));
    let hrefs = common::attr_values(sidebar_nav(&index), "href");
    for target in ["hidden/ignored.html", "drop.html", "no-extension"] {
        assert!(hrefs.contains(&target), "missing {target}: {hrefs:?}");
    }
    assert_eq!(
        common::snapshot_dir(&dir),
        before
            .into_iter()
            .filter_map(|(path, bytes)| path
                .strip_prefix("sources/")
                .map(|rel| (rel.to_owned(), bytes)))
            .collect(),
        "source tree must be untouched"
    );
}

/// R13.2, R13.4: selected pages are opaque bytes; their dependencies are not
/// discovered, their content is not wrapped, and tasks are not collected.
#[test]
fn html_bytes_are_unchanged_without_processing_dependencies_or_tasks() {
    let base = TempDir::new().expect("create base");
    let dir = source_dir(&base);
    let bytes = b"<!doctype html>\r\n<link rel=\"stylesheet\" href=\"extra.css\">\r\n<img src=\"image.png\"><script src=\"extra.js\"></script>\r\n- [ ] HTML-only task\n\xff\0";
    common::write_file(&dir, "standalone/page.html", bytes);
    common::write_tree(
        &dir,
        &[
            ("standalone/extra.css", "body { color: red }\n"),
            ("standalone/extra.js", "alert('hello');\n"),
            ("standalone/image.png", "image\n"),
        ],
    );
    let before = common::snapshot_dir(&dir);
    let html = dir.join("standalone/page.html");
    let out = base.path().join("site");
    let result = build(
        &dir,
        &out,
        &["--todo", "--endpoint", "Standalone", html.to_str().unwrap()],
    );
    assert!(result.success(), "build failed: {}", result.stderr);
    assert_eq!(
        std::fs::read(out.join("standalone/page.html")).unwrap(),
        bytes
    );
    assert_eq!(
        common::relative_paths(&out),
        [
            "assets/ohimark.css",
            "assets/ohimark.js",
            "index.html",
            "standalone/page.html"
        ]
    );
    assert_eq!(before, common::snapshot_dir(&dir));
}

/// R13.5: output collisions include root and synthetic pages, case-only
/// equality, and file/directory prefix conflicts in both directions.
#[test]
fn rendered_page_collisions_fail_without_output_or_input_changes() {
    for (target, external, markdown, extra) in [
        ("INDEX.HTML", false, "other.md", &[][..]),
        ("guide.html", false, "guide.md", &[][..]),
        ("GUIDE.HTML", true, "guide.html/topic.md", &[][..]),
        ("guide.html/report.html", false, "guide.md", &[][..]),
        ("todo.html", true, "tasks.md", &["--todo"][..]),
    ] {
        let base = TempDir::new().expect("create base");
        let dir = source_dir(&base);
        common::write_tree(&dir, &[(markdown, "# Document\n\n- [ ] Open task\n")]);
        let source_root = if external { base.path() } else { &dir };
        common::write_file(source_root, target, b"<p>Selected HTML</p>\n");
        let html = source_root.join(target);
        let before_html = std::fs::read(&html).unwrap();
        let out = base.path().join("site");
        let mut args = vec!["--endpoint", "Selected", html.to_str().unwrap()];
        args.extend_from_slice(extra);
        assert_rejected(&dir, &out, &args);
        assert_eq!(std::fs::read(html).unwrap(), before_html, "{target}");
    }
}

/// R13.5: reservations cover bundled and enabled optional styles/scripts,
/// plus assets actually referenced by Markdown.
#[test]
fn bundled_optional_style_and_referenced_asset_collisions_are_rejected() {
    for target in [
        "assets/OHIMARK.CSS",
        "assets/ohimark.js/inner.html",
        "assets/THEME.CSS",
        "assets/custom.css/inner.html",
        "IMAGE.PNG",
        "media",
    ] {
        let base = TempDir::new().expect("create base");
        let dir = source_dir(&base);
        let external = matches!(target, "IMAGE.PNG" | "media");
        if external {
            common::write_tree(
                &dir,
                &[
                    (
                        "README.md",
                        "# Home\n\n![Image](image.png)\n\n![Media](media/pixel.png)\n",
                    ),
                    ("image.png", "image\n"),
                    ("media/pixel.png", "pixel\n"),
                ],
            );
        }
        let source_root = if external { base.path() } else { &dir };
        common::write_file(source_root, target, b"<p>Selected</p>\n");
        let html = source_root.join(target);
        let stylesheet = base.path().join("override.css");
        std::fs::write(&stylesheet, b"body { color: blue }\n").unwrap();
        let mut args = vec!["--endpoint", "Selected", html.to_str().unwrap()];
        if target == "assets/THEME.CSS" {
            args.extend_from_slice(&["--theme", "default"]);
        } else if target == "assets/custom.css/inner.html" {
            args.extend_from_slice(&["--stylesheet", stylesheet.to_str().unwrap()]);
        }
        assert_rejected(&dir, &base.path().join("site"), &args);
        assert_eq!(std::fs::read(html).unwrap(), b"<p>Selected</p>\n");
        assert_eq!(
            std::fs::read(stylesheet).unwrap(),
            b"body { color: blue }\n"
        );
    }
}

/// R13.5: independently selected HTML pages must not share or obstruct a target.
#[test]
fn html_page_collisions_are_casefolded_and_include_directory_prefixes() {
    for prefix in [false, true] {
        let base = TempDir::new().expect("create base");
        let dir = source_dir(&base);
        let first = base.path().join("first/Report.html");
        common::write_file(base.path(), "first/Report.html", b"first");
        let second = if prefix {
            common::write_file(&dir, "report.html/details.html", b"second");
            dir.join("report.html/details.html")
        } else {
            common::write_file(base.path(), "second/report.HTML", b"second");
            base.path().join("second/report.HTML")
        };
        assert_rejected(
            &dir,
            &base.path().join("site"),
            &[
                "--endpoint",
                "First",
                first.to_str().unwrap(),
                "--endpoint",
                "Second",
                second.to_str().unwrap(),
            ],
        );
        assert_eq!(std::fs::read(first).unwrap(), b"first");
        assert_eq!(std::fs::read(second).unwrap(), b"second");
    }
}

/// R13.5: one file may be both an explicit page and a Markdown-referenced asset.
#[test]
fn selecting_the_same_source_already_referenced_as_an_asset_is_allowed() {
    let base = TempDir::new().expect("create base");
    let dir = source_dir(&base);
    common::write_tree(
        &dir,
        &[
            ("README.md", "# Home\n\n[Report](manual/Report.HTML)\n"),
            ("manual/Report.HTML", "<p>Report bytes</p>\r\n"),
        ],
    );
    let html = dir.join("manual/Report.HTML");
    let out = base.path().join("site");
    let before = common::snapshot_dir(&dir);
    let result = build(
        &dir,
        &out,
        &["--endpoint", "Report page", html.to_str().unwrap()],
    );
    assert!(result.success(), "build failed: {}", result.stderr);
    assert_eq!(
        std::fs::read(out.join("manual/Report.HTML")).unwrap(),
        b"<p>Report bytes</p>\r\n"
    );
    let index = common::read_built(&out.join("index.html"));
    assert!(sidebar_nav(&index).contains("href=\"manual/Report.HTML\">Report page</a>"));
    assert!(common::article_body(&index).contains("href=\"manual/Report.HTML\">Report</a>"));
    assert_eq!(before, common::snapshot_dir(&dir));
}

/// R13.2: external symlinks use the supplied alias filename, not the canonical filename.
#[cfg(unix)]
#[test]
fn an_external_symlink_uses_its_supplied_alias_filename() {
    let base = TempDir::new().expect("create base");
    let dir = source_dir(&base);
    common::write_file(
        base.path(),
        "external/original.html",
        b"<p>External</p>\r\n",
    );
    let original = base.path().join("external/original.html");
    let alias = base.path().join("Public report.HTML");
    std::os::unix::fs::symlink(&original, &alias).expect("create external alias");
    let out = base.path().join("site");
    let result = build(
        &dir,
        &out,
        &["--endpoint", "Report", alias.to_str().unwrap()],
    );
    assert!(result.success(), "build failed: {}", result.stderr);
    assert_eq!(
        std::fs::read(out.join("Public report.HTML")).unwrap(),
        b"<p>External</p>\r\n"
    );
    assert!(!out.join("original.html").exists());
    let index = common::read_built(&out.join("index.html"));
    assert!(sidebar_nav(&index).contains("href=\"Public%20report.HTML\">Report</a>"));
    assert_eq!(std::fs::read(original).unwrap(), b"<p>External</p>\r\n");
    assert!(
        std::fs::symlink_metadata(alias)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

/// R9.9, R13.1: each occurrence requires exactly a nonblank title and a regular file.
#[test]
fn invalid_paths_titles_and_incomplete_or_overfull_pairs_leave_no_output() {
    let base = TempDir::new().expect("create base");
    let dir = source_dir(&base);
    common::write_file(&dir, "page.html", b"<p>Page</p>\n");
    let html = dir.join("page.html");
    let missing = dir.join("missing.html");
    for (index, args) in [
        vec!["--endpoint"],
        vec!["--endpoint", "Title"],
        vec!["--endpoint", "", html.to_str().unwrap()],
        vec!["--endpoint", " \t\n", html.to_str().unwrap()],
        vec!["--endpoint", "Missing", missing.to_str().unwrap()],
        vec!["--endpoint", "Directory", dir.to_str().unwrap()],
        vec!["--endpoint", "Title", html.to_str().unwrap(), "extra-value"],
    ]
    .into_iter()
    .enumerate()
    {
        assert_rejected(&dir, &base.path().join(format!("site-{index}")), &args);
    }
}

/// R13.1, R13.4: a selected file disappearing after validation is fatal and atomic.
#[test]
fn a_file_removed_after_configuration_validation_fails_without_output() {
    let base = TempDir::new().expect("create base");
    let dir = source_dir(&base);
    common::write_file(&dir, "page.html", b"<p>Page</p>\n");
    let html = dir.join("page.html");
    let out = base.path().join("site");
    let config = ohimark::Args::parse_from([
        "ohimark",
        dir.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--endpoint",
        "Selected",
        html.to_str().unwrap(),
    ])
    .into_config()
    .expect("valid HTML selection");
    assert_eq!(config.html_pages.len(), 1);
    assert_eq!(config.html_pages[0].title, "Selected");
    assert_eq!(
        config.html_pages[0].source.as_std_path(),
        std::fs::canonicalize(&html).unwrap()
    );
    assert_eq!(config.html_pages[0].target.as_path().as_str(), "page.html");
    std::fs::remove_file(html).expect("remove validated input");
    let before = common::snapshot_dir(&dir);
    let error = ohimark::build(&config).expect_err("missing explicit input must be fatal");
    assert!(error.to_string().contains("page.html"), "{error}");
    assert!(common::is_absent_or_empty(&out));
    assert_eq!(before, common::snapshot_dir(&dir));
}
