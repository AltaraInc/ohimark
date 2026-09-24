//! Output properties: relocatability, process hygiene, the Mermaid/theme
//! contract, and the no-JavaScript absence of script-injected controls
//! (R11.1, R10.1-R10.3, R7.4, R7.7, R8.2, R8.6, R8.10).

mod common;

use tempfile::TempDir;

/// R11.1: no absolute path of `DIR`, of the output directory, or of the system temp
/// directory appears anywhere in the built site, and no internal `href`/`src` is
/// root-absolute.
#[test]
fn r11_1_built_site_is_relocatable() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[
            (
                "README.md",
                "# Home\n\n[Guide](docs/guide.md)\n\n![Img](docs/img.png)\n",
            ),
            ("docs/guide.md", "# Guide\n"),
        ],
    );
    common::write_file(dir.path(), "docs/img.png", &tiny_png());

    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");
    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    let dir_abs = dir.path().to_string_lossy().into_owned();
    let out_abs = out_path.to_string_lossy().into_owned();
    let tmp_abs = std::env::temp_dir().to_string_lossy().into_owned();

    for (rel, bytes) in common::snapshot_dir(&out_path) {
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue; // a binary asset: nothing to grep for an absolute path.
        };
        assert!(!text.contains(&dir_abs), "{rel} leaks DIR's absolute path");
        assert!(
            !text.contains(&out_abs),
            "{rel} leaks the output directory's absolute path"
        );
        assert!(
            !text.contains(&tmp_abs),
            "{rel} leaks the system temp directory's absolute path"
        );
        if rel.ends_with(".html") {
            for href in common::attr_values(text, "href") {
                assert!(
                    !href.starts_with('/'),
                    "{rel}: href is root-absolute: {href}"
                );
            }
            for src in common::attr_values(text, "src") {
                assert!(!src.starts_with('/'), "{rel}: src is root-absolute: {src}");
            }
        }
    }
}

/// R10.1: a successful build never writes anything under `DIR`.
#[test]
fn r10_1_dir_is_untouched_by_a_successful_build() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(dir.path(), &[("README.md", "# Home\n")]);
    let before = common::snapshot_dir(dir.path());

    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");
    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    let after = common::snapshot_dir(dir.path());
    assert_eq!(
        before, after,
        "DIR must be byte-for-byte unchanged after a successful build"
    );
}

/// R10.1: a failed build never writes anything under `DIR` either.
#[test]
fn r10_1_dir_is_untouched_by_a_failed_build() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(dir.path(), &[("notes.md", "# Notes\n")]); // no README.md: forces failure
    let before = common::snapshot_dir(dir.path());

    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");
    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(!result.success(), "expected the build to fail");

    let after = common::snapshot_dir(dir.path());
    assert_eq!(
        before, after,
        "DIR must be byte-for-byte unchanged after a failed build"
    );
}

/// R10.2 / R10.3: a build that fails after staging has begun (here: `--out`'s parent is not
/// writable, so the final move-into-place fails) leaves the output directory absent or
/// empty, and its staging directory does not survive. `TMPDIR` is pinned to a private, empty
/// directory for the call so "no temporary directory survives" is an exact check rather than
/// a guess about the system temp directory's ordinary contents.
#[test]
fn r10_2_and_r10_3_failed_build_leaves_no_output_and_no_leftover_staging_dir() {
    let base = TempDir::new().expect("create base");
    let dir = base.path().join("src");
    std::fs::create_dir(&dir).expect("create src dir");
    common::write_tree(&dir, &[("README.md", "# Home\n")]);

    let readonly_parent = base.path().join("readonly");
    std::fs::create_dir(&readonly_parent).expect("create readonly parent");
    set_readonly(&readonly_parent);
    let out_path = readonly_parent.join("site"); // never created: parent forbids it

    let tmp_home = TempDir::new().expect("create tmp home");
    assert!(
        common::is_absent_or_empty(tmp_home.path()),
        "tmp home must start empty"
    );

    let result = common::run_with_envs(
        [dir.to_str().unwrap(), "--out", out_path.to_str().unwrap()],
        &[("TMPDIR", tmp_home.path().to_str().unwrap())],
    );

    set_writable(&readonly_parent); // restore, so TempDir can clean up `base` on drop

    assert!(
        !result.success(),
        "expected the build to fail when --out cannot be created, stderr: {}",
        result.stderr
    );
    assert!(
        common::is_absent_or_empty(&out_path),
        "output directory must be absent or empty after a failed build"
    );
    assert!(
        common::is_absent_or_empty(tmp_home.path()),
        "no staging directory must survive a failed build"
    );
}

#[cfg(unix)]
fn set_readonly(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).expect("stat dir").permissions();
    perms.set_mode(0o555); // read + execute, no write: entries cannot be added or removed.
    std::fs::set_permissions(path, perms).expect("make dir read-only");
}

#[cfg(unix)]
fn set_writable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).expect("stat dir").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).expect("restore dir permissions");
}

