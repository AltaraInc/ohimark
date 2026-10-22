//! Discovery, root resolution, target mapping, titles and the navigation
//! tree (R1-R4). Discovery and title extraction are the only I/O `ohimark`
//! performs before the render pass.

use std::collections::HashMap;
use std::ffi::OsStr;

use camino::{Utf8Path, Utf8PathBuf};
use ignore::{WalkBuilder, overrides::OverrideBuilder};
use rayon::prelude::*;

use crate::cli::Config;
use crate::links;
use crate::model::{
    Error, NavNode, Page, PageTimes, Result, Site, SourceKind, SourceRel, TargetPath, Warnings,
};
use crate::todo;

/// Everything read from disk, kept for the render pass.
pub struct Planned {
    /// The fully mapped site: pages, the source lookup, and the nav tree.
    pub site: Site,
    /// Markdown source text, one entry per `site.pages`, in the same order.
    pub sources: Vec<String>,
    /// Non-fatal findings collected while planning (R1.7, unreadable files).
    pub warnings: Warnings,
    /// True when `site.todo` names a page planned here with no file behind it,
    /// rather than a `TODO.md` found in `DIR` (R12.3, R12.4).
    pub todo_synthetic: bool,
}

/// Discover, read, title, map and build the navigation tree.
pub fn plan(config: &Config) -> Result<Planned> {
    let mut warnings = Warnings::new();
    let discovered = discover(config, &mut warnings)?;

    // R2.1 / R2.2: the root file must exist among the discovered sources.
    let root_folded = links::fold(config.root.as_str());
    let root_index = discovered
        .markdown
        .iter()
        .position(|source| links::fold(source.as_str()) == root_folded)
        .ok_or_else(|| {
            Error::Plan(format!(
                "root file `{}` was not found (missing, ignored, or excluded)",
                config.root
            ))
        })?;

    // Step 4 (R3, R11.3): read every source and extract its title, in
    // parallel; `par_iter().map().collect()` preserves input order.
    let read_results: Vec<ReadOutcome> = discovered
        .markdown
        .par_iter()
        .map(|source| read_and_title(&config.dir, source))
        .collect();

    let mut pages: Vec<Page> = Vec::with_capacity(discovered.markdown.len());
    let mut sources: Vec<String> = Vec::with_capacity(discovered.markdown.len());

    for (index, (source, outcome)) in discovered.markdown.iter().zip(read_results).enumerate() {
        let is_root = index == root_index;
        match outcome.text {
            Some(text) => {
                warnings.absorb(outcome.warnings);
                let target = if is_root {
                    TargetPath::new("index.html")
                } else {
                    retarget(source)
                };
                debug_assert!(
                    target.as_path().is_relative()
                        && target
                            .as_path()
                            .components()
                            .all(|component| component != camino::Utf8Component::ParentDir),
                    "target path must be output-relative and free of parent-directory components (R2.6)"
                );
                pages.push(Page {
                    source: source.clone(),
                    target,
                    title: outcome.title,
                    times: outcome.times,
                    is_root,
                });
                sources.push(text);
            }
            None => {
                if is_root {
                    return Err(Error::Plan(format!(
                        "root file `{source}` could not be used: {}",
                        outcome
                            .warnings
                            .iter()
                            .next()
                            .cloned()
                            .unwrap_or_else(|| "it could not be read".to_string())
                    )));
                }
                warnings.absorb(outcome.warnings);
            }
        }
    }

    // R12.3 / R12.4: with `--todo`, the root-level TODO page collects every
    // open task. A root TODO file that is absent, excluded, ignored or
    // unreadable is not a page, so a synthetic one stands in for it - but only
    // when some page has an open task to collect; with none, no TODO page is
    // planned at all. Planned before the lookup map and the nav are built, so
    // a synthetic page flows through both - and the collision check - like any
    // other. It has no file behind it, which is safe: nothing after planning
    // reads a page's source from disk.
    let mut todo_synthetic = false;
    let todo = if config.todo {
        match pages.iter().position(is_root_todo) {
            Some(index) => Some(index),
            // R12.3: with no open task to collect, nothing is generated, so
            // nothing is added to the sidebar and no file can be overwritten.
            None if !sources.iter().any(|source| todo::has_open_task(source)) => None,
            None => {
                let target = TargetPath::new(TODO_TARGET);
                // R12.3: a file already at that path would be overwritten in the
                // output; refuse, naming it, as R2.5 does for two pages.
                if let Some(html_page) = config
                    .html_pages
                    .iter()
                    .find(|page| output_paths_conflict(page.target.as_path(), target.as_path()))
                {
                    return Err(Error::Plan(format!(
                        "--endpoint `{}` produces `{}`, which conflicts with --todo output `{target}`",
                        html_page.source, html_page.target
                    )));
                }
                if let Some(asset) = discovered
                    .assets
                    .iter()
                    .find(|asset| asset.folded() == target.folded())
                {
                    return Err(Error::Plan(format!(
                        "`--todo` generates `{target}`, but `{asset}` already produces that target"
                    )));
                }
                todo_synthetic = true;
                pages.push(Page {
                    source: SourceRel::new(TODO_SOURCE),
                    target,
                    title: "TODO".to_string(),
                    times: PageTimes::default(),
                    is_root: false,
                });
                sources.push("# TODO\n".to_string());
                Some(pages.len() - 1)
            }
        }
    } else {
        None
    };

    // R2.5: two sources whose folded targets collide are a hard error, named
    // deterministically in page order.
    detect_target_collisions(&pages)?;

    let mut by_source: HashMap<String, SourceKind> = HashMap::new();
    for (index, page) in pages.iter().enumerate() {
        by_source.insert(page.source.folded(), SourceKind::Page(index));
    }
    for asset in &discovered.assets {
        by_source.insert(asset.folded(), SourceKind::Asset(asset.clone()));
    }

    let mut nav = build_nav(&pages);
    debug_assert_eq!(
        count_reachable_pages(&nav),
        pages.len(),
        "every page must be reachable from the sidebar (R4.6)"
    );
    nav.extend(config.html_pages.iter().map(|page| NavNode::Page {
        title: page.title.clone(),
        target: page.target.clone(),
    }));

    Ok(Planned {
        site: Site {
            pages,
            by_source,
            nav,
            todo,
        },
        sources,
        warnings,
        todo_synthetic,
    })
}

