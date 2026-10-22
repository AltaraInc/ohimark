//! Discovery, paths, navigation and assets (R1, R2, R4, R5, R6.6), driven
//! against the real binary.

mod common;

use tempfile::TempDir;

/// The sidebar navigation, excluding page content and the separate outline.
fn sidebar_nav(html: &str) -> &str {
    let open = "<nav class=\"ohm-nav\" aria-label=\"Site navigation\">";
    let start = html.find(open).expect("site navigation must exist") + open.len();
    let rest = &html[start..];
    let end = rest.find("</nav>").expect("site navigation must close");
    &rest[..end]
}

/// R1.1: every file whose extension casefolds to `md` or `markdown` becomes a page,
/// regardless of the case of the extension.
#[test]
fn r1_1_extension_matched_casefolded() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n"),
            ("a.md", "# A page\n"),
            ("B.MD", "# B page\n"),
            ("c.markdown", "# C page\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    for target in ["a.html", "B.html", "c.html"] {
        assert!(
            out_path.join(target).is_file(),
            "expected page not built: {target}"
        );
    }
}

/// R1.2: `.gitignore` is honoured even when `DIR` contains no `.git` directory.
#[test]
fn r1_2_gitignore_honoured_without_a_git_directory() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n"),
            ("ignored.md", "# Ignored\n"),
            (".gitignore", "ignored.md\n"),
        ],
    );
    assert!(
        !dir.path().join(".git").exists(),
        "fixture must not contain a .git directory"
    );
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);
    assert!(out_path.join("index.html").is_file());
    assert!(
        !out_path.join("ignored.html").exists(),
        "gitignored file must not become a page"
    );
}

/// R1.3: a `.md` file under `.git/`, at the top or nested, produces no page.
#[test]
fn r1_3_dot_git_directory_always_skipped() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n"),
            (".git/tracked.md", "# Tracked\n"),
            ("nested/.git/tracked.md", "# Nested tracked\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    let built = common::relative_paths(&out_path);
    assert!(
        built.iter().all(|p| !p.contains("tracked")),
        "a file under .git must never become a page: {built:?}"
    );
}

/// R1.4: `--exclude` is repeatable; each glob excludes its own matches and they accumulate.
/// A glob matching a directory takes everything under it.
#[test]
fn r1_4_exclude_globs_accumulate_and_a_directory_glob_takes_everything_under_it() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n"),
            ("keep.md", "# Keep\n"),
            ("drop1.md", "# Drop 1\n"),
            ("dropdir/whatever.md", "# Whatever\n"),
            ("dropdir/sub/thing.md", "# Thing\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
        "--exclude",
        "drop1.md",
        "--exclude",
        "dropdir",
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    assert!(out_path.join("keep.html").is_file());
    assert!(!out_path.join("drop1.html").exists());
    assert!(
        !out_path.join("dropdir").exists(),
        "a glob matching a directory must exclude everything under it"
    );
}

/// R1.5: hidden files are eligible; a leading dot alone does not exclude a file.
#[test]
fn r1_5_hidden_files_are_eligible() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[("README.md", "# Home\n"), (".hidden.md", "# Hidden\n")],
    );
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);
    assert!(
        out_path.join(".hidden.html").is_file(),
        "a leading dot alone must not exclude a file"
    );
}

/// R1.8: symlink files and directories cannot introduce sources or assets from outside DIR.
#[cfg(unix)]
#[test]
fn r1_8_symlink_markdown_directories_and_assets_are_not_discovered() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().expect("create dir");
    let outside = TempDir::new().expect("create outside dir");
    common::write_tree(
        dir.path(),
        &[
            (
                "README.md",
                "# Home\n\n[Real](real.md)\n\n[Linked](linked.md)\n\n[Directory](linked-dir/nested.md)\n\n![Outside](linked.png)\n\n![Missing](missing.png)\n",
            ),
            ("real.md", "# Real page\n\nReal page content.\n"),
        ],
    );
    common::write_tree(
        outside.path(),
        &[
            ("page.md", "# Outside page\n\n- [ ] Outside task\n"),
            ("nested.md", "# Outside nested\n\n- [ ] Nested task\n"),
        ],
    );
    common::write_file(outside.path(), "image.png", &tiny_png());
    symlink(outside.path().join("page.md"), dir.path().join("linked.md"))
        .expect("symlink Markdown file");
    symlink(outside.path(), dir.path().join("linked-dir")).expect("symlink directory");
    symlink(
        outside.path().join("image.png"),
        dir.path().join("linked.png"),
    )
    .expect("symlink asset");

    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");
    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
        "--todo",
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    let real = common::read_built(&out_path.join("real.html"));
    assert!(common::article_body(&real).contains("Real page content."));
    let index = common::read_built(&out_path.join("index.html"));
    let nav = sidebar_nav(&index);
    assert!(nav.contains("href=\"real.html\""), "{nav}");
    assert!(!nav.contains("Outside page"), "{nav}");
    assert!(!nav.contains("Outside nested"), "{nav}");
    for path in [
        "linked.html",
        "linked-dir",
        "linked.png",
        "missing.png",
        "TODO.html",
    ] {
        assert!(!out_path.join(path).exists(), "unexpected output: {path}");
    }
    for path in [
        "linked.md",
        "linked-dir/nested.md",
        "linked.png",
        "missing.png",
    ] {
        assert!(
            result
                .stderr
                .contains(&format!("unresolved reference `{path}`")),
            "missing reference warning for {path}: {}",
            result.stderr
        );
    }
}