/// R7.4 + R8.2: a page with several Mermaid diagram families each renders to an inline
/// `<svg>` retaining its diagram source, and every colour in every diagram is expressed as a
/// `var(--ohm-dg-…)` custom property — no raw `#rrggbb` literal survives inside any `<svg>`,
/// which is the whole point of one render serving both themes.
#[test]
fn r7_4_and_r8_2_diagram_families_render_to_themed_inline_svg() {
    let dir = TempDir::new().expect("create dir");
    let content = r###"# Diagrams

```mermaid
flowchart TD
A --> B
```

```mermaid
sequenceDiagram
Alice->>Bob: Hello
```

```mermaid
pie title Pets
"Dogs" : 40
"Cats" : 60
```

```mermaid
stateDiagram-v2
[*] --> Still
Still --> [*]
```
"###;
    common::write_tree(dir.path(), &[("README.md", content)]);

    let out = TempDir::new().expect("create out");
    let out_path = out.path().join("site");
    let result = common::run([
        dir.path().to_str().unwrap(),
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(result.success(), "build failed: {}", result.stderr);

    let index = common::read_built(&out_path.join("index.html"));
    let body = common::article_body(&index);

    let figure_count = body.matches("<figure class=\"ohm-diagram\">").count();
    assert_eq!(figure_count, 4, "expected 4 rendered diagrams: {body}");
    let svg_count = body.matches("<svg").count();
    assert_eq!(svg_count, 4, "expected 4 inline <svg> elements: {body}");

    for source_fragment in [
        "flowchart TD",
        "sequenceDiagram",
        "pie title Pets",
        "stateDiagram-v2",
    ] {
        assert!(
            body.contains(source_fragment),
            "diagram source `{source_fragment}` must be retained in the page"
        );
    }

    let svg_regions = extract_svg_regions(body);
    assert_eq!(svg_regions.len(), 4, "expected 4 <svg>...</svg> regions");
    for (i, region) in svg_regions.iter().enumerate() {
        assert!(
            region.contains("var(--ohm-dg-"),
            "diagram {i} carries no CSS custom property colours"
        );
        if let Some(hex) = find_hex_color_literal(region) {
            panic!("diagram {i} leaks a raw hex colour literal `{hex}` into its <svg>");
        }
    }
}

/// R7.7: a malformed `mermaid` fence produces one warning naming the page, falls back to an
/// ordinary fenced code block, and the build still succeeds with no `<svg>` for that block.
#[test]
fn r7_7_malformed_mermaid_fence_falls_back_with_a_warning() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[(
            "README.md",
            "# Bad\n\n```mermaid\nthis is not a valid diagram {{{\n```\n",
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
        "a malformed diagram must still exit zero: {}",
        result.stderr
    );

    let warning_lines: Vec<&str> = result
        .stderr
        .lines()
        .filter(|line| line.contains("mermaid"))
        .collect();
    assert_eq!(
        warning_lines.len(),
        1,
        "expected exactly one mermaid warning, got: {:?}",
        result.stderr
    );
    assert!(
        warning_lines[0].contains("README.md"),
        "warning must name the page: {}",
        warning_lines[0]
    );

    let index = common::read_built(&out_path.join("index.html"));
    let body = common::article_body(&index);
    assert!(
        body.contains("<pre><code class=\"language-mermaid\">"),
        "expected a fenced code block fallback: {body}"
    );
    assert!(
        !body.contains("<svg"),
        "a malformed diagram must not produce an <svg>: {body}"
    );
}

/// R8.10 (no-JavaScript half) + R8.6 (no-JavaScript half): with JavaScript disabled the copy
/// control on a fenced code block and on a diagram, and the graphic/source toggle on a
/// diagram, are all absent from the built markup — they are injected by `ohimark.js`, never
/// present but inert.
#[test]
fn r8_10_copy_controls_are_absent_without_javascript() {
    let dir = TempDir::new().expect("create dir");
    common::write_tree(
        dir.path(),
        &[(
            "README.md",
            "# Home\n\n```rust\nfn main() {}\n```\n\n```mermaid\nflowchart TD\nA --> B\n```\n",
        )],
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
    let body = common::article_body(&index);

    assert!(
        body.contains("<pre><code class=\"language-rust\">"),
        "expected the fenced code block to be present: {body}"
    );
    assert!(
        body.contains("class=\"ohm-diagram\""),
        "expected the diagram figure to be present: {body}"
    );
    assert!(
        body.contains("<pre class=\"ohm-diagram-source\">"),
        "expected the diagram's source to be present: {body}"
    );

    assert!(
        !body.contains("ohm-copy-button"),
        "no copy control must appear without JavaScript: {body}"
    );
    assert!(
        !body.contains("<button"),
        "no toggle or copy control must appear without JavaScript: {body}"
    );
}

/// Every `<svg ...>...</svg>` region in `html`, in order.
fn extract_svg_regions(html: &str) -> Vec<&str> {
    let mut regions = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<svg") {
        let after_start = &rest[start..];
        match after_start.find("</svg>") {
            Some(end) => {
                regions.push(&after_start[..end + "</svg>".len()]);
                rest = &after_start[end + "</svg>".len()..];
            }
            None => break,
        }
    }
    regions
}

/// The first `#rgb`, `#rrggbb` or `#rrggbbaa` hex colour literal found in `s`, if any.
fn find_hex_color_literal(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            let mut j = i + 1;
            while j < bytes.len() && j - i - 1 < 8 && (bytes[j] as char).is_ascii_hexdigit() {
                j += 1;
            }
            let n = j - i - 1;
            // A boundary check: the run must not continue into a 9th+ hex digit
            // (which would mean this isn't a clean 3/6/8-digit token).
            let boundary_ok = bytes
                .get(j)
                .is_none_or(|b| !(*b as char).is_ascii_hexdigit());
            if boundary_ok && matches!(n, 3 | 6 | 8) {
                return Some(s[i..j].to_string());
            }
        }
        i += 1;
    }
    None
}

/// A minimal, valid PNG signature plus a little body.
fn tiny_png() -> Vec<u8> {
    let mut bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(b"not a real png body, just bytes to copy verbatim");
    bytes
}