/// File outputs conflict when either would have to be the other's directory.
pub(crate) fn output_paths_conflict(left: &Utf8Path, right: &Utf8Path) -> bool {
    let left = links::fold(left.as_str());
    let right = links::fold(right.as_str());
    let left = Utf8Path::new(&left);
    let right = Utf8Path::new(&right);
    left.starts_with(right) || right.starts_with(left)
}

/// The source path a synthetic TODO page is planned under (R12.3).
const TODO_SOURCE: &str = "TODO.md";

/// The target a synthetic TODO page is written to (R12.3).
const TODO_TARGET: &str = "TODO.html";

/// R12.4: a page is the root-level TODO page when its source folds to
/// `todo.md` or `todo.markdown` at the top of `DIR`. Matched on the source, not
/// the target, so a `--root TODO.md` emitted as `index.html` still counts.
fn is_root_todo(page: &Page) -> bool {
    matches!(page.source.folded().as_str(), "todo.md" | "todo.markdown")
}

/// R2.5: reject two sources whose targets fold to the same value, in
/// deterministic (sorted page) order.
fn detect_target_collisions(pages: &[Page]) -> Result<()> {
    let mut seen: HashMap<String, &SourceRel> = HashMap::new();
    for page in pages {
        let folded = page.target.folded();
        if let Some(existing) = seen.get(&folded) {
            return Err(Error::Plan(format!(
                "`{existing}` and `{}` both produce the target `{}`",
                page.source, page.target
            )));
        }
        seen.insert(folded, &page.source);
    }
    Ok(())
}

