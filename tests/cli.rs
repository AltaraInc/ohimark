//! The command-line surface and process behaviour (R9, R10.4, R10.5, R2.1, R2.2).

mod common;

use tempfile::TempDir;

/// R9.1: omitting the positional `DIR` is an error with a non-zero exit and a usage message.
#[test]
fn r9_1_missing_dir_is_a_usage_error() {
    let out = TempDir::new().expect("create out dir");
    let result = common::run(["--out", out.path().to_str().unwrap()]);
    assert!(
        !result.success(),
        "expected non-zero exit, got success. stderr: {}",
        result.stderr
    );
    assert!(
        result.stderr.to_lowercase().contains("usage"),
        "no usage message on stderr: {}",
        result.stderr
    );
}

/// R9.1: omitting `--out` is an error with a non-zero exit and a usage message.
#[test]
fn r9_1_missing_out_is_a_usage_error() {
    let dir = TempDir::new().expect("create dir");
    let result = common::run([dir.path().to_str().unwrap()]);
    assert!(
        !result.success(),
        "expected non-zero exit, got success. stderr: {}",
        result.stderr
    );
    assert!(
        result.stderr.to_lowercase().contains("usage"),
        "no usage message on stderr: {}",
        result.stderr
    );
}

/// R9.2: a `DIR` that does not exist is an error with a non-zero exit.
#[test]
fn r9_2_nonexistent_dir_is_rejected() {
    let base = TempDir::new().expect("create base");
    let missing = base.path().join("does-not-exist");
    let out = base.path().join("out");
    let result = common::run([missing.to_str().unwrap(), "--out", out.to_str().unwrap()]);
    assert!(
        !result.success(),
        "expected non-zero exit for a missing DIR, stderr: {}",
        result.stderr
    );
}

/// R9.2: a `DIR` that is a regular file (not a directory) is an error with a non-zero exit.
#[test]
fn r9_2_dir_that_is_a_file_is_rejected() {
    let base = TempDir::new().expect("create base");
    let file = base.path().join("a-file");
    std::fs::write(&file, b"not a directory").expect("write file");
    let out = base.path().join("out");
    let result = common::run([file.to_str().unwrap(), "--out", out.to_str().unwrap()]);
    assert!(
        !result.success(),
        "expected non-zero exit for a DIR that is a file, stderr: {}",
        result.stderr
    );
}

/// R9.3: a pre-populated output directory is refused, and its existing contents are untouched.
#[test]
fn r9_3_non_empty_output_directory_is_refused() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(dir.path(), &[("README.md", "# Home\n")]);

    let out = TempDir::new().expect("create out dir");
    let sentinel = out.path().join("sentinel.txt");
    std::fs::write(&sentinel, b"do not touch me").expect("write sentinel");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out.path().to_str().unwrap(),
    ]);
    assert!(
        !result.success(),
        "expected non-zero exit for a non-empty --out, stderr: {}",
        result.stderr
    );

    let contents = std::fs::read(&sentinel).expect("sentinel must survive a refused build");
    assert_eq!(
        contents, b"do not touch me",
        "sentinel file contents changed after a refused build"
    );
    let entries: Vec<_> = std::fs::read_dir(out.path())
        .expect("read out dir")
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        entries,
        vec![std::ffi::OsString::from("sentinel.txt")],
        "out directory contents changed after a refused build: {entries:?}"
    );
}

/// R9.4: the output directory is created when absent, and accepted when empty. Both exit zero.
#[test]
fn r9_4_absent_and_empty_output_directories_are_accepted() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(dir.path(), &[("README.md", "# Home\n")]);

    let base = TempDir::new().expect("create base");

    // Absent: `emit` must create it.
    let absent_out = base.path().join("absent-out");
    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        absent_out.to_str().unwrap(),
    ]);
    assert!(
        result.success(),
        "build into an absent --out failed: {}",
        result.stderr
    );
    assert!(
        absent_out.join("index.html").is_file(),
        "absent --out was not created and populated"
    );

    // Empty: pre-create it, then build into it.
    let empty_out = base.path().join("empty-out");
    std::fs::create_dir(&empty_out).expect("pre-create empty out");
    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        empty_out.to_str().unwrap(),
    ]);
    assert!(
        result.success(),
        "build into a pre-existing empty --out failed: {}",
        result.stderr
    );
    assert!(
        empty_out.join("index.html").is_file(),
        "empty --out was not populated"
    );
}

