//! Link and asset resolution: turns an authored relative reference into the
//! relative URL that must be emitted. See `docs/design/links.md` for the
//! normative algorithm this module implements (R2.7, R5.1-R5.4, R6.1-R6.7, R6.9).

use camino::{Utf8Path, Utf8PathBuf};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC};

use crate::model::{Site, SourceKind, SourceRel};

/// The set of bytes that must be percent-encoded when emitting a resolved
/// path. Ordinary filename characters and path separators survive untouched
/// so that decoding-then-encoding round-trips without double-encoding (R6.4).
const PATH_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'/')
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// The outcome of resolving one href or image src.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// Not ours to touch: emit byte-for-byte as written.
    Unchanged,
    /// Resolved to a page in the site; the final relative href.
    Page(String),
    /// Resolved to a non-Markdown file; the final relative href, plus the
    /// source path that must therefore be copied into the output (R5.1).
    Asset {
        /// The final relative href to emit.
        href: String,
        /// The source file that must be copied into the output.
        source: SourceRel,
    },
    /// Nothing matched. The caller warns and emits the original unchanged (R5.2, R6.6).
    Missing,
}

/// Lowercase. The single casefolding rule used for every lookup.
#[must_use]
pub fn fold(s: &str) -> String {
    s.to_lowercase()
}

