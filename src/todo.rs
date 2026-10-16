//! Task collection for `--todo` (R12): lift every open task-list item out of the site's
//! pages and append them to the root-level TODO page, under a linked heading per file.
//!
//! Each page is re-parsed with the same options `render` uses. An unchecked task item is
//! lifted whole, minus any checked task items nested in it; an unchecked item nested in a
//! checked one, at any depth, is lifted on its own (R12.2). Links and images inside a
//! lifted item are resolved from the file they were written in but emitted relative to the
//! TODO page, so they keep pointing at the same thing (R12.6). The lifted items become
//! raw-HTML lists under generated headings in an appendix, which is rendered through
//! `render::render_part` after the TODO page's own content, as a separate part sharing its
//! anchors, so the headings get anchors, outline entries and link resolution like any
//! other. Nothing here touches the filesystem (R12.7).

use camino::Utf8Path;
use percent_encoding::{AsciiSet, CONTROLS};
use pulldown_cmark::{CowStr, Event, Parser, Tag, TagEnd};
use rayon::prelude::*;

use crate::links::{self, Resolved};
use crate::model::{Page, Site, SourceRel, TargetPath, escape_html};
use crate::render::{self, Diagrams, PageIds, Rendered};
use crate::task_tags::{self, Annotation};
use crate::theme::page_time_attrs;

/// The `<li>` a task item opens with, exactly as `render` emits it (R7.8).
#[cfg(test)]
const TASK_ITEM_OPEN: &str = r#"<li class="ohm-task">"#;

/// Bytes percent-encoded in a generated file link, so `links::resolve` decodes the
/// destination back to the exact source path: `%` would otherwise be decoded, `#` and `?`
/// would split off as a fragment or query, `:` could make the path look like a URL scheme,
/// `&` could start a character reference, and `<`, `>` and `\` would end or escape the
/// `<…>` destination.
const DEST_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b'%')
    .add(b'#')
    .add(b'?')
    .add(b':')
    .add(b'&')
    .add(b'<')
    .add(b'>')
    .add(b'\\');

/// The open tasks lifted out of one page.
#[derive(Clone, Debug, Default)]
pub struct Collected {
    /// One `<li>…</li>` fragment per lifted item, in document order, each on one line.
    pub html_items: Vec<String>,
    /// Non-Markdown files the lifted items reference, so `emit` copies them (R12.6).
    pub assets: Vec<SourceRel>,
}

/// Render the TODO page `site.pages[todo_index]` with every other page's open tasks
/// appended (R12.3-R12.5). `appended_heading` is true when the page is a real `TODO.md`,
/// whose own content is followed by a `Collected Tasks` section (R12.4).
///
/// The page's own Markdown and the appendix are parsed and rendered as two parts, so a
/// fence or HTML block left open at the end of `TODO.md` cannot swallow the appendix;
/// the parts share one set of page ids, so anchors stay unique across both (R6.8). An
/// empty appendix is not rendered at all, so with no open task collected the page comes
/// out as it does without `--todo` (R12.4, R12.5).
///
/// Collection runs in parallel and preserves page order in its result (R11.3). The
/// appendix keeps that order and gives each group the same safe timestamp attributes as
/// the sidebar when available. Its warnings are discarded: each page's own render
/// already reported every unresolved reference.
#[must_use]
pub fn render_page(
    site: &Site,
    sources: &[String],
    todo_index: usize,
    appended_heading: bool,
    diagrams: &Diagrams,
) -> Rendered {
    let todo_page = site.page(todo_index);
    let collected: Vec<Collected> = site
        .pages
        .par_iter()
        .zip(sources.par_iter())
        .enumerate()
        .map(|(index, (page, source))| {
            if index == todo_index {
                // R12.4: the TODO page's own tasks already show in its own content.
                Collected::default()
            } else {
                collect(page, source, site, &todo_page.target)
            }
        })
        .collect();

    let mut ids = PageIds::default();
    let mut rendered =
        render::render_part(todo_page, &sources[todo_index], site, diagrams, &mut ids);
    let appendix = appendix(site, &collected, appended_heading);
    if !appendix.is_empty() {
        let appended = render::render_part(todo_page, &appendix, site, diagrams, &mut ids);
        rendered.html.push_str(&appended.html);
        rendered.outline.extend(appended.outline);
        rendered.assets.extend(appended.assets);
        rendered.warnings.absorb(appended.warnings);
    }
    rendered
        .assets
        .extend(collected.into_iter().flat_map(|page| page.assets));
    rendered
}