/// R2.4: replace the source's extension with `.html`. No other renaming.
fn retarget(source: &SourceRel) -> TargetPath {
    TargetPath::new(source.as_path().with_extension("html"))
}

// ---------------------------------------------------------------------------
// Discovery (R1)
// ---------------------------------------------------------------------------

/// The discovered source set, each list sorted by folded `DIR`-relative path
/// ascending (R1.6).
struct Discovered {
    markdown: Vec<SourceRel>,
    assets: Vec<SourceRel>,
}

/// Walk `config.dir`, honouring ignore files and `--exclude`, and partition
/// the result into Markdown sources and asset candidates.
fn discover(config: &Config, warnings: &mut Warnings) -> Result<Discovered> {
    let overrides = build_overrides(config)?;

    let dir_for_filter = config.dir.clone();
    let skip_rel = config.out_inside_dir.clone();

    let mut builder = WalkBuilder::new(config.dir.as_std_path());
    builder
        .hidden(false) // R1.5: hidden files are eligible.
        .parents(false)
        .ignore(true)
        .git_ignore(true) // R1.2: honoured even without a `.git` directory,
        .git_exclude(true) // because of `require_git(false)` below.
        .require_git(false)
        .follow_links(false) // R1.8: symlinks are never followed.
        .overrides(overrides)
        .filter_entry(move |entry| {
            // R1.3: `.git` is always skipped, at any depth, regardless of
            // ignore rules.
            let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());
            if is_dir && entry.file_name() == OsStr::new(".git") {
                return false;
            }
            // R1.9: the output directory, when inside `DIR`, is never
            // discovered as input.
            if let Some(skip) = &skip_rel
                && let Ok(relative) = entry.path().strip_prefix(dir_for_filter.as_std_path())
                && let Some(relative) = Utf8Path::from_path(relative)
                && relative == skip.as_path()
            {
                return false;
            }
            true
        });

    let mut markdown = Vec::new();
    let mut assets = Vec::new();
    let mut deepest: Option<(usize, Utf8PathBuf)> = None;

    for entry in builder.build() {
        let entry = entry.map_err(|err| Error::Discovery(err.to_string()))?;

        if entry.depth() == 0 {
            continue; // DIR itself.
        }

        let Ok(relative) = entry.path().strip_prefix(config.dir.as_std_path()) else {
            continue;
        };
        let Some(relative) = Utf8Path::from_path(relative) else {
            warnings.warn(format!(
                "`{}` is not valid UTF-8 and was skipped",
                entry.path().display()
            ));
            continue;
        };

        // R1.7: track the deepest descent, breaking ties on folded path so
        // the single warning is deterministic regardless of walk order.
        let depth = entry.depth();
        if depth > 7 {
            let replace = match &deepest {
                None => true,
                Some((seen_depth, seen_path)) => {
                    depth > *seen_depth
                        || (depth == *seen_depth
                            && links::fold(relative.as_str()) < links::fold(seen_path.as_str()))
                }
            };
            if replace {
                deepest = Some((depth, relative.to_path_buf()));
            }
        }

        // R1.8: follow_links(false) prevents traversal, while this also
        // excludes symlink files and special entries from later reads.
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }

        let source = SourceRel::new(relative.to_path_buf());
        if is_markdown(relative) {
            markdown.push(source);
        } else {
            assets.push(source);
        }
    }

    if let Some((_, path)) = deepest {
        warnings.warn(format!(
            "`{path}` descends more than seven directory levels below DIR; consider adding an ignore rule"
        ));
    }

    markdown.sort_by_cached_key(SourceRel::folded);
    assets.sort_by_cached_key(SourceRel::folded);

    Ok(Discovered { markdown, assets })
}

/// R1.1: extension casefolds to `md` or `markdown`.
fn is_markdown(path: &Utf8Path) -> bool {
    matches!(
        path.extension().map(str::to_lowercase).as_deref(),
        Some("md") | Some("markdown")
    )
}