/// Resolve `.` and `..` lexically against `base`. Never touches the filesystem,
/// so a symlink cannot redirect a link (R6.7). Returns `None` if the result
/// escapes above the root (a leading `..` survives) - the caller treats that as
/// `Missing` (R5.3).
#[must_use]
pub fn normalize(base: &Utf8Path, rel: &str) -> Option<Utf8PathBuf> {
    let mut stack: Vec<&str> = base
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect();

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

/// A relative POSIX path from directory `from_dir` to file `to`. Emits `../`
/// segments as needed; never emits a leading `/` (R11.1 - every internal link
/// is relative) and never returns an empty string (a same-directory target
/// yields just the file name).
#[must_use]
pub fn relative_to(from_dir: &Utf8Path, to: &Utf8Path) -> String {
    let from_comps: Vec<&str> = from_dir
        .as_str()
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let to_comps: Vec<&str> = to.as_str().split('/').filter(|s| !s.is_empty()).collect();

    let mut common = 0;
    while common < from_comps.len()
        && common + 1 < to_comps.len()
        && from_comps[common] == to_comps[common]
    {
        common += 1;
    }

    let ups = from_comps.len() - common;
    let mut parts: Vec<&str> = Vec::with_capacity(ups + (to_comps.len() - common));
    parts.extend(std::iter::repeat_n("..", ups));
    parts.extend_from_slice(&to_comps[common..]);
    parts.join("/")
}

/// Split an authored reference into `(path, query, fragment)`. The fragment is
/// taken at the first `#`, then the query at the first `?` of what remains.
/// Both include their leading delimiter, or are empty.
#[must_use]
pub fn split_reference(raw: &str) -> (&str, &str, &str) {
    let (before_fragment, fragment) = match raw.find('#') {
        Some(idx) => (&raw[..idx], &raw[idx..]),
        None => (raw, ""),
    };
    let (path, query) = match before_fragment.find('?') {
        Some(idx) => (&before_fragment[..idx], &before_fragment[idx..]),
        None => (before_fragment, ""),
    };
    (path, query, fragment)
}

/// True when `s` begins with a URL scheme, i.e. matches `^[A-Za-z][A-Za-z0-9+.-]*:`.
fn has_scheme(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    for c in chars {
        if c == ':' {
            return true;
        }
        if c.is_ascii_alphanumeric() || c == '+' || c == '.' || c == '-' {
            continue;
        }
        return false;
    }
    false
}

/// Resolve one authored reference.
///
/// * `raw` - the href or src exactly as the author wrote it.
/// * `source_dir` - the linking page's own source directory (what the author
///   wrote relative to). `""` for a top-level page.
/// * `target_dir` - the directory the emitted URL must be relative to. Identical
///   to `source_dir` for every page except a `--root` file taken from a
///   subdirectory, whose target dir is the output root (R2.7).
#[must_use]
pub fn resolve(raw: &str, source_dir: &Utf8Path, target_dir: &Utf8Path, site: &Site) -> Resolved {
    if raw.trim().is_empty() || raw.starts_with('#') || raw.starts_with('/') || has_scheme(raw) {
        return Resolved::Unchanged;
    }

    let (path, query, fragment) = split_reference(raw);
    if path.is_empty() {
        return Resolved::Unchanged;
    }

    let decoded = match percent_encoding::percent_decode_str(path).decode_utf8() {
        Ok(decoded) => decoded,
        Err(_) => return Resolved::Missing,
    };

    let normalized = match normalize(source_dir, &decoded) {
        Some(normalized) => normalized,
        None => return Resolved::Missing,
    };

    let folded = fold(normalized.as_str());
    let (target_path, asset_source) = match site.lookup(&folded) {
        Some(SourceKind::Page(index)) => (site.page(*index).target.as_path().to_owned(), None),
        Some(SourceKind::Asset(source)) => (source.as_path().to_owned(), Some(source.clone())),
        None => return Resolved::Missing,
    };

    let href_path = relative_to(target_dir, &target_path);
    let encoded = percent_encoding::utf8_percent_encode(&href_path, PATH_ENCODE_SET);
    let href = format!("{encoded}{query}{fragment}");

    match asset_source {
        Some(source) => Resolved::Asset { href, source },
        None => Resolved::Page(href),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Page, TargetPath};
    use std::collections::HashMap;

    fn site_with(entries: &[(&str, SourceKind)], pages: Vec<Page>) -> Site {
        let mut by_source = HashMap::new();
        for (path, kind) in entries {
            by_source.insert(fold(path), kind.clone());
        }
        Site {
            pages,
            by_source,
            nav: Vec::new(),
        }
    }

    fn page(source: &str, target: &str, is_root: bool) -> Page {
        Page {
            source: SourceRel::new(source),
            target: TargetPath::new(target),
            title: String::new(),
            is_root,
        }
    }

    #[test]
    fn r6_1_page_link_gets_html_extension() {
        let pages = vec![page("docs/guide.md", "docs/guide.html", false)];
        let site = site_with(&[("docs/guide.md", SourceKind::Page(0))], pages);

        let resolved = resolve("docs/guide.md", Utf8Path::new(""), Utf8Path::new(""), &site);
        assert_eq!(resolved, Resolved::Page("docs/guide.html".to_string()));
    }

    #[test]
    fn r6_2_link_to_root_file_resolves_to_index() {
        let pages = vec![page("README.md", "index.html", true)];
        let site = site_with(&[("README.md", SourceKind::Page(0))], pages);

        let resolved = resolve(
            "../README.md",
            Utf8Path::new("docs"),
            Utf8Path::new("docs"),
            &site,
        );
        assert_eq!(resolved, Resolved::Page("../index.html".to_string()));
    }

    #[test]
    fn r6_3_query_and_fragment_preserved() {
        let pages = vec![page("guide.md", "guide.html", false)];
        let site = site_with(&[("guide.md", SourceKind::Page(0))], pages);

        let resolved = resolve(
            "guide.md?x=1#frag",
            Utf8Path::new(""),
            Utf8Path::new(""),
            &site,
        );
        assert_eq!(resolved, Resolved::Page("guide.html?x=1#frag".to_string()));
    }

    #[test]
    fn r6_4_percent_encoding_round_trips() {
        let pages = vec![page("my file.md", "my file.html", false)];
        let site = site_with(&[("my file.md", SourceKind::Page(0))], pages);

        let resolved = resolve("my%20file.md", Utf8Path::new(""), Utf8Path::new(""), &site);
        assert_eq!(resolved, Resolved::Page("my%20file.html".to_string()));
    }

    #[test]
    fn r6_5_never_touched_forms_are_unchanged() {
        let site = Site::default();
        let forms = [
            "https://example.com/x",
            "//example.com/x",
            "/absolute/path.md",
            "#section",
            "mailto:a@b.com",
            "data:text/plain,hi",
        ];
        for form in forms {
            let resolved = resolve(form, Utf8Path::new(""), Utf8Path::new(""), &site);
            assert_eq!(resolved, Resolved::Unchanged, "form: {form}");
        }
    }

    #[test]
    fn r6_6_unknown_target_is_missing() {
        let site = Site::default();
        let resolved = resolve("nowhere.md", Utf8Path::new(""), Utf8Path::new(""), &site);
        assert_eq!(resolved, Resolved::Missing);
    }

    #[test]
    fn r6_7_dotdot_resolves_lexically_against_source_dir() {
        let pages = vec![page("a/b.md", "a/b.html", false)];
        let site = site_with(&[("a/b.md", SourceKind::Page(0))], pages);

        let resolved = resolve(
            "../b.md",
            Utf8Path::new("a/sub"),
            Utf8Path::new("a/sub"),
            &site,
        );
        assert_eq!(resolved, Resolved::Page("../b.html".to_string()));
    }

    #[test]
    fn r2_7_root_from_subdirectory_rebases_its_own_links() {
        let pages = vec![page("docs/guide.md", "docs/guide.html", false)];
        let site = site_with(
            &[
                ("docs/guide.md", SourceKind::Page(0)),
                (
                    "docs/img/d.png",
                    SourceKind::Asset(SourceRel::new("docs/img/d.png")),
                ),
            ],
            pages,
        );

        let link = resolve("guide.md", Utf8Path::new("docs"), Utf8Path::new(""), &site);
        assert_eq!(link, Resolved::Page("docs/guide.html".to_string()));

        let image = resolve("img/d.png", Utf8Path::new("docs"), Utf8Path::new(""), &site);
        assert_eq!(
            image,
            Resolved::Asset {
                href: "docs/img/d.png".to_string(),
                source: SourceRel::new("docs/img/d.png"),
            }
        );
    }
}