/// R9.5: `--help` exits zero and mentions the tool, the positional argument, and every flag.
#[test]
fn r9_5_help_describes_tool_positional_and_every_flag() {
    let result = common::run(["--help"]);
    assert!(
        result.success(),
        "--help must exit zero, stderr: {}",
        result.stderr
    );
    let stdout = &result.stdout;
    assert!(stdout.contains("ohimark"), "help omits the tool name");
    assert!(stdout.contains("DIR"), "help omits the positional DIR");
    assert!(stdout.contains("--out"), "help omits --out");
    assert!(stdout.contains("--root"), "help omits --root");
    assert!(stdout.contains("--stylesheet"), "help omits --stylesheet");
    assert!(stdout.contains("--exclude"), "help omits --exclude");
}

/// R9.6: flags are accepted both before and after the positional argument.
#[test]
fn r9_6_flags_accepted_before_or_after_positional() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(dir.path(), &[("README.md", "# Home\n")]);
    let base = TempDir::new().expect("create base");

    let before_out = base.path().join("before");
    let before = common::run([
        "--out",
        before_out.to_str().unwrap(),
        dir.path().to_str().unwrap(),
    ]);
    assert!(
        before.success(),
        "flags before positional failed: {}",
        before.stderr
    );
    assert!(before_out.join("index.html").is_file());

    let after_out = base.path().join("after");
    let after = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        after_out.to_str().unwrap(),
    ]);
    assert!(
        after.success(),
        "flags after positional failed: {}",
        after.stderr
    );
    assert!(after_out.join("index.html").is_file());
}

/// R9.6: `--exclude` accumulates across occurrences; each supplied glob excludes its match.
#[test]
fn r9_6_exclude_accumulates() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            ("README.md", "# Home\n"),
            ("keep.md", "# Keep\n"),
            ("drop-a.md", "# A\n"),
            ("drop-b.md", "# B\n"),
        ],
    );
    let out = TempDir::new().expect("create out");
    // Do not build into `out` (must be empty/absent); use a subpath instead.
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
        "--exclude",
        "drop-a.md",
        "--exclude",
        "drop-b.md",
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);
    assert!(out_path.join("keep.html").is_file(), "keep.md must survive");
    assert!(
        !out_path.join("drop-a.html").exists(),
        "drop-a.md must be excluded by the first glob"
    );
    assert!(
        !out_path.join("drop-b.html").exists(),
        "drop-b.md must be excluded by the second glob"
    );
}

/// R9.6: for `--out`, the last occurrence wins — assert it, not just trust it.
#[test]
fn r9_6_last_out_wins() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(dir.path(), &[("README.md", "# Home\n")]);
    let base = TempDir::new().expect("create base");
    let first = base.path().join("first");
    let second = base.path().join("second");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        first.to_str().unwrap(),
        "--out",
        second.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);
    assert!(
        !first.exists(),
        "the first, overridden --out must not have been written to"
    );
    assert!(
        second.join("index.html").is_file(),
        "the last --out must be the one actually written"
    );
}