/// R1.4: build the `--exclude` overrides. For each glob `g`, both `g` and
/// everything under a directory matching `g` are excluded.
fn build_overrides(config: &Config) -> Result<ignore::overrides::Override> {
    let mut builder = OverrideBuilder::new(config.dir.as_std_path());
    for glob in &config.excludes {
        builder.add(&format!("!{glob}")).map_err(|err| {
            Error::Config(format!("--exclude `{glob}` is not a valid glob: {err}"))
        })?;
        builder.add(&format!("!{glob}/**")).map_err(|err| {
            Error::Config(format!("--exclude `{glob}` is not a valid glob: {err}"))
        })?;
    }
    builder
        .build()
        .map_err(|err| Error::Config(format!("--exclude globs could not be compiled: {err}")))
}

// ---------------------------------------------------------------------------
// Read and title (R3)
// ---------------------------------------------------------------------------

/// The result of reading and titling one source file.
struct ReadOutcome {
    /// `None` when the file could not be read as UTF-8 text.
    text: Option<String>,
    /// Meaningless when `text` is `None`.
    title: String,
    /// Unknown when source metadata is unavailable.
    times: PageTimes,
    /// This task's own findings; merged by the caller after the parallel
    /// phase, in page order, so stderr stays deterministic (R11.3).
    warnings: Warnings,
}

/// Read one Markdown source and extract its title. Pure with respect to
/// everything but the filesystem, so it is safe to call from any thread.
fn read_and_title(dir: &Utf8Path, source: &SourceRel) -> ReadOutcome {
    let mut warnings = Warnings::new();
    let full_path = dir.join(source.as_path());

    match read_utf8(&full_path) {
        Ok(text) => {
            let title = extract_title(&text, source);
            let times = std::fs::metadata(&full_path)
                .map(|metadata| PageTimes {
                    created: metadata.created().ok(),
                    modified: metadata.modified().ok(),
                })
                .unwrap_or_default();
            ReadOutcome {
                text: Some(text),
                title,
                times,
                warnings,
            }
        }
        Err(reason) => {
            warnings.warn(format!("`{source}` {reason}"));
            ReadOutcome {
                text: None,
                title: String::new(),
                times: PageTimes::default(),
                warnings,
            }
        }
    }
}

/// Read `path` and decode it as UTF-8, or the reason that failed.
fn read_utf8(path: &Utf8Path) -> std::result::Result<String, String> {
    let bytes =
        std::fs::read(path.as_std_path()).map_err(|err| format!("could not be read: {err}"))?;
    String::from_utf8(bytes).map_err(|_| "is not valid UTF-8 and was skipped".to_string())
}

/// R3.1-R3.3: the first line matching a leading `#` then one or more spaces,
/// trimmed; otherwise R3.2, the file stem. Line-based, not Markdown-aware, so
/// a `#` line inside a fenced code block still counts (R3.3). The text is
/// returned literally: no Markdown metacharacter is stripped (R3.4).
fn extract_title(text: &str, source: &SourceRel) -> String {
    for line in text.lines() {
        if let Some(heading) = leading_hash_heading(line) {
            return heading.to_string();
        }
    }
    stem_of(source).to_string()
}

/// Matches `<optional whitespace>#<one or more spaces><text>`, returning the
/// trimmed text. A second `#` right after the first (an H2+ heading) does not
/// match: R3.1 names a single leading `#`.
fn leading_hash_heading(line: &str) -> Option<&str> {
    let after_indent = line.trim_start();
    let after_hash = after_indent.strip_prefix('#')?;
    if !after_hash.starts_with(' ') {
        return None;
    }
    Some(after_hash.trim())
}

/// The file stem (R3.2), e.g. `guide` for `docs/guide.md`.
fn stem_of(source: &SourceRel) -> &str {
    source
        .as_path()
        .file_stem()
        .unwrap_or_else(|| source.as_str())
}

// ---------------------------------------------------------------------------
// Navigation tree (R4)
// ---------------------------------------------------------------------------