/// R1.9: the output directory is never discovered as input, even when it lies inside `DIR`.
/// Build twice with `--out` inside `DIR`; a second build (into a different output path, since
/// the first output is left on disk) must discover the same page set as the first.
#[test]
fn r1_9_output_directory_is_never_discovered_as_input() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(dir.path(), &[("README.md", "# Home\n"), ("a.md", "# A\n")]);

    let out1 = dir.path().join("site1");
    let first = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out1.to_str().unwrap(),
    ]);
    assert!(first.success(), "first build failed: {}", first.stderr);

    // `out1` is left on disk, non-empty, inside DIR. A second build into a
    // different output path must still see exactly the same source pages -
    // it must not pick up `out1`'s .html/.css/.js as new sources, nor fail.
    let out2 = dir.path().join("site2");
    let second = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out2.to_str().unwrap(),
    ]);
    assert!(second.success(), "second build failed: {}", second.stderr);

    let pages_of = |out: &std::path::Path| -> Vec<String> {
        common::relative_paths(out)
            .into_iter()
            .filter(|p| p.ends_with(".html"))
            .collect()
    };
    let mut first_pages = pages_of(&out1);
    let mut second_pages = pages_of(&out2);
    first_pages.sort();
    second_pages.sort();
    assert_eq!(
        first_pages, second_pages,
        "second build's page set must match the first's, unaffected by the first build's leftover output"
    );
}

/// R2.3 / R2.4: the root file is emitted as `index.html`; other pages keep their
/// `DIR`-relative path with the extension replaced by `.html`, and no other renaming.
#[test]
fn r2_3_and_r2_4_root_becomes_index_and_others_keep_their_path() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n"),
            ("a/b/c.md", "# C\n"),
            ("a/b/d.markdown", "# D\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    let index = common::read_built(&out_path.join("index.html"));
    assert!(
        index.contains("Home"),
        "index.html must carry the root's content"
    );
    assert!(out_path.join("a/b/c.html").is_file());
    assert!(out_path.join("a/b/d.html").is_file());
}

/// R2.6: consecutive dots inside a filename are not parent-directory components.
#[test]
fn r2_6_filename_with_consecutive_dots_renders_and_links() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n\n[Version](version..md)\n"),
            ("version..md", "# Version\n\n[Home](README.md)\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");
    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);
    assert!(result.stderr.is_empty(), "{}", result.stderr);

    let index = common::read_built(&out_path.join("index.html"));
    let version = common::read_built(&out_path.join("version..html"));
    assert!(common::attr_values(common::article_body(&index), "href").contains(&"version..html"));
    assert!(common::attr_values(common::article_body(&version), "href").contains(&"index.html"));
}

/// R2.5: two sources whose target paths fold to the same value are a hard error naming both
/// sources, and produce no output. `.md` and `.markdown` are treated identically (R2.4), so
/// `guide.md` and `guide.markdown` collide on `guide.html` on every filesystem, regardless of
/// case sensitivity.
#[test]
fn r2_5_colliding_extensions_are_a_hard_error_naming_both_sources() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n"),
            ("docs/guide.md", "# Guide md\n"),
            ("docs/guide.markdown", "# Guide markdown\n"),
        ],
    );
    let base = TempDir::new().expect("create base");
    let out_path = base.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(
        !result.success(),
        "expected non-zero exit: guide.md and guide.markdown both target guide.html"
    );
    assert!(
        result.stderr.contains("guide.md"),
        "stderr must name guide.md: {}",
        result.stderr
    );
    assert!(
        result.stderr.contains("guide.markdown"),
        "stderr must name guide.markdown: {}",
        result.stderr
    );
    assert!(
        common::is_absent_or_empty(&out_path),
        "no output must be produced on a target collision"
    );
}