/// Lift every open task item out of one page's Markdown (R12.2), with its links and
/// images re-based to `todo_target` (R12.6).
#[must_use]
pub fn collect(page: &Page, markdown: &str, site: &Site, todo_target: &TargetPath) -> Collected {
    let (events, annotations) = task_tags::parse(markdown, render::parser_options());
    let cx = Context {
        page,
        site,
        todo_dir: todo_target.dir(),
    };
    let mut collected = Collected::default();
    lift_open_items(&events, &annotations, &cx, &mut collected);
    collected
}

/// True when the page has an open task item at any depth, that is, when `collect` would
/// lift something out of it (R12.2).
#[must_use]
pub fn has_open_task(markdown: &str) -> bool {
    let events: Vec<Event> = Parser::new_ext(markdown, render::parser_options()).collect();
    (0..events.len()).any(|index| task_state(&events, index) == Some(false))
}

/// What every lifted item is resolved against: the page it was written in, and the TODO
/// page's directory it is emitted relative to.
struct Context<'s> {
    page: &'s Page,
    site: &'s Site,
    todo_dir: &'s Utf8Path,
}

/// Lift every open task item in `events` that is not inside another lifted one, in
/// source order.
fn lift_open_items(
    events: &[Event],
    annotations: &[Annotation],
    cx: &Context,
    collected: &mut Collected,
) {
    let mut index = 0;
    while index < events.len() {
        if task_state(events, index) == Some(false) {
            // Lifted whole, so an open item nested in this one is not lifted again.
            let end = matching_end(events, index);
            lift_item(
                &events[index..=end],
                &annotations[index..=end],
                cx,
                collected,
            );
            index = end + 1;
        } else {
            // Anything else, a checked item included, is stepped into rather than
            // over, so an open item nested inside it is still found.
            index += 1;
        }
    }
}

/// Assemble the Markdown appended to the TODO page: for each file with open tasks, in
/// page order (R1.6), a linked file heading, then that file's open tasks as one raw-HTML
/// list (R12.5). Each group receives the same safe created/updated timestamp attributes
/// as the sidebar when available. Empty when no file has an open task, the `Collected
/// Tasks` heading included, so nothing is rendered for them (R12.5). `collected` is
/// parallel to `site.pages`.
fn appendix(site: &Site, collected: &[Collected], appended_heading: bool) -> String {
    if collected.iter().all(|page| page.html_items.is_empty()) {
        return String::new();
    }

    let mut out = String::new();
    if appended_heading {
        out.push_str("<section data-ohm-todo-collected>\n\n# Collected Tasks\n\n");
    }

    for (index, page_collected) in collected.iter().enumerate() {
        if page_collected.html_items.is_empty() {
            continue;
        }
        let page = site.page(index);
        // The destination is the source path, so the ordinary link resolution of the
        // re-render turns it into the right href (R6.1).
        let dest = percent_encoding::utf8_percent_encode(page.source.as_str(), DEST_ENCODE_SET);
        let time_attrs = page_time_attrs(Some(&page.times));
        out.push_str(&format!(
            "<section data-ohm-todo-group{time_attrs}>\n\n## [{}](<{dest}>)\n\n<ul>{}</ul>\n\n</section>\n\n",
            escape_markdown(&page.title),
            page_collected.html_items.concat()
        ));
    }
    if appended_heading {
        out.push_str("</section>\n\n");
    }
    out
}

/// Backslash-escape every ASCII punctuation character, so a title appears literally in a
/// generated heading (R3.4).
fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// `Some(checked)` when `events[index]` opens a task-list item, `None` otherwise. A loose
/// item opens a paragraph before its marker.
fn task_state(events: &[Event], index: usize) -> Option<bool> {
    if !matches!(events.get(index), Some(Event::Start(Tag::Item))) {
        return None;
    }
    let marker = match events.get(index + 1) {
        Some(Event::Start(Tag::Paragraph)) => events.get(index + 2),
        other => other,
    };
    match marker {
        Some(Event::TaskListMarker(checked)) => Some(*checked),
        _ => None,
    }
}

