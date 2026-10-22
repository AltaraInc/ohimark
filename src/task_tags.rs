//! Source-aware task tags, kept alongside the original parser events so collection
//! can inspect the Markdown tree before any rendering decoration is applied.

use std::ops::Range;

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd};

use crate::model::escape_html;

const TASK_ITEM_OPEN: &str = r#"<li class="ohm-task">"#;

#[derive(Default)]
pub(crate) struct Annotation {
    text: Option<String>,
    item_open: Option<String>,
    badges_before: Option<String>,
}

impl Annotation {
    /// Emit badges at the start of the original task label.
    pub(crate) fn emit_before<'a>(&self, out: &mut Vec<Event<'a>>) {
        if let Some(html) = &self.badges_before {
            out.push(Event::InlineHtml(CowStr::from(html.clone())));
        }
    }

    /// Apply decoration only after consumers have inspected the original event.
    pub(crate) fn decorate<'a>(&self, event: Event<'a>) -> Event<'a> {
        match (event, &self.item_open, &self.text) {
            (Event::Start(Tag::Item), Some(html), _) => Event::Html(CowStr::from(html.clone())),
            (Event::Text(_), _, Some(text)) => Event::Text(CowStr::from(text.clone())),
            (other, _, _) => other,
        }
    }
}

#[derive(Debug)]
struct TaskTag {
    key: String,
    label: String,
}

#[derive(Default)]
struct ProseState {
    owner: Option<usize>,
    // An unmatched source opener invalidates nested groups even when inline markup
    // splits it across runs. Block boundaries reset this state.
    depth: usize,
}