/// R2.5, case-folded variant: `Guide.md` and `guide.md` also collide, on `guide.html`. Skipped
/// on a case-insensitive, case-preserving filesystem (default macOS/APFS), where the second
/// write silently overwrites the first and the fixture cannot exist at all — verified here by
/// actually writing both files and checking that both survive with their own content, rather
/// than by a separate, possibly-unrepresentative probe.
#[test]
fn r2_5_case_folded_collision_is_a_hard_error_naming_both_sources() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(dir.path(), &[("README.md", "# Home\n")]);
    common::write_file(dir.path(), "docs/Guide.md", b"# Guide upper\n");
    common::write_file(dir.path(), "docs/guide.md", b"# guide lower\n");

    let upper_survived = std::fs::read(dir.path().join("docs/Guide.md"))
        .map(|bytes| bytes == b"# Guide upper\n")
        .unwrap_or(false);
    let lower_survived = std::fs::read(dir.path().join("docs/guide.md"))
        .map(|bytes| bytes == b"# guide lower\n")
        .unwrap_or(false);
    if !(upper_survived && lower_survived) {
        eprintln!(
            "skipping r2_5_case_folded_collision_is_a_hard_error_naming_both_sources: \
             host filesystem is case-insensitive and cannot hold `Guide.md` and `guide.md` \
             simultaneously"
        );
        return;
    }

    let base = TempDir::new().expect("create base");
    let out_path = base.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(
        !result.success(),
        "expected non-zero exit for colliding targets"
    );
    assert!(
        result.stderr.contains("Guide.md"),
        "stderr must name Guide.md: {}",
        result.stderr
    );
    assert!(
        result.stderr.contains("guide.md"),
        "stderr must name guide.md: {}",
        result.stderr
    );
    assert!(
        common::is_absent_or_empty(&out_path),
        "no output must be produced on a target collision"
    );
}

/// R2.7: when the root file comes from a subdirectory, its own relative links and image
/// references are re-based from its original directory to the output root.
#[test]
fn r2_7_root_from_subdirectory_rebases_its_own_links_and_images() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            (
                "docs/home.md",
                "# Home\n\n[Guide](guide.md)\n\n![D](img/d.png)\n",
            ),
            ("docs/guide.md", "# Guide\n"),
        ],
    );
    common::write_file(dir.path(), "docs/img/d.png", &tiny_png());
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
        "--root",
        "docs/home.md",
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    let index = common::read_built(&out_path.join("index.html"));
    let hrefs = common::attr_values(&index, "href");
    assert!(
        hrefs.contains(&"docs/guide.html"),
        "link to guide.md must resolve to docs/guide.html from the output root: {hrefs:?}"
    );
    let srcs = common::attr_values(&index, "src");
    assert!(
        srcs.contains(&"docs/img/d.png"),
        "image src must resolve to docs/img/d.png from the output root: {srcs:?}"
    );
}

/// R4.2: a directory holding a page whose stem casefolds to the directory's own name becomes
/// a link to that page in the sidebar.
#[test]
fn r4_2_directory_page_matching_dir_name_becomes_the_nav_link() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Root\n"),
            ("docs/docs.md", "# Docs\n"),
            ("docs/README.markdown", "# README child\n"),
            ("docs/guide.md", "# Guide\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    let index = common::read_built(&out_path.join("index.html"));
    let nav = sidebar_nav(&index);
    assert!(
        nav.contains("<a class=\"ohm-nav-link ohm-nav-label\" href=\"docs/docs.html\">docs</a>"),
        "docs node must link to docs/docs.html: {index}"
    );
    assert_eq!(nav.matches("href=\"docs/docs.html\"").count(), 1);
    assert_eq!(nav.matches("href=\"docs/README.html\"").count(), 1);
    assert!(
        nav.contains("href=\"docs/README.html\">README child</a>"),
        "the unselected README must remain a child page: {nav}"
    );
}

/// R4.2: a directory README becomes its heading link and is not repeated as a child page.
#[test]
fn r4_2_readme_directory_page_becomes_nav_link_once_without_child() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Root\n"),
            ("docs/README.md", "# Docs overview\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    let index = common::read_built(&out_path.join("index.html"));
    let nav = sidebar_nav(&index);
    assert_eq!(nav.matches("href=\"docs/README.html\"").count(), 1);
    assert!(
        nav.contains(
            "<details class=\"ohm-nav-details\" data-ohm-nav-group=\"docs\" open><summary class=\"ohm-nav-summary\"><a class=\"ohm-nav-link ohm-nav-label\" href=\"docs/README.html\">docs</a></summary><ul class=\"ohm-nav-list\"></ul></details></li>"
        ),
        "README landing page must keep the native details/summary group markup: {nav}"
    );
    assert!(
        !nav.contains(">Docs overview</a>"),
        "the README title must not appear as a redundant child page: {nav}"
    );
}