/// The index of the `End` event matching the `Start` at `start`. The parser always
/// balances its tags, so counting every start and end is enough.
fn matching_end(events: &[Event], start: usize) -> usize {
    let mut depth = 0usize;
    for (offset, event) in events[start..].iter().enumerate() {
        match event {
            Event::Start(_) => depth += 1,
            Event::End(_) => {
                depth -= 1;
                if depth == 0 {
                    return start + offset;
                }
            }
            _ => {}
        }
    }
    events.len() - 1
}

/// True when every item of the list opened at `start` is a checked task item, so the
/// list would be left empty once they are dropped.
fn only_checked_items(events: &[Event], start: usize) -> bool {
    let end = matching_end(events, start);
    let mut index = start + 1;
    while index < end {
        if task_state(events, index) != Some(true) {
            return false;
        }
        index = matching_end(events, index) + 1;
    }
    true
}

/// Serialise one lifted item, `item` running from its `Start(Item)` to its `End(Item)`,
/// to a single line of HTML, then lift any open items under the checked items it drops.
fn lift_item(item: &[Event], annotations: &[Annotation], cx: &Context, collected: &mut Collected) {
    let mut out: Vec<Event> = Vec::with_capacity(item.len());
    // Subtrees dropped from this item, in source order. An open item inside one is still
    // open, so it is lifted on its own, right after this item (R12.2).
    let mut dropped: Vec<(&[Event], &[Annotation])> = Vec::new();
    let mut in_code = false;

    let mut index = 0;
    while index < item.len() {
        // A parent's badges can belong immediately before a child list that is
        // pruned below. Emit them before deciding whether to skip that subtree.
        annotations[index].emit_before(&mut out);
        match &item[index] {
            // R12.2: a nested checked item is dropped with everything under it, and a
            // nested list left with nothing in it goes too.
            Event::Start(Tag::Item) if task_state(item, index) == Some(true) => {
                let end = matching_end(item, index);
                dropped.push((&item[index..=end], &annotations[index..=end]));
                index = end + 1;
                continue;
            }
            Event::Start(Tag::List(_)) if only_checked_items(item, index) => {
                let end = matching_end(item, index);
                dropped.push((&item[index..=end], &annotations[index..=end]));
                index = end + 1;
                continue;
            }
            ev @ (Event::Start(Tag::Link { .. }) | Event::Start(Tag::Image { .. })) => {
                out.push(resolve_event(ev.clone(), cx, &mut collected.assets));
            }
            // The footnote's definition stays on its own page, so a reference here would
            // link to nothing; it is kept as the text it was written as.
            Event::FootnoteReference(name) => {
                out.push(Event::Text(CowStr::from(format!("[^{name}]"))));
            }
            ev @ Event::Start(Tag::CodeBlock(_)) => {
                in_code = true;
                out.push(ev.clone());
            }
            ev @ Event::End(TagEnd::CodeBlock) => {
                in_code = false;
                out.push(ev.clone());
            }
            // A code block's line breaks are significant, so they are kept as character
            // references the newline collapse below leaves alone.
            Event::Text(text) if in_code => {
                out.push(Event::Html(CowStr::from(
                    escape_html(text).replace('\n', "&#10;"),
                )));
            }
            other => out.push(annotations[index].decorate(other.clone())),
        }
        index += 1;
    }

    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, out.into_iter());
    // A blank line would end the raw-HTML block the fragment is appended in, so the
    // fragment is kept to one line.
    collected
        .html_items
        .push(html.trim_end().replace('\n', " "));

    for (subtree, subtree_annotations) in dropped {
        lift_open_items(subtree, subtree_annotations, cx, collected);
    }
}