/// Parse once, retaining the structural events and a parallel decoration array.
pub(crate) fn parse(markdown: &str, options: Options) -> (Vec<Event<'_>>, Vec<Annotation>) {
    let (events, ranges): (Vec<_>, Vec<_>) = Parser::new_ext(markdown, options)
        .into_offset_iter()
        .unzip();
    let mut annotations: Vec<Annotation> =
        (0..events.len()).map(|_| Annotation::default()).collect();
    let mut tags: Vec<Vec<TaskTag>> = (0..events.len()).map(|_| Vec::new()).collect();
    let mut items = Vec::new();
    let mut excluded = 0usize;
    let mut run = Vec::new();
    let mut prose = ProseState::default();

    for (index, event) in events.iter().enumerate() {
        let owner = items.last().copied().filter(|&item| is_task(&events, item));
        if matches!(event, Event::Text(_)) && excluded == 0 && owner.is_some() {
            if prose.owner != owner {
                annotate_run(
                    markdown,
                    &events,
                    &ranges,
                    &run,
                    &mut prose,
                    &mut annotations,
                    &mut tags,
                );
                run.clear();
                prose = ProseState { owner, depth: 0 };
            }
            run.push(index);
            continue;
        }

        annotate_run(
            markdown,
            &events,
            &ranges,
            &run,
            &mut prose,
            &mut annotations,
            &mut tags,
        );
        run.clear();
        if ends_prose_block(event) {
            prose = ProseState::default();
        }
        match event {
            Event::Start(Tag::Item) => items.push(index),
            Event::End(TagEnd::Item) => {
                items.pop();
            }
            Event::Start(
                Tag::CodeBlock(_)
                | Tag::Heading { .. }
                | Tag::Link { .. }
                | Tag::Image { .. }
                | Tag::Table(_),
            ) => {
                excluded += 1;
            }
            Event::End(
                TagEnd::CodeBlock
                | TagEnd::Heading(_)
                | TagEnd::Link
                | TagEnd::Image
                | TagEnd::Table,
            ) => {
                excluded -= 1;
            }
            _ => {}
        }
    }
    annotate_run(
        markdown,
        &events,
        &ranges,
        &run,
        &mut prose,
        &mut annotations,
        &mut tags,
    );

    for (index, item_tags) in tags.iter().enumerate() {
        if !is_task(&events, index) {
            continue;
        }
        if item_tags.is_empty() {
            annotations[index].item_open = Some(TASK_ITEM_OPEN.to_string());
            continue;
        }
        let keys: Vec<&str> = item_tags.iter().map(|tag| tag.key.as_str()).collect();
        // Serialising strings cannot fail; HTML escaping protects the attribute too.
        let json = serde_json::to_string(&keys).expect("tag keys are serializable");
        annotations[index].item_open = Some(format!(
            r#"<li class="ohm-task" data-ohm-tags="{}">"#,
            escape_html(&json)
        ));
        let badges = item_tags
            .iter()
            .map(|tag| {
                format!(
                    r#"<span data-ohm-tag-color="{}" class="ohm-task-tag">{}</span>"#,
                    palette_slot(&tag.key),
                    escape_html(&tag.label)
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        let boundary = label_start(&events, index);
        annotations[boundary].badges_before =
            Some(format!(r#"<span class="ohm-task-tags">{badges}</span> "#));
    }
    (events, annotations)
}

/// FNV-1a over canonical UTF-8 keys keeps badge colors stable without browser state.
fn palette_slot(key: &str) -> u32 {
    key.bytes().fold(2_166_136_261u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(16_777_619)
    }) % 8
}

fn is_task(events: &[Event<'_>], index: usize) -> bool {
    if !matches!(events[index], Event::Start(Tag::Item)) {
        return false;
    }
    let next = if matches!(events.get(index + 1), Some(Event::Start(Tag::Paragraph))) {
        index + 2
    } else {
        index + 1
    };
    matches!(events.get(next), Some(Event::TaskListMarker(_)))
}

/// The task marker is followed by the first label event in either list style.
fn label_start(events: &[Event<'_>], item: usize) -> usize {
    if matches!(events.get(item + 1), Some(Event::Start(Tag::Paragraph))) {
        item + 3
    } else {
        item + 2
    }
}

fn is_inline_tag(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Emphasis
            | Tag::Strong
            | Tag::Strikethrough
            | Tag::Superscript
            | Tag::Subscript
            | Tag::Link { .. }
            | Tag::Image { .. }
    )
}

fn ends_prose_block(event: &Event<'_>) -> bool {
    match event {
        Event::Start(tag) => !is_inline_tag(tag),
        Event::End(end) => !matches!(
            end,
            TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Superscript
                | TagEnd::Subscript
                | TagEnd::Link
                | TagEnd::Image
        ),
        Event::Html(_) | Event::Rule | Event::DisplayMath(_) => true,
        _ => false,
    }
}

fn annotate_run(
    markdown: &str,
    events: &[Event<'_>],
    ranges: &[Range<usize>],
    run: &[usize],
    prose: &mut ProseState,
    annotations: &mut [Annotation],
    tags: &mut [Vec<TaskTag>],
) {
    let Some(owner) = prose.owner else { return };
    if run.is_empty() {
        return;
    }
    let mut text = String::new();
    let mut delimiters = Vec::new();
    let mut offsets = Vec::with_capacity(run.len());
    for &index in run {
        let Event::Text(part) = &events[index] else {
            unreachable!()
        };
        let offset = text.len();
        offsets.push(offset);
        // Entity expansions are synthesized text whose source is different. Only
        // literal source braces can delimit tags; escaped braces are checked against
        // the full source so an escape split into another Text event still counts.
        if markdown[ranges[index].clone()] == **part {
            for (byte, ch) in part.char_indices() {
                if matches!(ch, '{' | '}') {
                    let source = ranges[index].start + byte;
                    let slashes = markdown.as_bytes()[..source]
                        .iter()
                        .rev()
                        .take_while(|&&ch| ch == b'\\')
                        .count();
                    if slashes % 2 == 0 {
                        delimiters.push((offset + byte, ch));
                    }
                }
            }
        }
        text.push_str(part);
    }

    let mut removed = Vec::new();
    let mut cursor = 0;
    while cursor < delimiters.len() {
        let (start, ch) = delimiters[cursor];
        cursor += 1;
        if prose.depth > 0 {
            if ch == '{' {
                prose.depth += 1;
            } else {
                prose.depth -= 1;
            }
            continue;
        }
        if ch != '{' {
            continue;
        }
        let mut depth = 1usize;
        let mut nested = false;
        let mut end = None;
        while cursor < delimiters.len() {
            let (byte, ch) = delimiters[cursor];
            cursor += 1;
            if ch == '{' {
                nested = true;
                depth += 1;
            } else {
                depth -= 1;
                if depth == 0 {
                    end = Some(byte);
                    break;
                }
            }
        }
        let Some(end) = end else {
            prose.depth = depth;
            break;
        };
        if nested {
            continue;
        }
        let labels: Vec<_> = text[start + 1..end]
            .split(',')
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .collect();
        if labels.is_empty() {
            continue;
        }
        for label in labels {
            let key = label.to_lowercase();
            if !tags[owner].iter().any(|tag| tag.key == key) {
                tags[owner].push(TaskTag {
                    key,
                    label: label.to_string(),
                });
            }
        }
        removed.push(start..end + 1);
    }
    if removed.is_empty() {
        return;
    }
    for (&index, &start) in run.iter().zip(&offsets) {
        let Event::Text(part) = &events[index] else {
            unreachable!()
        };
        let end = start + part.len();
        let mut replacement = String::new();
        let mut copied = start;
        for range in &removed {
            if range.end <= start || range.start >= end {
                continue;
            }
            let removal_start = range.start.max(start);
            replacement.push_str(&text[copied..removal_start]);
            copied = range.end.min(end);
        }
        replacement.push_str(&text[copied..end]);
        if replacement != **part {
            annotations[index].text = Some(replacement);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(markdown: &str) -> String {
        let (events, annotations) = parse(markdown, crate::render::parser_options());
        let mut decorated = Vec::new();
        for (event, annotation) in events.into_iter().zip(annotations) {
            annotation.emit_before(&mut decorated);
            decorated.push(annotation.decorate(event));
        }
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, decorated.into_iter());
        html
    }

    #[test]
    fn groups_merge_trim_and_deduplicate_with_stable_labels() {
        let rendered = html("- [ ] { Travel, buy, , TRAVEL} pack {Buy, Work} now\n");
        assert!(
            rendered.contains(
                r#"data-ohm-tags="[&quot;travel&quot;,&quot;buy&quot;,&quot;work&quot;]""#
            ),
            "{rendered}"
        );
        for label in ["Travel", "buy", "Work"] {
            assert!(
                rendered.contains(&format!(r#"class="ohm-task-tag">{label}</span>"#)),
                "{rendered}"
            );
        }
        assert_eq!(rendered.matches("class=\"ohm-task-tag\"").count(), 3);
        assert!(!rendered.contains('{'));
    }

    #[test]
    fn static_colors_follow_canonical_keys_across_case_and_order() {
        for (markdown, labels) in [
            (
                "- [ ] trip {Travel, buy, café}\n",
                [("Travel", 1), ("buy", 3), ("café", 1)],
            ),
            (
                "- [ ] other {work, CAFÉ, BUY, travel}\n",
                [("travel", 1), ("BUY", 3), ("CAFÉ", 1)],
            ),
        ] {
            let rendered = html(markdown);
            for (label, color) in labels {
                assert!(
                    rendered.contains(&format!(
                        r#"data-ohm-tag-color="{color}" class="ohm-task-tag">{label}</span>"#
                    )),
                    "{rendered}"
                );
            }
        }
    }

    #[test]
    fn malformed_escaped_and_entity_braces_remain_literal() {
        let rendered =
            html(r"- [ ] {} { , } {outer {inner}} \{escaped} &#123;entity&#125; {unclosed");
        assert!(!rendered.contains("data-ohm-tags"), "{rendered}");
        assert!(
            rendered.contains("{} { , } {outer {inner}} {escaped} {entity} {unclosed"),
            "{rendered}"
        );
        let even = html(r"- [ ] \\{tag}");
        assert!(even.contains("data-ohm-tags"), "{even}");
        let odd = html(r"- [ ] \\\{tag}");
        assert!(!odd.contains("data-ohm-tags"), "{odd}");
    }

    #[test]
    fn adjacent_text_events_support_entities_and_escapes_in_labels() {
        let rendered = html(r"- [ ] {a &amp; b, c\*d, &#123;literal&#125;}");
        assert!(
            rendered.contains(r#"class="ohm-task-tag">a &amp; b</span>"#),
            "{rendered}"
        );
        assert!(
            rendered.contains(r#"class="ohm-task-tag">c*d</span>"#),
            "{rendered}"
        );
        assert!(
            rendered.contains(r#"class="ohm-task-tag">{literal}</span>"#),
            "{rendered}"
        );
    }

    #[test]
    fn protected_content_and_inline_boundaries_stay_literal() {
        let rendered = html(
            "- [ ] *{yes}* {no *span*} `{code}` [{link}](x) ![{image}](x) <b title=\"{html}\">{outside}</b> {no<br>span}\n\n  # {heading}\n\n  ```\n  {fence}\n  ```\n",
        );
        assert!(
            rendered.contains(r#"data-ohm-tags="[&quot;yes&quot;,&quot;outside&quot;]""#),
            "{rendered}"
        );
        for text in [
            "{no <em>span</em>}",
            "{code}",
            "{link}",
            "{image}",
            "{html}",
            "{no<br>span}",
            "{heading}",
            "{fence}",
        ] {
            assert!(rendered.contains(text), "missing {text}: {rendered}");
        }
    }

    #[test]
    fn groups_do_not_cross_breaks_or_code_spans() {
        for markdown in [
            "- [ ] {one\n  two}\n",
            "- [ ] {one  \n  two}\n",
            "- [ ] {one `code` two}\n",
            "- [ ] {one\n\n  two}\n",
        ] {
            let rendered = html(markdown);
            assert!(!rendered.contains("data-ohm-tags"), "{rendered}");
            assert!(rendered.contains("{one"), "{rendered}");
            assert!(rendered.contains("two}"), "{rendered}");
        }
    }

    #[test]
    fn tables_keep_literal_braces_without_contributing_tags() {
        let rendered = html(
            "- [ ] trip {travel}\n\n  | {header} | Notes |\n  | --- | --- |\n  | {literal} | Details |\n\n  more prose\n",
        );
        assert!(
            rendered.contains(r#"data-ohm-tags="[&quot;travel&quot;]""#),
            "{rendered}"
        );
        assert_eq!(rendered.matches("class=\"ohm-task-tag\"").count(), 1);
        assert!(rendered.contains("<th>{header}</th>"), "{rendered}");
        assert!(rendered.contains("<td>{literal}</td>"), "{rendered}");
    }

    #[test]
    fn malformed_groups_remain_literal_across_inline_boundaries() {
        for (source, literal) in [
            ("{outer *{inner}*}", "{outer <em>{inner}</em>}"),
            ("{outer <b>{inner}</b>}", "{outer <b>{inner}</b>}"),
            ("{bad *format*}", "{bad <em>format</em>}"),
            ("{outer `}` {inner}}", "{outer <code>}</code> {inner}}"),
            (
                "{outer [close }](x) {inner}}",
                "{outer <a href=\"x\">close }</a> {inner}}",
            ),
            (
                "{outer <b title=\"}\">{inner}</b>}",
                "{outer <b title=\"}\">{inner}</b>}",
            ),
        ] {
            let rendered = html(&format!("- [ ] {source} {{good}}\n"));
            assert!(rendered.contains(literal), "{rendered}");
            assert!(
                rendered.contains(r#"data-ohm-tags="[&quot;good&quot;]""#),
                "{rendered}"
            );
            assert_eq!(rendered.matches("class=\"ohm-task-tag\"").count(), 1);
        }
        let valid = html("- [ ] *{valid}*\n");
        assert!(
            valid.contains(r#"data-ohm-tags="[&quot;valid&quot;]""#),
            "{valid}"
        );
    }

    #[test]
    fn unclosed_groups_do_not_suppress_later_paragraphs_or_child_tasks() {
        for markdown in [
            "- [ ] parent {unclosed *format*\n\n  later {travel}\n\n  - [ ] child {child}\n",
            "- [ ] parent {unclosed *format*\n  - [ ] child {child}\n",
        ] {
            let rendered = html(markdown);
            assert!(rendered.contains("{unclosed <em>format</em>"), "{rendered}");
            assert!(
                rendered.contains(r#"data-ohm-tags="[&quot;child&quot;]""#),
                "{rendered}"
            );
            if markdown.contains("later") {
                assert!(
                    rendered.contains(r#"data-ohm-tags="[&quot;travel&quot;]""#),
                    "{rendered}"
                );
                assert_eq!(rendered.matches("data-ohm-tags=").count(), 2);
            } else {
                assert_eq!(rendered.matches("data-ohm-tags=").count(), 1);
            }
        }
    }

    #[test]
    fn own_paragraphs_contribute_without_descendant_tags() {
        let rendered =
            html("- [ ] parent {a}\n\n  later {b}\n\n  - plain {plain}\n    - [x] child {c}\n");
        assert!(
            rendered.contains(r#"data-ohm-tags="[&quot;a&quot;,&quot;b&quot;]""#),
            "{rendered}"
        );
        assert!(
            rendered.contains(r#"data-ohm-tags="[&quot;c&quot;]""#),
            "{rendered}"
        );
        assert!(rendered.contains("plain {plain}"), "{rendered}");
        let paragraph = rendered.find("<p>").unwrap();
        let checkbox = rendered
            .find(r#"<input disabled="" type="checkbox"/>"#)
            .unwrap();
        let badges = rendered.find("class=\"ohm-task-tags\"").unwrap();
        let parent = rendered.find("parent").unwrap();
        assert!(
            paragraph < checkbox && checkbox < badges && badges < parent,
            "{rendered}"
        );
        assert!(badges < rendered.find("</p>").unwrap(), "{rendered}");
        assert!(badges < rendered.find("later").unwrap(), "{rendered}");
    }

    #[test]
    fn badge_labels_and_json_attributes_are_escaped() {
        let rendered = html("- [ ] {a&amp;&quot;&#39;&lt;b&gt;, ü}\n");
        assert!(
            rendered.contains(r#"class="ohm-task-tag">a&amp;&quot;&#39;&lt;b&gt;</span>"#),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                r#"data-ohm-tags="[&quot;a&amp;\&quot;&#39;&lt;b&gt;&quot;,&quot;ü&quot;]""#
            ),
            "{rendered}"
        );
    }
}
