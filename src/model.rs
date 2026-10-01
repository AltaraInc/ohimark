//! Shared domain types, errors, and the warning sink. No I/O here.

use camino::{Utf8Path, Utf8PathBuf};

/// A `DIR`-relative POSIX path to an input file.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceRel(Utf8PathBuf);

impl SourceRel {
    /// Build a `SourceRel` from any path-like value.
    #[must_use]
    pub fn new(path: impl Into<Utf8PathBuf>) -> Self {
        Self(path.into())
    }

    /// Borrow the path.
    #[must_use]
    pub fn as_path(&self) -> &Utf8Path {
        &self.0
    }

    /// Borrow the path as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Lowercased form, used as a map key. Case-insensitive matching everywhere.
    #[must_use]
    pub fn folded(&self) -> String {
        self.0.as_str().to_lowercase()
    }

    /// Parent directory, or `""` for a top-level file.
    #[must_use]
    pub fn dir(&self) -> &Utf8Path {
        self.0.parent().unwrap_or_else(|| Utf8Path::new(""))
    }
}

impl std::fmt::Display for SourceRel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An output-relative path. Ends in `.html` for a page.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetPath(Utf8PathBuf);

impl TargetPath {
    /// Build a `TargetPath` from any path-like value.
    #[must_use]
    pub fn new(path: impl Into<Utf8PathBuf>) -> Self {
        Self(path.into())
    }

    /// Borrow the path.
    #[must_use]
    pub fn as_path(&self) -> &Utf8Path {
        &self.0
    }

    /// Borrow the path as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Lowercased form, used as a map key. Case-insensitive matching everywhere.
    #[must_use]
    pub fn folded(&self) -> String {
        self.0.as_str().to_lowercase()
    }

    /// Parent directory, or `""` for a top-level file.
    #[must_use]
    pub fn dir(&self) -> &Utf8Path {
        self.0.parent().unwrap_or_else(|| Utf8Path::new(""))
    }
}

impl std::fmt::Display for TargetPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One rendered document.
#[derive(Clone, Debug)]
pub struct Page {
    /// The Markdown source this page was rendered from.
    pub source: SourceRel,
    /// Where this page is written in the output tree.
    pub target: TargetPath,
    /// The page's title.
    pub title: String,
    /// True for exactly one page: the `--root` file, emitted as `index.html`.
    pub is_root: bool,
}

/// What a `DIR`-relative source path resolves to.
#[derive(Clone, Debug)]
pub enum SourceKind {
    /// A Markdown source that became a page; the index into `Site::pages`.
    Page(usize),
    /// A non-Markdown file eligible to be copied if something references it.
    Asset(SourceRel),
}

/// The complete plan: every page, the lookup map links resolve against, and the nav tree.
#[derive(Clone, Debug, Default)]
pub struct Site {
    /// Every page in the site, in discovery order.
    pub pages: Vec<Page>,
    /// Keyed by folded `DIR`-relative source path. Contains every page AND every
    /// non-Markdown file discovered under `DIR`.
    pub by_source: std::collections::HashMap<String, SourceKind>,
    /// The sidebar navigation tree.
    pub nav: Vec<NavNode>,
    /// With `--todo`, the index into `pages` of the root-level TODO page that
    /// collected tasks are appended to (R12.3, R12.4); `None` without it.
    pub todo: Option<usize>,
}

impl Site {
    /// Look up a folded `DIR`-relative source path.
    #[must_use]
    pub fn lookup(&self, folded: &str) -> Option<&SourceKind> {
        self.by_source.get(folded)
    }

    /// Fetch a page by its index into `pages`.
    #[must_use]
    pub fn page(&self, index: usize) -> &Page {
        &self.pages[index]
    }

    /// The root page (`is_root == true`).
    #[must_use]
    pub fn root_page(&self) -> &Page {
        self.pages
            .iter()
            .find(|page| page.is_root)
            .expect("site must have a root page")
    }
}

/// A sidebar entry.
#[derive(Clone, Debug)]
pub enum NavNode {
    /// A leaf: a page with no children.
    Page {
        /// The page's title.
        title: String,
        /// Where the page is written in the output tree.
        target: TargetPath,
    },
    /// A directory. `link` is `Some` when the directory holds a page whose stem
    /// casefolds to the directory name (R4.2); otherwise it is a plain label (R4.3).
    Group {
        /// The directory's display label.
        label: String,
        /// The directory's own page, if it has one.
        link: Option<TargetPath>,
        /// The directory's children.
        children: Vec<NavNode>,
    },
}

/// One heading, for the on-this-page outline (R8.3).
#[derive(Clone, Debug)]
pub struct OutlineEntry {
    /// 1..=6
    pub level: u8,
    /// The heading's anchor identifier, without the leading `#`.
    pub id: String,
    /// Rendered inline HTML of the heading text (already escaped where needed).
    pub text: String,
}

/// An ordered warning sink. Nothing in it changes the exit code (R10.5).
/// Deliberately not thread-shared: each parallel task builds its own and the
/// sequential merge appends them in page order, so stderr is deterministic (R11.3).
#[derive(Clone, Debug, Default)]
pub struct Warnings {
    items: Vec<String>,
}

impl Warnings {
    /// An empty warning sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a warning message.
    pub fn warn(&mut self, message: impl Into<String>) {
        self.items.push(message.into());
    }

    /// Append another sink's items, preserving order.
    pub fn absorb(&mut self, other: Warnings) {
        self.items.extend(other.items);
    }

    /// True when no warnings have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The number of recorded warnings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Iterate the recorded warnings in order.
    pub fn iter(&self) -> std::slice::Iter<'_, String> {
        self.items.iter()
    }

    /// Write every warning to stderr, in order, one per line, each prefixed
    /// `ohimark: warning: `.
    pub fn emit(&self) {
        for item in &self.items {
            eprintln!("ohimark: warning: {item}");
        }
    }
}

/// Fatal errors. Every variant names the stage that failed (R10.4).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Configuration failed to validate.
    #[error("configuration: {0}")]
    Config(String),
    /// Discovering source files failed.
    #[error("discovery: {0}")]
    Discovery(String),
    /// Planning the site failed.
    #[error("planning: {0}")]
    Plan(String),
    /// Rendering a page failed.
    #[error("rendering: {0}")]
    Render(String),
    /// Writing output failed.
    #[error("output: {0}")]
    Emit(String),
    /// An I/O error occurred during a named stage.
    #[error("{stage}: {source}")]
    Io {
        /// The stage that failed.
        stage: &'static str,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

/// The result type used throughout `ohimark`.
pub type Result<T> = std::result::Result<T, Error>;

/// Escape `&`, `<`, `>`, `"` and `'`. The one escaping rule shared by `render`
/// (heading anchors, code fences) and `theme` (titles, nav labels): every
/// character but these five passes through unchanged, so an escaped string's
/// own Markdown metacharacters still appear literally (R3.4).
#[must_use]
pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}