/// Rewrite a link's or image's destination from the lifted item's own page to the TODO
/// page, the same rule `render` applies (R6.1-R6.9, R12.6). The page's own render has
/// already warned about anything unresolved; here it is only re-based:
///
/// - a pure fragment points into the lifted item's own page, not the TODO page;
/// - a missing target keeps the path the author wrote, re-based lexically from the
///   item's source directory, so it still points where it would have.
///
/// Every other unchanged form (external, root-absolute, ...) is left as written.
fn resolve_event<'a>(event: Event<'a>, cx: &Context, assets: &mut Vec<SourceRel>) -> Event<'a> {
    let source_dir = cx.page.source.dir();
    let mut resolve = |raw: CowStr<'a>| -> CowStr<'a> {
        match links::resolve(&raw, source_dir, cx.todo_dir, cx.site) {
            Resolved::Page(href) => CowStr::from(href),
            Resolved::Asset { href, source } => {
                assets.push(source);
                CowStr::from(href)
            }
            Resolved::Unchanged if raw.starts_with('#') => CowStr::from(format!(
                "{}{raw}",
                links::relative_to(cx.todo_dir, cx.page.target.as_path())
            )),
            Resolved::Unchanged => raw,
            Resolved::Missing => {
                let (path, query, fragment) = links::split_reference(&raw);
                match links::normalize(source_dir, path) {
                    Some(target) if !target.as_str().is_empty() => CowStr::from(format!(
                        "{}{query}{fragment}",
                        links::relative_to(cx.todo_dir, &target)
                    )),
                    // Escapes `DIR`, or is `DIR` itself: nothing to re-base it onto.
                    _ => raw,
                }
            }
        }
    };
    match event {
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: resolve(dest_url),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: resolve(dest_url),
            title,
            id,
        }),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PageTimes, SourceKind};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn page(source: &str) -> Page {
        let source = SourceRel::new(source);
        let target = TargetPath::new(source.as_path().with_extension("html"));
        Page {
            source,
            target,
            title: "Page".to_string(),
            times: Default::default(),
            is_root: false,
        }
    }

    fn collect_from(markdown: &str) -> Vec<String> {
        collect(
            &page("page.md"),
            markdown,
            &Site::default(),
            &TargetPath::new("TODO.html"),
        )
        .html_items
    }

    #[test]
    fn r12_2_only_unchecked_items_are_collected() {
        let items = collect_from("- [x] done\n- [ ] open\n- plain\n\n1. [ ] ordered\n");
        assert_eq!(
            items,
            vec![
                r#"<li class="ohm-task"><input disabled="" type="checkbox"/> open</li>"#,
                r#"<li class="ohm-task"><input disabled="" type="checkbox"/> ordered</li>"#,
            ]
        );
    }

    #[test]
    fn r12_2_loose_item_is_collected_through_its_paragraph() {
        let items = collect_from("- [x] done\n\n- [ ] open\n");
        assert_eq!(
            items,
            vec![r#"<li class="ohm-task"> <p><input disabled="" type="checkbox"/> open</p> </li>"#]
        );
        assert!(!items[0].contains('\n'));
    }

    #[test]
    fn r12_2_nested_items_stay_inside_their_collected_parent() {
        let md = "- [ ] parent\n  - [x] done child\n    - [ ] under done\n  - [ ] open child\n  - plain child\n";
        let items = collect_from(md);
        // The open child is lifted with its parent, not again on its own; the open item
        // under the dropped checked child is lifted on its own, after the parent.
        assert_eq!(items.len(), 2);
        let item = &items[0];
        assert!(item.contains("parent"));
        assert!(item.contains("open child"));
        assert!(item.contains("<li>plain child</li>"));
        assert!(!item.contains("done child"));
        assert!(!item.contains("under done"));
        assert_eq!(item.matches(TASK_ITEM_OPEN).count(), 2);
        assert!(items[1].contains("under done"));
        assert!(!items[1].contains("done child"));
    }

    #[test]
    fn r12_2_open_grandchild_under_a_dropped_checked_child_is_lifted() {
        let items = collect_from("- [ ] A\n  - [x] B\n    - [ ] C\n");
        assert_eq!(items.len(), 2);
        assert!(items[0].contains('A') && !items[0].contains('C'));
        assert!(items[1].contains('C') && !items[1].contains('B'));
    }

    #[test]
    fn r12_2_open_items_under_dropped_subtrees_keep_source_order() {
        let md = "- [ ] A\n  - [x] B\n    - [ ] C\n      - [x] D\n        - [ ] E\n  - [ ] F\n";
        let items = collect_from(md);
        assert_eq!(items.len(), 3);
        assert!(items[0].contains('A') && items[0].contains('F'));
        assert!(items[1].contains('C') && !items[1].contains('E'));
        assert!(items[2].contains('E'));
        for absent in ['B', 'D'] {
            assert!(items.iter().all(|item| !item.contains(absent)));
        }
    }

    #[test]
    fn r12_2_list_of_only_checked_children_is_dropped() {
        let items = collect_from("- [ ] parent\n  - [x] a\n  - [x] b\n");
        assert_eq!(items.len(), 1);
        assert!(!items[0].contains("<ul>"));
    }

    #[test]
    fn r12_2_open_child_of_checked_parent_is_collected_alone() {
        let md = "- [x] checked parent\n  - [ ] open child\n  - [x] done child\n";
        let items = collect_from(md);
        assert_eq!(items.len(), 1);
        assert!(items[0].contains("open child"));
        assert!(!items[0].contains("checked parent"));
        assert!(!items[0].contains("done child"));
    }

    #[test]
    fn r12_2_code_block_line_breaks_survive_the_collapse() {
        let items = collect_from("- [ ] run\n\n  ```sh\n  a <b>\n  c\n  ```\n");
        assert_eq!(items.len(), 1);
        assert!(items[0].contains("a &lt;b&gt;&#10;c"));
        assert!(!items[0].contains('\n'));
    }

    #[test]
    fn task_tags_match_ordinary_rendering_in_tight_and_loose_lists() {
        for markdown in [
            "- [ ] parent {Travel, Buy} *{work}*\n  - plain {literal}\n  - [ ] child {BUY}\n",
            "- [ ] parent {Travel}\n\n  second paragraph {Buy}\n\n  - plain {literal}\n  - [ ] child {work}\n",
        ] {
            let rendered = render::render_page(
                &page("page.md"),
                markdown,
                &Site::default(),
                &Diagrams::new(),
            );
            let items = collect_from(markdown);
            assert_eq!(items.len(), 1);
            // The normal page has only its filename and enclosing list in addition
            // to the same task subtree; collection merely collapses line breaks.
            let start = rendered.html.find("<li class=\"ohm-task\"").unwrap();
            let end = rendered.html.rfind("</li>").unwrap() + "</li>".len();
            assert_eq!(items[0], rendered.html[start..end].replace('\n', " "));
            assert!(items[0].contains("plain {literal}"));
            assert!(!items[0].contains("{Travel"));
        }
    }

    #[test]
    fn tag_annotations_follow_pruned_and_independently_lifted_subtrees() {
        let items = collect_from(
            "- [ ] parent {parent}\n  - [x] done {done}\n    - [ ] grandchild {grandchild}\n      - [x] done again {lost}\n        - [ ] deep {deep}\n  - [ ] sibling {sibling}\n",
        );
        assert_eq!(items.len(), 3);
        for (index, expected) in [
            (0, vec!["parent", "sibling"]),
            (1, vec!["grandchild"]),
            (2, vec!["deep"]),
        ] {
            for label in expected {
                assert!(
                    items[index].contains(&format!(r#"data-ohm-tags="[&quot;{label}&quot;]""#)),
                    "{}",
                    items[index]
                );
            }
            assert!(!items[index].contains("{done}"));
            assert!(!items[index].contains("{lost}"));
            assert!(!items[index].contains('\n'));
        }
        assert_eq!(items[0].matches("class=\"ohm-task-tags\"").count(), 2);
        assert_eq!(items[1].matches("class=\"ohm-task-tags\"").count(), 1);
        assert_eq!(items[2].matches("class=\"ohm-task-tags\"").count(), 1);
    }

    #[test]
    fn parent_badges_survive_removing_the_entire_nested_list() {
        let items = collect_from("- [ ] parent {a}\n  - [x] child {b}\n");
        assert_eq!(items.len(), 1);
        assert!(items[0].contains(r#"class="ohm-task-tag">a</span>"#));
        assert!(!items[0].contains("<ul>"));
        assert!(!items[0].contains("child"));
    }

    #[test]
    fn r12_6_fragment_links_point_at_the_items_own_page() {
        let collected = collect(
            &page("guides/a.md"),
            "- [ ] see [below](#notes)\n",
            &Site::default(),
            &TargetPath::new("TODO.html"),
        );
        assert!(collected.html_items[0].contains(r#"href="guides/a.html#notes""#));
    }

    #[test]
    fn r12_6_footnote_references_become_plain_text() {
        let items = collect_from("- [ ] see note[^n]\n\n[^n]: the note\n");
        assert_eq!(items.len(), 1);
        assert!(items[0].contains("note[^n]"), "{}", items[0]);
        assert!(!items[0].contains("<a"), "{}", items[0]);
        assert!(!items[0].contains("the note"));
    }

    #[test]
    fn r12_6_links_and_images_are_rebased_to_the_todo_page() {
        let pages = vec![page("guides/install.md"), page("guides/api/ref.md")];
        let mut site = Site {
            pages,
            ..Site::default()
        };
        site.by_source
            .insert("guides/install.md".to_string(), SourceKind::Page(0));
        site.by_source.insert(
            "guides/api/img/d.png".to_string(),
            SourceKind::Asset(SourceRel::new("guides/api/img/d.png")),
        );

        let collected = collect(
            &site.pages[1],
            "- [ ] see [Install](../install.md) ![D](img/d.png) [gone](gone.md?q=1#f) [out](../../../out.md) [web](https://example.com/x)\n",
            &site,
            &TargetPath::new("TODO.html"),
        );
        let item = &collected.html_items[0];
        assert!(item.contains(r#"href="guides/install.html""#));
        assert!(item.contains(r#"src="guides/api/img/d.png""#));
        // Missing: re-based lexically from the item's own directory, query and fragment
        // kept; one escaping `DIR` has nothing to re-base onto and is left as written.
        assert!(item.contains(r#"href="guides/api/gone.md?q=1#f""#));
        assert!(item.contains(r#"href="../../../out.md""#));
        assert!(item.contains(r#"href="https://example.com/x""#));
        assert_eq!(
            collected.assets,
            vec![SourceRel::new("guides/api/img/d.png")]
        );
    }

    #[test]
    fn r12_5_titles_and_paths_are_escaped_in_generated_headings() {
        let mut odd = page("we#ird/[x] 100%.md");
        odd.title = "[Weird] <b>".to_string();
        let site = Site {
            pages: vec![odd],
            ..Site::default()
        };
        let collected = vec![Collected {
            html_items: vec!["<li>x</li>".to_string()],
            assets: Vec::new(),
        }];
        let out = appendix(&site, &collected, false);
        assert_eq!(
            out,
            "<section data-ohm-todo-group>\n\n## [\\[Weird\\] \\<b\\>](<we%23ird/[x] 100%25.md>)\n\n<ul><li>x</li></ul>\n\n</section>\n\n"
        );
    }

    #[test]
    fn r12_5_file_groups_keep_page_order_and_timestamps() {
        let page_at = |source: &str,
                       title: &str,
                       created: Option<SystemTime>,
                       modified: Option<SystemTime>| {
            let mut page = page(source);
            page.title = title.to_string();
            page.times = PageTimes { created, modified };
            page
        };
        let site = Site {
            pages: vec![
                page_at(
                    "a.md",
                    "Alpha",
                    Some(UNIX_EPOCH - Duration::from_millis(1500)),
                    Some(UNIX_EPOCH),
                ),
                page_at("b.md", "Beta", None, None),
                page_at(
                    "c-empty.md",
                    "Empty",
                    Some(UNIX_EPOCH + Duration::from_secs(2)),
                    Some(UNIX_EPOCH + Duration::from_secs(3)),
                ),
                page_at(
                    "d.md",
                    "Delta",
                    Some(UNIX_EPOCH + Duration::from_secs(4)),
                    Some(UNIX_EPOCH + Duration::from_secs(10)),
                ),
            ],
            ..Site::default()
        };
        let collected = vec![
            Collected {
                html_items: vec![
                    "<li>alpha-first</li>".to_string(),
                    "<li>alpha-second</li>".to_string(),
                ],
                assets: Vec::new(),
            },
            Collected {
                html_items: vec!["<li>beta</li>".to_string()],
                assets: Vec::new(),
            },
            Collected::default(),
            Collected {
                html_items: vec!["<li>delta</li>".to_string()],
                assets: Vec::new(),
            },
        ];
        let expected_groups = [
            (
                "Alpha",
                "a.md",
                " data-ohm-created=\"-1500\" data-ohm-updated=\"0\"",
                "<ul><li>alpha-first</li><li>alpha-second</li></ul>",
            ),
            ("Beta", "b.md", "", "<ul><li>beta</li></ul>"),
            (
                "Delta",
                "d.md",
                " data-ohm-created=\"4000\" data-ohm-updated=\"10000\"",
                "<ul><li>delta</li></ul>",
            ),
        ];

        for appended_heading in [false, true] {
            let out = appendix(&site, &collected, appended_heading);
            let mut search_from = 0;
            for (title, source, attrs, items) in expected_groups {
                let group = format!(
                    "<section data-ohm-todo-group{attrs}>\n\n## [{title}](<{source}>)\n\n{items}\n\n</section>\n\n"
                );
                let offset = out[search_from..]
                    .find(&group)
                    .unwrap_or_else(|| panic!("missing or out-of-order group {title}: {out}"));
                search_from += offset + group.len();
            }
            assert!(!out.contains("Empty"), "empty groups are omitted: {out}");
            assert_eq!(
                out.contains("data-ohm-todo-collected"),
                appended_heading,
                "appendix wrapper follows mode: {out}"
            );
        }
    }

    #[test]
    fn appendix_wrappers_preserve_generated_headings_and_outline() {
        let mut source = page("notes.md");
        source.title = "Source Notes".to_string();
        let mut site = Site {
            pages: vec![source],
            ..Site::default()
        };
        site.by_source
            .insert("notes.md".to_string(), SourceKind::Page(0));
        let collected = vec![Collected {
            html_items: collect_from("- [ ] work {travel}\n"),
            assets: Vec::new(),
        }];
        for appended_heading in [false, true] {
            let markdown = appendix(&site, &collected, appended_heading);
            let rendered =
                render::render_page(&page("TODO.md"), &markdown, &site, &Diagrams::new());
            assert_eq!(rendered.outline.len(), if appended_heading { 2 } else { 1 });
            assert_eq!(rendered.outline.last().unwrap().id, "source-notes");
            assert!(rendered.html.contains("<section data-ohm-todo-group>"));
            assert_eq!(
                rendered.html.contains("data-ohm-todo-collected"),
                appended_heading
            );
            assert!(rendered.html.contains(r#"href="notes.html""#));
            assert!(
                rendered
                    .html
                    .contains(r#"data-ohm-tags="[&quot;travel&quot;]""#)
            );
            assert!(rendered.warnings.is_empty());
        }
    }

    #[test]
    fn r12_5_generated_file_links_round_trip_through_resolution() {
        let names = [
            "we#ird/[x] 100%.md",
            "Re: notes.md",
            "a&amp;b.md",
            "q?<x>\\y.md",
        ];
        let mut site = Site {
            pages: names.iter().map(|name| page(name)).collect(),
            ..Site::default()
        };
        for (index, name) in names.iter().enumerate() {
            site.by_source
                .insert(links::fold(name), SourceKind::Page(index));
        }
        let collected: Vec<Collected> = names
            .iter()
            .map(|_| Collected {
                html_items: vec!["<li>x</li>".to_string()],
                assets: Vec::new(),
            })
            .collect();
        let out = appendix(&site, &collected, false);
        assert!(out.contains("(<Re%3A notes.md>)"), "{out}");
        assert!(out.contains("(<a%26amp;b.md>)"), "{out}");

        // Each generated destination, parsed back as Markdown, resolves to its own page.
        let rendered = render::render_page(&page("TODO.md"), &out, &site, &Diagrams::new());
        assert!(rendered.warnings.is_empty(), "{:?}", rendered.warnings);
        // A missing page would have warned; one taken for a URL would keep its `.md`.
        assert!(!rendered.html.contains(".md\""), "{}", rendered.html);
        assert_eq!(rendered.html.matches(".html\"").count(), names.len());
    }

    #[test]
    fn r12_5_nothing_open_appends_nothing() {
        let site = Site {
            pages: vec![page("a.md")],
            ..Site::default()
        };
        for appended_heading in [true, false] {
            let out = appendix(&site, &[Collected::default()], appended_heading);
            assert_eq!(out, "", "appended_heading: {appended_heading}");
        }
    }

    #[test]
    fn r12_2_has_open_task_finds_an_open_item_at_any_depth() {
        assert!(has_open_task("- [x] done\n- [ ] open\n"));
        assert!(!has_open_task("- [x] done\n  - [x] nested done\n"));
        assert!(has_open_task("- [x] done\n  - [ ] open under done\n"));
        assert!(!has_open_task("# Title\n\n- plain\n\nNo tasks here.\n"));
    }
}