/// R4.2: README matching is case-insensitive, supports both Markdown extensions, and recurses.
#[test]
fn r4_2_readme_directory_links_recurse_with_relative_current_links() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Root\n"),
            ("Guides/README.MARKDOWN", "# Guides overview\n"),
            ("Guides/API/ReAdMe.Md", "# API overview\n"),
            ("Guides/API/reference.md", "# Reference\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    let index = common::read_built(&out_path.join("index.html"));
    let root_nav = sidebar_nav(&index);
    assert_eq!(root_nav.matches("href=\"Guides/README.html\"").count(), 1);
    assert_eq!(
        root_nav.matches("href=\"Guides/API/ReAdMe.html\"").count(),
        1
    );
    assert!(root_nav.contains("href=\"Guides/API/ReAdMe.html\">API</a>"));

    let landing = common::read_built(&out_path.join("Guides/README.html"));
    let landing_nav = sidebar_nav(&landing);
    assert_eq!(landing_nav.matches("href=\"README.html\"").count(), 1);
    assert!(
        landing_nav.contains(
            "class=\"ohm-nav-link ohm-nav-label ohm-nav-link--current\" aria-current=\"page\" href=\"README.html\">Guides</a>"
        ),
        "the landing page heading must carry the current marker and relative URL: {landing_nav}"
    );
    assert_eq!(landing_nav.matches("href=\"API/ReAdMe.html\"").count(), 1);
    assert!(
        landing_nav.contains("href=\"API/ReAdMe.html\">API</a>"),
        "nested heading URLs must be relative to the landing page: {landing_nav}"
    );
}

/// R4.3: without a matching page, the directory node is a group label with no link.
#[test]
fn r4_3_directory_without_matching_page_has_no_nav_link() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[("README.md", "# Root\n"), ("docs/guide.md", "# Guide\n")],
    );
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    let index = common::read_built(&out_path.join("index.html"));
    assert!(
        index.contains("<span class=\"ohm-nav-label\">docs</span>"),
        "docs node must be a plain label with no link: {index}"
    );
    assert!(
        !index.contains("href=\"docs.html\"") && !index.contains("href=\"docs/docs.html\""),
        "docs node must carry no href: {index}"
    );
}

/// R5.1: a referenced image is copied byte-identically at its `DIR`-relative path; an
/// unreferenced image is absent from the output.
#[test]
fn r5_1_referenced_asset_copied_byte_identical_unreferenced_absent() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[("README.md", "# Home\n\n![Used](docs/used.png)\n")],
    );
    let used_bytes = tiny_png();
    let unused_bytes = tiny_png_variant();
    common::write_file(dir.path(), "docs/used.png", &used_bytes);
    common::write_file(dir.path(), "docs/unused.png", &unused_bytes);

    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");
    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    let copied = std::fs::read(out_path.join("docs/used.png")).expect("used.png must be copied");
    assert_eq!(copied, used_bytes, "copied asset must be byte-identical");
    assert!(
        !out_path.join("docs/unused.png").exists(),
        "an unreferenced asset must not be copied"
    );
}

/// R5.4: an asset referenced by more than one page is copied exactly once.
#[test]
fn r5_4_asset_referenced_twice_copied_once() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n\n![Shared](shared.png)\n"),
            ("a.md", "# A\n\n![Shared](shared.png)\n"),
        ],
    );
    let bytes = tiny_png();
    common::write_file(dir.path(), "shared.png", &bytes);

    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");
    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    let built = common::relative_paths(&out_path);
    let shared_copies: Vec<_> = built.iter().filter(|p| p.ends_with("shared.png")).collect();
    assert_eq!(
        shared_copies.len(),
        1,
        "shared.png must be copied exactly once: {built:?}"
    );
    let copied = std::fs::read(out_path.join("shared.png")).expect("shared.png must exist");
    assert_eq!(copied, bytes);
}

/// R6.6: a link to a Markdown file that is not a page in the site produces one warning, and
/// leaves the href byte-for-byte unchanged. The build still succeeds.
#[test]
fn r6_6_link_to_a_non_page_markdown_file_is_unchanged_with_a_warning() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[("README.md", "# Home\n\n[Nowhere](nowhere.md)\n")],
    );
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);
    assert!(
        result.stderr.contains("nowhere.md"),
        "expected a warning naming nowhere.md: {}",
        result.stderr
    );

    let index = common::read_built(&out_path.join("index.html"));
    assert!(
        index.contains("href=\"nowhere.md\""),
        "href to a non-page target must be left byte-for-byte unchanged: {index}"
    );
}

/// A minimal, valid PNG signature plus a little body — enough to exercise byte-identical
/// copying without needing a real image.
fn tiny_png() -> Vec<u8> {
    let mut bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(b"not a real png body, just bytes to copy verbatim");
    bytes
}

/// A second, distinct set of bytes carrying the same signature, for tests needing two
/// different assets.
fn tiny_png_variant() -> Vec<u8> {
    let mut bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(b"a different body from the other fixture image");
    bytes
}