/// A page index paired with its remaining directory path components.
type NavEntries = Vec<(usize, Vec<String>)>;

/// A directory's display name, paired with the entries that lie below it.
type NavGroup = (String, NavEntries);

/// Keyed by folded directory name.
type NavChildren = HashMap<String, NavGroup>;

/// A `NavChildren` map's groups, in folded ascending order (R4.4).
type NavGroups = Vec<NavGroup>;

/// Build the sidebar tree from the final page list.
fn build_nav(pages: &[Page]) -> Vec<NavNode> {
    let mut root_index = None;
    let mut entries: NavEntries = Vec::new();

    for (index, page) in pages.iter().enumerate() {
        if page.is_root {
            // R4.5: the root page is pulled out entirely - it does not also
            // appear wherever its own directory would have placed it.
            root_index = Some(index);
        } else {
            entries.push((index, dir_components(page)));
        }
    }

    let (mut top_own, top_children) = partition_entries(entries);
    top_own.sort_by_key(|&index| links::fold(stem_of(&pages[index].source)));

    let mut nav = Vec::with_capacity(1 + top_own.len() + top_children.len());
    nav.extend(root_index.map(|index| nav_page(pages, index)));
    nav.extend(top_own.into_iter().map(|index| nav_page(pages, index)));
    for (display_name, group_entries) in sorted_groups(top_children) {
        nav.push(build_group(display_name, group_entries, pages));
    }

    nav
}

/// A sidebar leaf for `pages[index]`.
fn nav_page(pages: &[Page], index: usize) -> NavNode {
    NavNode::Page {
        title: pages[index].title.clone(),
        target: pages[index].target.clone(),
    }
}

/// The page's directory, split into displayed (non-folded) components.
fn dir_components(page: &Page) -> Vec<String> {
    page.source
        .dir()
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(String::from)
        .collect()
}

/// Split entries into those that belong to this directory (an empty
/// remaining path) and those that belong to a subdirectory bucket, keyed by
/// that subdirectory's folded first remaining component.
fn partition_entries(entries: NavEntries) -> (Vec<usize>, NavChildren) {
    let mut own = Vec::new();
    let mut children: NavChildren = HashMap::new();
    for (index, mut components) in entries {
        if components.is_empty() {
            own.push(index);
        } else {
            let first = components.remove(0);
            let folded = links::fold(&first);
            children
                .entry(folded)
                .or_insert_with(|| (first, Vec::new()))
                .1
                .push((index, components));
        }
    }
    (own, children)
}

/// Consume a `NavChildren` map into its (display name, entries) groups, in
/// folded ascending order (R4.4).
fn sorted_groups(mut children: NavChildren) -> NavGroups {
    let mut folded_names: Vec<String> = children.keys().cloned().collect();
    folded_names.sort();
    folded_names
        .into_iter()
        .map(|folded| children.remove(&folded).expect("key just listed"))
        .collect()
}

/// Build one directory's node, recursing into its subdirectories.
///
/// `entries` are the pages that live at or below this directory, each paired
/// with its remaining path components relative to this directory.
fn build_group(name: String, entries: Vec<(usize, Vec<String>)>, pages: &[Page]) -> NavNode {
    let (mut own, children_by_name) = partition_entries(entries);

    // R4.2: a directory's own page whose stem casefolds to its name takes
    // priority; otherwise its immediate README becomes the link. The selected
    // landing page is not also listed as a child.
    let name_folded = links::fold(&name);
    let link_position = own
        .iter()
        .position(|&index| links::fold(stem_of(&pages[index].source)) == name_folded)
        .or_else(|| {
            own.iter()
                .position(|&index| links::fold(stem_of(&pages[index].source)) == "readme")
        });
    let link = link_position.map(|position| pages[own.remove(position)].target.clone());

    own.sort_by_key(|&index| links::fold(stem_of(&pages[index].source)));

    let mut children: Vec<NavNode> = own
        .into_iter()
        .map(|index| nav_page(pages, index))
        .collect();
    for (display_name, sub_entries) in sorted_groups(children_by_name) {
        children.push(build_group(display_name, sub_entries, pages));
    }

    NavNode::Group {
        label: name,
        link,
        children,
    }
}