/// R9.6: for `--root`, the last occurrence wins — assert it, not just trust it.
#[test]
fn r9_6_last_root_wins() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[("a.md", "# From A\n"), ("b.md", "# From B\n")],
    );
    let out = TempDir::new().expect("create base");
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
        "--root",
        "a.md",
        "--root",
        "b.md",
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);
    let index = common::read_built(&out_path.join("index.html"));

    // The whole document is not the right scope: the sidebar (R4.6, R4.7) links to every
    // other page by its title, so "From A" legitimately appears there too, as the nav entry
    // for a.html. Scope the check to the page's own identity instead: its <title> and its
    // own <article> body.
    let title = common::page_title(&index);
    assert_eq!(
        title, "From B",
        "index.html's <title> must come from the last --root (b.md)"
    );

    let body = common::article_body(&index);
    assert!(
        body.contains("From B"),
        "index.html's own content must come from the last --root (b.md): {body}"
    );
    assert!(
        !body.contains("From A"),
        "index.html's own content must not come from the overridden --root (a.md): {body}"
    );
}

/// R10.4: a forced failure (a missing root file) prints a message on stderr naming the
/// failing stage, and exits non-zero.
#[test]
fn r10_4_failure_names_the_failing_stage() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(dir.path(), &[("notes.md", "# Notes\n")]); // no README.md
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(
        !result.success(),
        "expected non-zero exit for a missing root file"
    );
    assert!(
        result.stderr.contains("planning"),
        "stderr must name the failing stage (planning): {}",
        result.stderr
    );
    assert!(
        result.stderr.contains("README.md"),
        "stderr must name the missing root file: {}",
        result.stderr
    );
}

/// R10.5: warnings (a broken relative Markdown link and a missing image) are written to
/// stderr only, stdout stays completely empty, and the exit code is still zero.
#[test]
fn r10_5_warnings_go_to_stderr_only_and_exit_is_zero() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[(
            "README.md",
            "# Home\n\n[Missing](nowhere.md)\n\n![Missing](missing.png)\n",
        )],
    );
    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(
        result.success(),
        "a page with only warnings must still exit zero: {}",
        result.stderr
    );
    assert!(
        result.stdout.is_empty(),
        "stdout must be completely empty, got: {:?}",
        result.stdout
    );
    assert!(
        !result.stderr.is_empty(),
        "expected warnings on stderr, got none"
    );
    assert!(
        result.stderr.contains("nowhere.md"),
        "stderr must name the broken link target: {}",
        result.stderr
    );
    assert!(
        result.stderr.contains("missing.png"),
        "stderr must name the missing image target: {}",
        result.stderr
    );
}

/// R2.2: a tree with no `README.md` fails, naming it, and produces no output.
#[test]
fn r2_2_missing_root_file_fails_and_produces_no_output() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(dir.path(), &[("notes.md", "# Notes\n")]);
    let base = TempDir::new().expect("create base");
    let out_path = base.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(!result.success(), "expected non-zero exit");
    assert!(
        result.stderr.contains("README.md"),
        "stderr must name the missing root file: {}",
        result.stderr
    );
    assert!(
        common::is_absent_or_empty(&out_path),
        "no output must be produced on failure"
    );
}

/// R2.2: a `README.md` excluded by a `.gitignore` is treated as absent — same failure, same
/// "no output" guarantee.
#[test]
fn r2_2_root_file_excluded_by_gitignore_fails_and_produces_no_output() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[("README.md", "# Home\n"), (".gitignore", "README.md\n")],
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
        "expected non-zero exit for a git-ignored root file"
    );
    assert!(
        result.stderr.contains("README.md"),
        "stderr must name the excluded root file: {}",
        result.stderr
    );
    assert!(
        common::is_absent_or_empty(&out_path),
        "no output must be produced on failure"
    );
}

/// R2.1: `--root` resolving outside `DIR` is a hard error with a non-zero exit.
#[test]
fn r2_1_root_escaping_dir_is_a_hard_error() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(dir.path(), &[("README.md", "# Home\n")]);
    let base = TempDir::new().expect("create base");
    let out_path = base.path().join("site");

    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
        "--root",
        "../outside.md",
    ]);
    assert!(
        !result.success(),
        "expected non-zero exit for a --root escaping DIR"
    );
    assert!(
        common::is_absent_or_empty(&out_path),
        "no output must be produced on failure"
    );
}
