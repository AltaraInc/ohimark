//! Collecting open tasks into the root-level TODO page with `--todo` (R12).

mod common;

use std::path::Path;

use tempfile::TempDir;

/// Build `dir` into a fresh `site` directory under `out`, passing `extra` flags, and
/// return the built site's path.
fn build(dir: &Path, out: &TempDir, extra: &[&str]) -> std::path::PathBuf {
    let out_path = out.path().join("site");
    let mut args = vec![dir.to_str().unwrap(), "--out", out_path.to_str().unwrap()];
    args.extend_from_slice(extra);
    let result = common::run(args);
    assert!(result.success(), "build failed: {}", result.stderr);
    out_path
}

/// The byte offset of `needle` in `haystack`, panicking with context when absent.
fn position(haystack: &str, needle: &str) -> usize {
    haystack
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` not found in:\n{haystack}"))
}

/// Return the generated `<section>` carrying `marker`, including its rendered content.
fn marked_section<'a>(html: &'a str, marker: &str) -> &'a str {
    let marker_position = position(html, marker);
    let start = html[..marker_position]
        .rfind("<section")
        .unwrap_or_else(|| panic!("`{marker}` is not inside a section: {html}"));
    let end = marker_position
        + html[marker_position..]
            .find("</section>")
            .unwrap_or_else(|| panic!("section for `{marker}` is not closed: {html}"))
        + "</section>".len();
    &html[start..end]
}

/// R12.1: without `--todo`, no TODO page is generated and an existing `TODO.md`
/// renders with nothing appended.
#[test]
fn r12_1_without_the_flag_nothing_is_collected() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n\n- [ ] home task\n"),
            ("docs/guide.md", "# Guide\n\n- [ ] guide task\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &[]);

    assert!(
        !site.join("TODO.html").exists(),
        "no TODO page without --todo"
    );
    let index = common::read_built(&site.join("index.html"));
    assert!(
        !index.contains("TODO.html"),
        "the sidebar must not list a TODO page"
    );

    common::write_tree(dir.path(), &[("TODO.md", "# TODO\n\n- [ ] own task\n")]);
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &[]);
    let todo = common::read_built(&site.join("TODO.html"));
    let body = common::article_body(&todo);
    assert!(
        !todo.contains("<article class=\"ohm-content\" data-ohm-todo>"),
        "without --todo an ordinary TODO.md is not a final collection page"
    );
    assert!(
        !body.contains("Collected Tasks"),
        "nothing appended: {body}"
    );
    assert!(!body.contains("home task"), "nothing collected: {body}");
}

/// R12.3 and R12.5: with no root `TODO.md`, a `TODO.html` titled `TODO` is generated,
/// listed in the sidebar, and carries the open tasks under an h2 linking each file, in
/// page order, with no directory headings and skipping files with none.
#[test]
fn r12_3_and_r12_5_synthetic_page_lists_tasks_under_file_headings() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n\n- [ ] home task\n"),
            ("a.md", "# Alpha\n\n- [ ] alpha task\n- [x] alpha done\n"),
            ("empty/x.md", "# X\n\nNo tasks here.\n"),
            ("guides/install.md", "# Install\n\n- [ ] install task\n"),
            ("guides/none.md", "# None\n\n- [x] all done\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &["--todo"]);

    let todo = common::read_built(&site.join("TODO.html"));
    assert!(common::page_title(&todo).starts_with("TODO "));
    assert!(
        todo.contains("<article class=\"ohm-content\" data-ohm-todo>"),
        "the synthetic TODO page is marked"
    );
    let body = common::article_body(&todo);

    assert!(body.contains(">TODO</h1>"), "synthetic h1: {body}");
    assert!(!body.contains("Collected Tasks"), "no appended h1: {body}");

    let alpha = position(body, r#"<a href="a.html">Alpha</a></h2>"#);
    let alpha_task = position(body, "alpha task");
    let home = position(body, r#"<a href="index.html">Home</a></h2>"#);
    let home_task = position(body, "home task");
    let install = position(body, r#"<a href="guides/install.html">Install</a></h2>"#);
    let install_task = position(body, "install task");
    // Page order is by casefolded source path (R1.6): `a.md`, `guides/install.md`,
    // `README.md`, with no regrouping by directory.
    assert!(
        alpha < alpha_task
            && alpha_task < install
            && install < install_task
            && install_task < home
            && home < home_task,
        "tasks must follow their file headings, in page order: {body}"
    );
    assert_eq!(
        body.matches("<h2").count(),
        3,
        "one h2 per file with open tasks, no directory headings: {body}"
    );
    assert!(!body.contains("<h3"), "no h3 file headings: {body}");
    assert!(
        !body.contains(">/</h2>"),
        "no root directory heading: {body}"
    );
    assert!(
        !body.contains(">guides</h2>"),
        "no directory heading: {body}"
    );

    assert_eq!(
        body.matches("data-ohm-todo-group").count(),
        3,
        "one generated group per source: {body}"
    );
    let first_group = marked_section(body, "data-ohm-todo-group");
    assert!(
        first_group.contains("<h2"),
        "rendered heading: {first_group}"
    );
    assert!(
        first_group.contains("<ul"),
        "rendered task list: {first_group}"
    );
    assert!(
        first_group.contains("alpha task"),
        "rendered task: {first_group}"
    );
    assert!(
        !first_group.contains("- [ ] alpha task"),
        "group contains rendered HTML rather than source Markdown: {first_group}"
    );

    assert!(
        !body.contains("alpha done"),
        "checked items are not collected"
    );
    assert!(
        !body.contains(">None<"),
        "a file with no open tasks is skipped"
    );
    assert!(!body.contains(">X<"), "a file with no task list is skipped");

    let index = common::read_built(&site.join("index.html"));
    assert!(
        !index.contains("<article class=\"ohm-content\" data-ohm-todo>"),
        "an ordinary page is not the final TODO page"
    );
    assert!(
        common::attr_values(&index, "href").contains(&"TODO.html"),
        "the TODO page must be in the sidebar"
    );
}

/// R12.3 and R12.5: with no open task anywhere and no root `TODO.md`, no TODO page is
/// generated, nothing is added to the sidebar, and no message stands in for the tasks.
#[test]
fn r12_5_no_open_tasks_generates_no_todo_page() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(dir.path(), &[("README.md", "# Home\n\n- [x] done\n")]);
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &["--todo"]);

    assert!(
        !site.join("TODO.html").exists(),
        "no TODO page with nothing to collect"
    );
    let index = common::read_built(&site.join("index.html"));
    assert!(
        !index.contains("TODO.html"),
        "the sidebar must not list a TODO page"
    );
    assert!(!index.contains("No open tasks"), "no message: {index}");
}

/// R12.4 and R12.5: with no open task on any other page, an existing root `TODO.md`
/// renders as it does without `--todo`: no `Collected Tasks` section and no message.
#[test]
fn r12_4_existing_todo_without_collected_tasks_renders_as_usual() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n\n- [x] done\n"),
            ("TODO.md", "# Roadmap\n\n- [ ] own task\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &["--todo"]);

    let todo = common::read_built(&site.join("TODO.html"));
    let body = common::article_body(&todo);
    assert!(
        todo.contains("<article class=\"ohm-content\" data-ohm-todo>"),
        "the TODO page is marked even when it has no appendix"
    );
    assert!(body.contains("own task"), "own content: {body}");
    assert!(!body.contains("Collected Tasks"), "no section: {body}");
    assert!(!body.contains("No open tasks"), "no message: {body}");
}

/// R12.4: an existing root `TODO.md` renders as usual with a `Collected Tasks` section
/// appended, and its own tasks are not collected into it again.
#[test]
fn r12_4_existing_todo_gets_an_appended_section_without_its_own_tasks() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n\n- [ ] readme task\n"),
            ("TODO.md", "# Roadmap\n\n- [ ] own task\n"),
            ("sub/TODO.md", "# Sub TODO\n\n- [ ] nested todo task\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &["--todo"]);

    let todo = common::read_built(&site.join("TODO.html"));
    assert!(common::page_title(&todo).starts_with("Roadmap "));
    assert!(
        todo.contains("<article class=\"ohm-content\" data-ohm-todo>"),
        "the existing TODO page is marked"
    );
    let body = common::article_body(&todo);
    assert_eq!(
        body.matches("own task").count(),
        1,
        "own tasks once: {body}"
    );
    let own = position(body, "own task");
    let section = position(body, ">Collected Tasks</h1>");
    let readme = position(body, "readme task");
    assert!(own < section && section < readme, "appended after: {body}");
    assert!(
        body.contains("data-ohm-todo-collected"),
        "appendix wrapper: {body}"
    );
    let group = marked_section(body, "data-ohm-todo-group");
    assert!(group.contains("<h2"), "group heading is rendered: {group}");
    assert!(
        group.contains("<ul"),
        "group task list is rendered: {group}"
    );
    assert!(
        body.contains("nested todo task"),
        "a TODO.md in a subdirectory is collected like any page: {body}"
    );
}

/// R12.2: checked items, and checked items nested inside an open one, are excluded; an
/// open item nested in a checked one is still collected, including one under a checked
/// item dropped from an open one, which follows that open item.
#[test]
fn r12_2_checked_and_nested_checked_items_are_excluded() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n"),
            (
                "a.md",
                "# A\n\n- [ ] parent open\n  - [x] nested done\n    - [ ] under done\n  - [ ] nested open\n  - plain child\n- [x] checked parent\n  - [ ] child of checked\n",
            ),
        ],
    );
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &["--todo"]);

    let todo = common::read_built(&site.join("TODO.html"));
    let body = common::article_body(&todo);
    for present in [
        "parent open",
        "nested open",
        "plain child",
        "under done",
        "child of checked",
    ] {
        assert!(
            body.contains(present),
            "`{present}` must be collected: {body}"
        );
    }
    for absent in ["nested done", "checked parent"] {
        assert!(
            !body.contains(absent),
            "`{absent}` must not be collected: {body}"
        );
    }
    assert!(
        !body.contains("checked=\"\""),
        "no checked box is collected"
    );
    assert_eq!(body.matches(r#"<li class="ohm-task">"#).count(), 4);
    let parent = position(body, "parent open");
    let nested = position(body, "nested open");
    let under = position(body, "under done");
    let child = position(body, "child of checked");
    assert!(
        parent < nested && nested < under && under < child,
        "source order: {body}"
    );
}

/// R12.6: a link and an image inside a task in a nested directory resolve from the TODO
/// page, and the image is copied.
#[test]
fn r12_6_links_and_images_resolve_from_the_todo_page_and_assets_are_copied() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n"),
            ("guides/install.md", "# Install\n"),
            (
                "guides/api/ref.md",
                "# Ref\n\n- [ ] see [Install](../install.md) and ![D](img/d.png)\n",
            ),
        ],
    );
    common::write_file(dir.path(), "guides/api/img/d.png", b"not really a png");
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &["--todo"]);

    let todo = common::read_built(&site.join("TODO.html"));
    let body = common::article_body(&todo);
    let hrefs = common::attr_values(body, "href");
    assert!(hrefs.contains(&"guides/install.html"), "link: {hrefs:?}");
    assert!(
        hrefs.contains(&"guides/api/ref.html"),
        "file heading: {hrefs:?}"
    );
    let srcs = common::attr_values(body, "src");
    assert!(srcs.contains(&"guides/api/img/d.png"), "image: {srcs:?}");
    assert!(
        body.contains(r#"<a href="guides/api/ref.html">Ref</a></h2>"#),
        "h2 file heading: {body}"
    );
    assert!(
        !body.contains(">guides/api</h2>"),
        "no directory heading: {body}"
    );

    let copied = std::fs::read(site.join("guides/api/img/d.png")).expect("asset copied");
    assert_eq!(copied, b"not really a png");
}

/// R12.4: `--root TODO.md` makes the TODO page the home page; the tasks are appended to
/// `index.html` and no separate `TODO.html` is generated.
#[test]
fn r12_4_root_todo_file_collects_into_index() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("TODO.md", "# TODO\n\n- [ ] own task\n"),
            ("notes.md", "# Notes\n\n- [ ] note task\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &["--todo", "--root", "TODO.md"]);

    assert!(!site.join("TODO.html").exists(), "no second TODO page");
    let index = common::read_built(&site.join("index.html"));
    assert!(
        index.contains("<article class=\"ohm-content\" data-ohm-todo>"),
        "a TODO selected as root is marked at index.html"
    );
    let body = common::article_body(&index);
    assert_eq!(
        body.matches("own task").count(),
        1,
        "own tasks once: {body}"
    );
    assert!(body.contains(">Collected Tasks</h1>"), "appended: {body}");
    assert!(body.contains(r#"<a href="notes.html">Notes</a>"#), "{body}");
    assert!(body.contains("note task"), "{body}");
}

/// R12.7: `--todo` writes nothing under `DIR`, synthetic TODO page or not.
#[test]
fn r12_7_dir_is_untouched() {
    for files in [
        &[("README.md", "# Home\n\n- [ ] task\n")][..],
        &[
            ("README.md", "# Home\n\n- [ ] task\n"),
            ("TODO.md", "# TODO\n\n- [ ] own\n"),
        ][..],
    ] {
        let dir = TempDir::new().expect("create dir");
        common::write_tree(dir.path(), files);
        let before = common::snapshot_dir(dir.path());

        let out = TempDir::new().expect("create out");
        build(dir.path(), &out, &["--todo"]);

        assert_eq!(
            before,
            common::snapshot_dir(dir.path()),
            "DIR must be byte-for-byte unchanged"
        );
    }
}

/// R12.1 and R9.5: `--help` lists `--todo`.
#[test]
fn r12_1_help_lists_the_todo_flag() {
    let result = common::run(["--help"]);
    assert!(result.success(), "--help failed: {}", result.stderr);
    assert!(result.stdout.contains("--todo"), "help omits --todo");
}

/// R12.2: an item in a loose list is collected through its paragraph.
#[test]
fn r12_2_loose_list_items_are_collected() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[(
            "README.md",
            "# Home\n\n- [x] loose done\n\n- [ ] loose open\n\n- [ ] loose second\n",
        )],
    );
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &["--todo"]);

    let todo = common::read_built(&site.join("TODO.html"));
    let body = common::article_body(&todo);
    assert!(
        body.contains(r#"<p><input disabled="" type="checkbox"/> loose open</p>"#),
        "{body}"
    );
    assert!(body.contains("loose second"), "{body}");
    assert!(!body.contains("loose done"), "{body}");
}

/// R12.2: a code block inside a task is collected verbatim, its line breaks intact.
#[test]
fn r12_2_code_block_inside_a_task_keeps_its_lines() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[(
            "README.md",
            "# Home\n\n- [ ] run this\n\n  ```sh\n  echo <one>\n  echo two\n  ```\n",
        )],
    );
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &["--todo"]);

    let todo = common::read_built(&site.join("TODO.html"));
    let body = common::article_body(&todo);
    assert!(
        body.contains(
            r#"<pre><code class="language-sh">echo &lt;one&gt;&#10;echo two&#10;</code></pre>"#
        ),
        "{body}"
    );
}

/// R12.6: inside a collected task, a fragment link points at the task's own page, a
/// footnote reference is kept as text, and a link to a missing file keeps pointing where
/// it did from the task's own directory.
#[test]
fn r12_6_fragments_footnotes_and_missing_links_are_rebased() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n"),
            (
                "guides/a.md",
                "# A\n\n- [ ] see [notes](#notes), note[^n] and [gone](gone.md)\n\n## Notes\n\n[^n]: the note\n",
            ),
        ],
    );
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &["--todo"]);

    let todo = common::read_built(&site.join("TODO.html"));
    let body = common::article_body(&todo);
    let hrefs = common::attr_values(body, "href");
    assert!(
        hrefs.contains(&"guides/a.html#notes"),
        "fragment: {hrefs:?}"
    );
    assert!(hrefs.contains(&"guides/gone.md"), "missing: {hrefs:?}");
    assert!(body.contains("note[^n]"), "footnote as text: {body}");
    assert!(!body.contains("footnote-reference"), "{body}");
}

/// R12.4: a root TODO file is found whatever its case and either Markdown extension.
#[test]
fn r12_4_root_todo_file_is_matched_casefolded_with_either_extension() {
    for (name, target) in [("Todo.md", "Todo.html"), ("TODO.markdown", "TODO.html")] {
        let dir = TempDir::new().expect("create dir");
        common::write_tree(
            dir.path(),
            &[
                ("README.md", "# Home\n\n- [ ] readme task\n"),
                (name, "# Mine\n\n- [ ] own task\n"),
            ],
        );
        let out = TempDir::new().expect("create out");
        let site = build(dir.path(), &out, &["--todo"]);

        let built = common::relative_paths(&site);
        assert!(built.iter().any(|path| path == target), "{name}: {built:?}");
        assert_eq!(
            built.iter().filter(|path| path.ends_with(".html")).count(),
            2,
            "{name}: no synthetic page beside it: {built:?}"
        );
        let todo = common::read_built(&site.join(target));
        let body = common::article_body(&todo);
        assert_eq!(body.matches("own task").count(), 1, "{name}: {body}");
        assert!(body.contains(">Collected Tasks</h1>"), "{name}: {body}");
        assert!(body.contains("readme task"), "{name}: {body}");
    }
}

/// R12.3: a root `TODO.md` removed by `--exclude` is not a page, so a synthetic one
/// stands in for it, and the excluded file's tasks are not collected.
#[test]
fn r12_3_excluded_todo_file_gets_a_synthetic_page() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n\n- [ ] readme task\n"),
            ("TODO.md", "# Mine\n\n- [ ] excluded task\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &["--todo", "--exclude", "TODO.md"]);

    let todo = common::read_built(&site.join("TODO.html"));
    assert!(common::page_title(&todo).starts_with("TODO "));
    let body = common::article_body(&todo);
    assert!(body.contains(">TODO</h1>"), "{body}");
    assert!(!body.contains("Collected Tasks"), "{body}");
    assert!(body.contains("readme task"), "{body}");
    assert!(!body.contains("excluded task"), "{body}");
}

/// R12.3: a synthetic `TODO.html` never overwrites a file already at that path; the
/// build fails at planning, naming it, and produces no output.
#[test]
fn r12_3_synthetic_page_colliding_with_a_file_is_a_hard_error() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n\n- [ ] task\n"),
            ("TODO.html", "<p>mine</p>\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");
    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
        "--todo",
    ]);
    assert!(!result.success(), "expected failure: {}", result.stderr);
    assert!(result.stderr.contains("planning"), "{}", result.stderr);
    assert!(result.stderr.contains("`TODO.html`"), "{}", result.stderr);
    assert!(
        common::is_absent_or_empty(&out_path),
        "no output on failure"
    );
}

/// R12.3: with no open task to collect, no `TODO.html` is generated, so a file already
/// at that path is not an error and, being referenced (R5.1), is copied as it is.
#[test]
fn r12_3_file_at_todo_html_is_kept_when_there_is_nothing_to_collect() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n\n[mine](TODO.html)\n"),
            ("TODO.html", "<p>mine</p>\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &["--todo"]);

    let copied = std::fs::read_to_string(site.join("TODO.html")).expect("file copied");
    assert!(copied.contains("<p>mine</p>"), "{copied}");
}

/// R12.4: a `TODO.md` ending inside an unclosed fence or HTML block does not swallow the
/// appended section, and anchors stay unique across the file and the section.
#[test]
fn r12_4_unclosed_constructs_in_todo_do_not_swallow_the_section() {
    for tail in [
        "```\nunclosed fence\n",
        "<!--\nunclosed comment\n",
        "<pre>\nunclosed pre\n",
    ] {
        let dir = TempDir::new().expect("create dir");
        common::write_tree(
            dir.path(),
            &[
                ("README.md", "# Home\n\n- [ ] readme task\n"),
                (
                    "TODO.md",
                    &format!("# TODO\n\n# Collected Tasks\n\n- [ ] own task\n\n{tail}"),
                ),
            ],
        );
        let out = TempDir::new().expect("create out");
        let site = build(dir.path(), &out, &["--todo"]);

        let todo = common::read_built(&site.join("TODO.html"));
        let body = common::article_body(&todo);
        assert!(
            body.contains(r#"<h1 id="collected-tasks">Collected Tasks</h1>"#),
            "{tail:?}: {body}"
        );
        assert!(
            body.contains(r#"<h1 id="collected-tasks-1">Collected Tasks</h1>"#),
            "{tail:?}: the appended heading is de-duplicated against the file's: {body}"
        );
        assert!(
            body.contains(r#"<a href="index.html">Home</a></h2>"#),
            "{tail:?}: {body}"
        );
        assert!(
            body.contains(r#"<input disabled="" type="checkbox"/> readme task</li>"#),
            "{tail:?}: {body}"
        );
        assert!(
            todo.contains(r##"href="#collected-tasks-1""##),
            "{tail:?}: the outline covers both parts"
        );
    }
}

/// Task tags render on ordinary pages, while protected and non-task brace groups remain prose.
#[test]
fn task_tags_render_on_ordinary_pages() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            (
                "README.md",
                r#"# Home {heading}

Outside {outside}.

- [ ] Plan a trip { Travel, , buy, travel }
- [ ] Budget {R&D's Travel}
- [ ] Code `{code}` and [link {link}](other.md) plus ![image {image}](pixel.png)
- [ ] escaped \{escaped\}, empty {}, nested {{nested}}, malformed {unfinished
- [ ] HTML <span data-value="{attribute}">inside {eligible}</span>
- [ ] Comment <!-- {payload} -->
"#,
            ),
            ("other.md", "# Other\n"),
            ("pixel.png", "image"),
        ],
    );
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &[]);

    let index = common::read_built(&site.join("index.html"));
    let body = common::article_body(&index);
    assert!(
        body.contains("data-ohm-tags=\"[&quot;travel&quot;,&quot;buy&quot;]\""),
        "canonical JSON keys and HTML-escaped quotes: {body}"
    );
    assert!(
        body.contains("data-ohm-tags=\"[&quot;r&amp;d&#39;s travel&quot;]\""),
        "author tag text is escaped in JSON metadata: {body}"
    );
    assert!(
        body.contains("data-ohm-tags=\"[&quot;eligible&quot;]\""),
        "text between inline HTML events remains task prose: {body}"
    );
    assert!(
        body.contains("class=\"ohm-task-tags\""),
        "badge group: {body}"
    );
    assert!(
        body.contains("<li class=\"ohm-task\">"),
        "an untagged task keeps the original opening tag: {body}"
    );
    assert!(
        body.contains(">Travel</span>"),
        "first spelling is retained: {body}"
    );
    assert!(
        body.contains("R&amp;D&#39;s Travel</span>"),
        "badge text is escaped: {body}"
    );
    assert!(
        body.contains("class=\"ohm-task-tag\">eligible</span>"),
        "HTML prose tag: {body}"
    );
    assert_eq!(
        body.matches("class=\"ohm-task-tag\"").count(),
        4,
        "one badge per unique tag: {body}"
    );
    assert_eq!(
        body.matches("data-ohm-tags=").count(),
        3,
        "only the three valid task groups are recognized: {body}"
    );
    for literal in [
        "{heading}",
        "{outside}",
        "<code>{code}</code>",
        "link {link}</a>",
        "alt=\"image {image}\"",
        "{escaped}",
        "{}",
        "{{nested}}",
        "{unfinished",
        "data-value=\"{attribute}\"",
        "<!-- {payload} -->",
    ] {
        assert!(
            body.contains(literal),
            "protected literal `{literal}`: {body}"
        );
    }
    assert!(
        !index.contains("data-ohm-todo"),
        "a normal page has no final TODO scope"
    );
    assert!(
        !body.contains("Filter by tags"),
        "ordinary pages have no TODO filtering control"
    );
}

/// The TODO source and collected tasks share tag rendering, with ownership kept per item.
#[test]
fn task_tags_render_on_collected_tasks_with_nested_ownership() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            (
                "README.md",
                "# Home\n\n- [ ] Source parent {Travel}\n  - [ ] Source child {Buy}\n  - [x] checked child {Hidden}\n- [x] checked source {Private}\n",
            ),
            (
                "TODO.md",
                "# Roadmap\n\n- [x] Already done {Personal}\n- [ ] Buy train {Travel}\n",
            ),
        ],
    );
    let out = TempDir::new().expect("create out");
    let site = build(dir.path(), &out, &["--todo"]);

    let home = common::read_built(&site.join("index.html"));
    let home_body = common::article_body(&home);
    let todo = common::read_built(&site.join("TODO.html"));
    let body = common::article_body(&todo);
    assert!(
        home_body.contains("data-ohm-tags=\"[&quot;travel&quot;]\""),
        "ordinary task metadata: {home_body}"
    );
    assert!(
        todo.contains("<article class=\"ohm-content\" data-ohm-todo>"),
        "only the final TODO page gets the filtering scope"
    );
    assert!(
        body.contains("data-ohm-todo-collected"),
        "appendix wrapper: {body}"
    );
    assert!(
        body.contains("Already done"),
        "checked TODO-owned tasks remain: {body}"
    );
    assert!(
        body.contains("data-ohm-tags=\"[&quot;personal&quot;]\""),
        "checked TODO-owned task metadata: {body}"
    );
    assert!(
        body.contains("data-ohm-tags=\"[&quot;travel&quot;]\""),
        "unchecked TODO-owned and collected tags are rendered: {body}"
    );
    assert!(
        body.contains("Source child"),
        "unchecked nested task is kept: {body}"
    );
    assert!(
        !body.contains("checked child") && !body.contains("checked source"),
        "checked source tasks are excluded from collection: {body}"
    );
    let group = marked_section(body, "data-ohm-todo-group");
    let parent = position(group, "Source parent");
    let parent_badge = position(group, ">Travel</span>");
    let child = position(group, "Source child");
    assert!(
        parent < parent_badge && parent_badge < child,
        "the parent's own badge follows its label and precedes its child: {group}"
    );
    let parent_li = group[..parent]
        .rfind("<li")
        .expect("parent list item opening tag");
    let parent_open_end = group[parent_li..]
        .find('>')
        .expect("parent list item opening tag is closed");
    let parent_item = &group[parent_li..parent_li + parent_open_end + 1];
    assert!(
        parent_item.contains("data-ohm-tags=\"[&quot;travel&quot;]\""),
        "parent owns only its own tag: {parent_item}"
    );
    assert!(
        !parent_item.contains("buy"),
        "child tags do not leak to the parent: {parent_item}"
    );
    assert!(
        group.contains("data-ohm-tags=\"[&quot;buy&quot;]\""),
        "nested task keeps its own tag: {group}"
    );
}