/// R4.6: count how many pages are reachable from the sidebar.
fn count_reachable_pages(nav: &[NavNode]) -> usize {
    nav.iter()
        .map(|node| match node {
            NavNode::Page { .. } => 1,
            NavNode::Group { link, children, .. } => {
                usize::from(link.is_some()) + count_reachable_pages(children)
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(source: &str, title: &str, is_root: bool) -> Page {
        let source = SourceRel::new(source);
        let target = if is_root {
            TargetPath::new("index.html")
        } else {
            retarget(&source)
        };
        Page {
            source,
            target,
            title: title.to_string(),
            times: PageTimes::default(),
            is_root,
        }
    }

    #[test]
    fn read_and_title_captures_source_metadata_with_controlled_modified_time() {
        let dir = tempfile::tempdir().expect("create source directory");
        let dir = Utf8Path::from_path(dir.path()).expect("UTF-8 fixture path");
        let path = dir.join("note.md");
        std::fs::write(&path, "# Note\n").expect("write source");
        let modified = std::time::UNIX_EPOCH + std::time::Duration::from_millis(1_600_000_000_123);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .expect("open source")
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .expect("set source modification time");
        let metadata = std::fs::metadata(&path).expect("read source metadata");

        let outcome = read_and_title(dir, &SourceRel::new("note.md"));
        assert_eq!(outcome.text.as_deref(), Some("# Note\n"));
        assert_eq!(outcome.title, "Note");
        assert_eq!(outcome.times.created, metadata.created().ok());
        assert_eq!(outcome.times.modified, Some(modified));
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn planning_preserves_real_todo_times_and_leaves_synthetic_times_unknown() {
        let dir = tempfile::tempdir().expect("create source directory");
        let dir = Utf8Path::from_path(dir.path()).expect("UTF-8 fixture path");
        std::fs::write(dir.join("README.md"), "# Home\n\n- [ ] Task\n").expect("write source");
        let config = Config {
            dir: dir.to_owned(),
            out: dir.join("output"),
            root: "README.md".into(),
            max_group_size: crate::cli::DEFAULT_MAX_GROUP_SIZE,
            theme: None,
            stylesheet: None,
            html_pages: Vec::new(),
            excludes: Vec::new(),
            out_inside_dir: Some("output".into()),
            todo: true,
        };

        let synthetic = plan(&config).expect("plan synthetic TODO");
        assert!(synthetic.todo_synthetic);
        let todo = synthetic.site.page(synthetic.site.todo.unwrap());
        assert!(todo.times.created.is_none());
        assert!(todo.times.modified.is_none());

        let path = dir.join("TODO.md");
        std::fs::write(&path, "# Roadmap\n").expect("write TODO source");
        let metadata = std::fs::metadata(&path).expect("read TODO metadata");
        let real = plan(&config).expect("plan real TODO");
        assert!(!real.todo_synthetic);
        let todo = real.site.page(real.site.todo.unwrap());
        assert_eq!(todo.title, "Roadmap");
        assert_eq!(todo.times.created, metadata.created().ok());
        assert_eq!(todo.times.modified, metadata.modified().ok());
    }

    #[test]
    fn r3_1_leading_hash_with_space_is_the_title() {
        let text = "#   Spaced Title   \nbody";
        let title = extract_title(text, &SourceRel::new("a.md"));
        assert_eq!(title, "Spaced Title");
    }

    #[test]
    fn r3_2_no_heading_falls_back_to_stem() {
        let text = "just a paragraph, no heading here";
        let title = extract_title(text, &SourceRel::new("docs/guide.md"));
        assert_eq!(title, "guide");
    }

    #[test]
    fn r3_3_heading_inside_a_fence_still_counts() {
        let text = "```\n# Inside Fence\n```\n";
        let title = extract_title(text, &SourceRel::new("a.md"));
        assert_eq!(title, "Inside Fence");
    }

    #[test]
    fn r3_second_level_heading_does_not_count() {
        let text = "## Not This\n# But This";
        let title = extract_title(text, &SourceRel::new("a.md"));
        assert_eq!(title, "But This");
    }

    #[test]
    fn r4_2_directory_page_matching_dir_name_becomes_the_link() {
        let pages = vec![
            page("docs/docs.md", "Docs", false),
            page("docs/guide.md", "Guide", false),
        ];
        let nav = build_nav(&pages);
        assert_eq!(nav.len(), 1);
        match &nav[0] {
            NavNode::Group {
                label,
                link,
                children,
            } => {
                assert_eq!(label, "docs");
                assert_eq!(link.as_ref().unwrap().as_str(), "docs/docs.html");
                assert_eq!(children.len(), 1);
                match &children[0] {
                    NavNode::Page { title, .. } => assert_eq!(title, "Guide"),
                    _ => panic!("expected a page"),
                }
            }
            _ => panic!("expected a group"),
        }
    }

    #[test]
    fn r4_3_directory_without_matching_page_has_no_link() {
        let pages = vec![page("docs/guide.md", "Guide", false)];
        let nav = build_nav(&pages);
        match &nav[0] {
            NavNode::Group { link, .. } => assert!(link.is_none()),
            _ => panic!("expected a group"),
        }
    }

    #[test]
    fn r4_4_own_pages_then_subdirectories_casefolded_ascending() {
        let pages = vec![
            page("docs/zebra.md", "zebra", false),
            page("docs/Alpha.md", "Alpha", false),
            page("docs/guides/x.md", "x", false),
        ];
        let nav = build_nav(&pages);
        match &nav[0] {
            NavNode::Group { children, .. } => {
                let names: Vec<String> = children
                    .iter()
                    .map(|node| match node {
                        NavNode::Page { title, .. } => title.clone(),
                        NavNode::Group { label, .. } => label.clone(),
                    })
                    .collect();
                assert_eq!(names, vec!["Alpha", "zebra", "guides"]);
            }
            _ => panic!("expected a group"),
        }
    }

    #[test]
    fn r4_5_root_page_is_first_and_not_duplicated() {
        let pages = vec![
            page("docs/home.md", "Home", true),
            page("docs/guide.md", "Guide", false),
            page("a.md", "A", false),
        ];
        let nav = build_nav(&pages);
        match &nav[0] {
            NavNode::Page { title, .. } => assert_eq!(title, "Home"),
            _ => panic!("expected the root page first"),
        }
        // The root's own directory keeps only its remaining page.
        let docs_group = nav
            .iter()
            .find_map(|node| match node {
                NavNode::Group {
                    label, children, ..
                } if label == "docs" => Some(children),
                _ => None,
            })
            .expect("docs group present");
        assert_eq!(docs_group.len(), 1);
    }

    #[test]
    fn r4_1_intermediate_directories_appear_without_their_own_page() {
        let pages = vec![page("a/b/c.md", "c", false)];
        let nav = build_nav(&pages);
        match &nav[0] {
            NavNode::Group {
                label,
                link,
                children,
            } => {
                assert_eq!(label, "a");
                assert!(link.is_none());
                match &children[0] {
                    NavNode::Group { label, .. } => assert_eq!(label, "b"),
                    _ => panic!("expected nested group `b`"),
                }
            }
            _ => panic!("expected a group"),
        }
    }

    #[test]
    fn r4_6_every_page_is_reachable() {
        let pages = vec![
            page("README.md", "Home", true),
            page("a.md", "A", false),
            page("docs/docs.md", "Docs", false),
            page("docs/guide.md", "Guide", false),
        ];
        let nav = build_nav(&pages);
        assert_eq!(count_reachable_pages(&nav), pages.len());
    }
}
