//! Shared helpers for the integration suite: build a fixture tree in a
//! `TempDir`, run the real `ohimark` binary against it, and inspect the
//! result. Every fixture lives under a `tempfile::TempDir` — nothing is ever
//! written outside one. Not every test file uses every helper here.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

/// Captured result of running the `ohimark` binary once.
pub struct RunResult {
    pub status: std::process::ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl RunResult {
    pub fn success(&self) -> bool {
        self.status.success()
    }
}

/// Build a fixture tree under `root`: `(relative_path, utf8_contents)` pairs,
/// creating parent directories as needed. `root` must already exist (it is
/// ordinarily a `TempDir`'s path).
pub fn write_tree(root: &Path, files: &[(&str, &str)]) {
    for (rel, contents) in files {
        write_file(root, rel, contents.as_bytes());
    }
}

/// Write one file with raw bytes (for binary fixtures, e.g. images).
pub fn write_file(root: &Path, rel: &str, contents: &[u8]) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|e| panic!("create parent dirs for {rel}: {e}"));
    }
    std::fs::write(&path, contents).unwrap_or_else(|e| panic!("write fixture {rel}: {e}"));
}

/// Run the built `ohimark` binary with `args`, capturing status/stdout/stderr.
pub fn run<I, S>(args: I) -> RunResult
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_with_envs(args, &[])
}

/// Run `ohimark` with extra environment variables set — e.g. `TMPDIR`, to pin
/// down where its internal staging directory is created (R10.3).
pub fn run_with_envs<I, S>(args: I, envs: &[(&str, &str)]) -> RunResult
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(env!("CARGO_BIN_EXE_ohimark"));
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command
        .output()
        .expect("failed to execute the ohimark binary");
    RunResult {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Read a built file to a `String`.
pub fn read_built(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read built file {}: {e}", path.display()))
}

/// A full recursive snapshot of every regular file under `root`: relative
/// POSIX path -> bytes. Used to compare directory trees byte-for-byte
/// (R11.3), or to prove a directory is untouched (R10.1). An absent `root`
/// snapshots as empty.
pub fn snapshot_dir(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut map = BTreeMap::new();
    walk_into(root, root, &mut map);
    map
}

fn walk_into(base: &Path, dir: &Path, map: &mut BTreeMap<String, Vec<u8>>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries {
        let entry = entry.expect("read fixture dir entry");
        let path = entry.path();
        let file_type = entry.file_type().expect("read fixture entry file type");
        if file_type.is_dir() {
            walk_into(base, &path, map);
        } else if file_type.is_file() {
            let rel = path
                .strip_prefix(base)
                .expect("entry under its own base")
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = std::fs::read(&path).expect("read fixture file");
            map.insert(rel, bytes);
        }
    }
}

/// The relative paths of every file present under `root`, for tests that
/// only care about the file set, not the bytes.
pub fn relative_paths(root: &Path) -> Vec<String> {
    snapshot_dir(root).into_keys().collect()
}

/// True when `path` does not exist, or exists as a directory with no entries.
pub fn is_absent_or_empty(path: &Path) -> bool {
    match std::fs::read_dir(path) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => true,
    }
}

/// True when `dir` can hold two entries whose names differ only by case
/// (e.g. `Guide.md` and `guide.md`). False on a case-insensitive,
/// case-preserving filesystem (default macOS/APFS), where the second write
/// silently overwrites the first and only one entry survives. R2.5 requires
/// both sources to exist simultaneously; callers use this to skip rather
/// than report a false pass or a false defect when the host filesystem
/// cannot represent the fixture at all.
pub fn filesystem_is_case_sensitive(dir: &Path) -> bool {
    let a = dir.join("ohm-case-probe.tmp");
    let b = dir.join("OHM-CASE-PROBE.tmp");
    std::fs::write(&a, b"a").expect("write case probe a");
    std::fs::write(&b, b"b").expect("write case probe b");
    let count = std::fs::read_dir(dir).expect("read case probe dir").count();
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
    count >= 2
}

/// The text between `<title>` and `</title>`. A whole-page `contains` check cannot
/// distinguish "this page's own title" from "the sidebar happens to mention that text",
/// since the sidebar (R4.6, R4.7) links to every other page by its title too — scope title
/// assertions through this instead.
pub fn page_title(html: &str) -> &str {
    let open = "<title>";
    let start = html.find(open).expect("no <title> found") + open.len();
    let rest = &html[start..];
    let end = rest.find("</title>").expect("no </title> found");
    &rest[..end]
}

/// The inner HTML of `<article class="ohm-content">...</article>`, i.e. the page's own
/// rendered body, excluding the sidebar and outline. Scope content assertions through this
/// rather than the whole document, for the same reason as `page_title`.
pub fn article_body(html: &str) -> &str {
    let open = "<article class=\"ohm-content\">";
    let start = html.find(open).expect("no <article> found") + open.len();
    let rest = &html[start..];
    let end = rest.find("</article>").expect("no </article> found");
    &rest[..end]
}

/// Every `attr="..."` value appearing anywhere in `html`, in order (e.g.
/// `attr = "href"` or `attr = "src"`).
pub fn attr_values<'a>(html: &'a str, attr: &str) -> Vec<&'a str> {
    let needle = format!("{attr}=\"");
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(pos) = rest.find(&needle) {
        rest = &rest[pos + needle.len()..];
        match rest.find('"') {
            Some(end) => {
                out.push(&rest[..end]);
                rest = &rest[end + 1..];
            }
            None => break,
        }
    }
    out
}
