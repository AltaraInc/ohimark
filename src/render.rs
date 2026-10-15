//! Markdown-to-HTML rendering for one page, including Mermaid-to-SVG (R7, R8.2, R6.8).
//!
//! One pass over the `pulldown-cmark` event stream, with five interceptions: links and
//! images go through `links::resolve` (R6.1-R6.7, R6.9, R5.1-R5.2); headings receive a
//! stable, per-page-deduplicated anchor and feed the outline, resolving any links or
//! images buffered inside them through that same pass rather than skipping it, and
//! rendering the outline's copy of the heading with any anchor-producing event
//! stripped so it does not nest inside the outline's own anchor (R6.8, R8.3); a fence
//! tagged `mermaid` is rendered to a themable inline SVG at build time (R7.4, R7.7,
//! R8.2); every other fence and indented code block becomes an escaped, verbatim
//! `<pre><code>` (R7.3); and a task-list item is tagged so the stylesheet can drop its
//! marker (R7.8).
//! `render_page` never fails: every problem becomes a warning and a graceful fallback.

use std::collections::HashMap;

use camino::Utf8Path;
use pulldown_cmark::{CodeBlockKind, CowStr, Event, HeadingLevel, Options, Tag, TagEnd};

use merman::svg::{
    CssOverridePolicy, HostTheme, HostThemeAppearance, Presentation, PresentationRenderPolicy,
    SvgOutputPolicy, SvgPipelinePreset, SvgRenderOptions, ThemeRole, sanitize_svg_id,
};
use merman::{
    Engine, MermaidConfig, OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest,
};

use crate::links::{self, Resolved};
use crate::model::{OutlineEntry, Page, Site, SourceRel, TargetPath, Warnings, escape_html};
use crate::task_tags;

/// One rendered page: its body HTML plus everything the rest of the build needs from it.
pub struct Rendered {
    /// The `<article>` inner HTML. Not a whole document - `theme::shell` wraps it.
    pub html: String,
    /// The page's headings, in document order (R8.3).
    pub outline: Vec<OutlineEntry>,
    /// Non-Markdown files this page referenced, so `emit` copies them (R5.1). May contain
    /// duplicates; the caller de-duplicates.
    pub assets: Vec<SourceRel>,
    /// This page's warnings, in the order they arose. Merged by the caller after the
    /// parallel render phase, so stderr stays deterministic (R11.3).
    pub warnings: Warnings,
}

/// The Markdown dialect every page is parsed with: CommonMark plus tables,
/// strikethrough, task lists and footnotes (R7). Shared with `todo`, which re-parses
/// each page to lift its open task items out (R12.2).
#[must_use]
pub fn parser_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);
    options
}

/// The ids that must stay unique across one page: heading anchors (R6.8) and Mermaid
/// diagram ids. Fresh for every page, so identical ids on different pages are never a
/// conflict; shared between the parts of a page rendered separately (R12.4). Also
/// records whether the page's filename subtitle has already been emitted.
#[derive(Debug, Default)]
pub struct PageIds {
    headings: HashMap<String, u32>,
    diagrams: usize,
    filename_emitted: bool,
}

/// Render one page's Markdown to body HTML. Never fails: every problem is a warning
/// (R7.7, R5.2, R6.6, R10.5).
#[must_use]
pub fn render_page(page: &Page, markdown: &str, site: &Site, diagrams: &Diagrams) -> Rendered {
    render_part(page, markdown, site, diagrams, &mut PageIds::default())
}

/// Render one separately parsed part of a page, so a construct left open at the end of
/// one part cannot swallow the next (R12.4). `ids` carries over from the page's earlier
/// parts, keeping its anchors and diagram ids unique and emitting its filename once.
#[must_use]
pub fn render_part(
    page: &Page,
    markdown: &str,
    site: &Site,
    diagrams: &Diagrams,
    ids: &mut PageIds,
) -> Rendered {
    let mut warnings = Warnings::new();
    let mut assets: Vec<SourceRel> = Vec::new();
    let mut outline: Vec<OutlineEntry> = Vec::new();
    let heading_ids = &mut ids.headings;
    let mermaid_index = &mut ids.diagrams;
    let filename_emitted = &mut ids.filename_emitted;

    let source_filename = page
        .source
        .as_path()
        .file_name()
        .unwrap_or(page.source.as_path().as_str());
    let filename_html = format!(
        "<div class=\"ohm-page-filename\">{}</div>\n",
        escape_html(source_filename)
    );

    let source_dir = page.source.dir();
    let target_dir = page.target.dir();

    let (events, annotations) = task_tags::parse(markdown, parser_options());
    let mut events_iter = events.into_iter().zip(&annotations);
    let mut out_events: Vec<Event> = Vec::new();
    let mut containers: Vec<TagEnd> = Vec::new();

    while let Some((event, annotation)) = events_iter.next() {
        annotation.emit_before(&mut out_events);
        // A title must be outside every enclosing Markdown container. Headings and
        // code blocks consume their own end events below, so neither enters this stack.
        match &event {
            Event::Start(Tag::Heading { .. } | Tag::CodeBlock(_)) => {}
            Event::Start(tag) => containers.push(tag.to_end()),
            Event::End(_) => {
                containers.pop();
            }
            _ => {}
        }
        match event {
            // Interception 2: headings (R6.8, R8.3). Buffered separately from the main
            // loop, so its contents are routed through `resolve_event` explicitly rather
            // than falling out of the loop where the link/image arm would have caught
            // them (R6.1-R6.9, R5.1-R5.2).
            Event::Start(Tag::Heading { level, id, .. }) => {
                let mut inner: Vec<Event> = Vec::new();
                for (ev, _) in events_iter.by_ref() {
                    if matches!(ev, Event::End(TagEnd::Heading(_))) {
                        break;
                    }
                    inner.push(ev);
                }
                let inner: Vec<Event> = inner
                    .into_iter()
                    .map(|ev| {
                        resolve_event(
                            ev,
                            source_dir,
                            target_dir,
                            site,
                            page,
                            &mut warnings,
                            &mut assets,
                        )
                    })
                    .collect();

                let plain = plain_text_of(&inner);
                let mut inline_html = String::new();
                pulldown_cmark::html::push_html(&mut inline_html, inner.iter().cloned());

                // The outline entry sits inside its own `<a href="#…">` (see
                // `theme::render_outline`). A link or footnote reference surviving from the
                // heading would nest a second anchor inside that one, which is invalid
                // HTML; browsers repair it by closing the outer anchor early, so the
                // outline entry stops linking to its own heading (R8.3). Dropping just the
                // anchor-producing events — not their contents — keeps the outline text
                // matching the heading everywhere except that one anchor.
                let mut outline_html = String::new();
                pulldown_cmark::html::push_html(
                    &mut outline_html,
                    inner
                        .iter()
                        .filter(|ev| {
                            !matches!(
                                ev,
                                Event::Start(Tag::Link { .. })
                                    | Event::End(TagEnd::Link)
                                    | Event::FootnoteReference(_)
                            )
                        })
                        .cloned(),
                );

                let level_num = level as u8;
                let base = match id {
                    Some(explicit) => explicit.to_string(),
                    None => slugify(&plain),
                };
                let anchor = dedup_heading_id(base, heading_ids);

                out_events.push(Event::Html(CowStr::from(format!(
                    "<h{level_num} id=\"{}\">{inline_html}</h{level_num}>\n",
                    escape_html(&anchor)
                ))));
                if !*filename_emitted && level == HeadingLevel::H1 && containers.is_empty() {
                    out_events.push(Event::Html(CowStr::from(filename_html.clone())));
                    *filename_emitted = true;
                }
                outline.push(OutlineEntry {
                    level: level_num,
                    id: anchor,
                    text: outline_html,
                });
            }
            // Interceptions 3 and 4: fenced/indented code blocks (R7.3, R7.4, R7.7).
            Event::Start(Tag::CodeBlock(kind)) => {
                let mut code = String::new();
                for (ev, _) in events_iter.by_ref() {
                    match ev {
                        Event::Text(t) => code.push_str(&t),
                        Event::End(TagEnd::CodeBlock) => break,
                        _ => {}
                    }
                }
                let info: &str = match &kind {
                    CodeBlockKind::Fenced(info) => info,
                    CodeBlockKind::Indented => "",
                };
                let first_word = info.split_whitespace().next().unwrap_or("");

                if !first_word.is_empty() && first_word.eq_ignore_ascii_case("mermaid") {
                    let markup =
                        render_mermaid_fence(diagrams, &code, page, *mermaid_index, &mut warnings);
                    *mermaid_index += 1;
                    out_events.push(Event::Html(CowStr::from(markup)));
                } else {
                    let class_attr = if first_word.is_empty() {
                        String::new()
                    } else {
                        format!(" class=\"language-{}\"", escape_html(first_word))
                    };
                    out_events.push(Event::Html(CowStr::from(format!(
                        "<pre><code{class_attr}>{}</code></pre>\n",
                        escape_html(&code)
                    ))));
                }
            }
            // Interception 1: links and images (R6.1-R6.7, R6.9, R5.1-R5.2). Also used by
            // the heading arm above, whose contents are buffered and rendered separately
            // and would otherwise never reach this resolution.
            ev @ (Event::Start(Tag::Link { .. }) | Event::Start(Tag::Image { .. })) => {
                out_events.push(resolve_event(
                    ev,
                    source_dir,
                    target_dir,
                    site,
                    page,
                    &mut warnings,
                    &mut assets,
                ));
            }
            // Interception 5: task classes, metadata, badges and tag-free prose. The
            // parallel annotation preserves structural events until emission, shared
            // with TODO collection's nesting and checked-state analysis.
            // Everything else, including raw HTML (R7.5), passes through unchanged.
            other => out_events.push(annotation.decorate(other)),
        }
    }

    // The first part without a top-level H1 still owns the subtitle. In particular,
    // a later generated TODO section must not claim it as its own title subtitle.
    if !*filename_emitted {
        out_events.insert(0, Event::Html(CowStr::from(filename_html)));
        *filename_emitted = true;
    }

    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, out_events.into_iter());

    Rendered {
        html,
        outline,
        assets,
        warnings,
    }
}

/// Rewrite a link's or image's destination through `resolve_href`, passing every other
/// event through untouched. Shared with the heading arm, whose contents are rendered
/// separately and would otherwise skip resolution entirely (R6.1-R6.9, R5.1-R5.2).
fn resolve_event<'a>(
    event: Event<'a>,
    source_dir: &Utf8Path,
    target_dir: &Utf8Path,
    site: &Site,
    page: &Page,
    warnings: &mut Warnings,
    assets: &mut Vec<SourceRel>,
) -> Event<'a> {
    match event {
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let new_url = resolve_href(
                &dest_url, source_dir, target_dir, site, page, warnings, assets,
            );
            Event::Start(Tag::Link {
                link_type,
                dest_url: CowStr::from(new_url),
                title,
                id,
            })
        }
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let new_url = resolve_href(
                &dest_url, source_dir, target_dir, site, page, warnings, assets,
            );
            Event::Start(Tag::Image {
                link_type,
                dest_url: CowStr::from(new_url),
                title,
                id,
            })
        }
        other => other,
    }
}

/// Resolve one href/src via `links::resolve`, recording a warning or an asset reference
/// as needed. The unresolved forms are always emitted byte-for-byte (R6.5, R6.6).
fn resolve_href(
    raw: &str,
    source_dir: &Utf8Path,
    target_dir: &Utf8Path,
    site: &Site,
    page: &Page,
    warnings: &mut Warnings,
    assets: &mut Vec<SourceRel>,
) -> String {
    match links::resolve(raw, source_dir, target_dir, site) {
        Resolved::Unchanged => raw.to_string(),
        Resolved::Page(href) => href,
        Resolved::Asset { href, source } => {
            assets.push(source);
            href
        }
        Resolved::Missing => {
            warnings.warn(format!("`{}`: unresolved reference `{raw}`", page.source));
            raw.to_string()
        }
    }
}

/// Plain text of a buffered heading's inner events: only `Text` and `Code` contribute,
/// matching the "plain text of the heading" that R6.8 slugs from.
fn plain_text_of(events: &[Event]) -> String {
    let mut out = String::new();
    for ev in events {
        match ev {
            Event::Text(t) | Event::Code(t) => out.push_str(t),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            _ => {}
        }
    }
    out
}

/// Slug a heading's plain text into an anchor identifier (R6.8): lowercase, runs of
/// non-alphanumeric ASCII collapsed to a single `-`, leading/trailing `-` trimmed. Non-ASCII
/// letters are dropped (they fall into the collapsed runs), keeping the result deterministic.
fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_dash = true; // suppresses a leading dash
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "section".to_string()
    } else {
        out
    }
}

/// Deduplicate an anchor id within one page (R6.8): the first occurrence keeps the bare
/// id; later occurrences of the same id take `-1`, `-2`, ... in document order.
fn dedup_heading_id(base: String, seen: &mut HashMap<String, u32>) -> String {
    match seen.get(&base).copied() {
        None => {
            seen.insert(base.clone(), 0);
            base
        }
        Some(count) => {
            let next = count + 1;
            seen.insert(base.clone(), next);
            format!("{base}-{next}")
        }
    }
}

/// A stable-across-builds slug for a page's target path, used to scope Mermaid diagram ids.
fn target_slug(target: &TargetPath) -> String {
    slugify(target.as_str())
}

/// Render a `mermaid`-tagged fence to the `<figure>` markup (R7.4), or warn and fall back
/// to an ordinary fenced code block (R7.7).
fn render_mermaid_fence(
    diagrams: &Diagrams,
    code: &str,
    page: &Page,
    index: usize,
    warnings: &mut Warnings,
) -> String {
    let unique_id = format!("ohm-{}-{index}", target_slug(&page.target));
    match diagrams.render(code, &unique_id) {
        Ok(svg) => format!(
            "<figure class=\"ohm-diagram\">\n<div class=\"ohm-diagram-graphic\">{svg}</div>\n<pre class=\"ohm-diagram-source\"><code class=\"language-mermaid\">{}</code></pre>\n</figure>\n",
            escape_html(code)
        ),
        Err(reason) => {
            warnings.warn(format!(
                "`{}`: mermaid diagram failed to render: {reason}",
                page.source
            ));
            format!(
                "<pre><code class=\"language-mermaid\">{}</code></pre>\n",
                escape_html(code)
            )
        }
    }
}

/// Renders Mermaid to themed SVG. Build one and share it across the parallel render
/// phase - `&Diagrams` is `Sync`: it owns only a `Renderer` (verified `Sync` against merman
/// 0.8.0-alpha.6), a small `Copy` render policy, an `SvgOutputPolicy`, and a plain
/// `HashMap<(u8, u8, u8), String>`, none of which hold interior mutability.
pub struct Diagrams {
    renderer: Renderer,
    policy: SvgOutputPolicy,
    render_policy: PresentationRenderPolicy,
    color_map: HashMap<(u8, u8, u8), String>,
}

impl Diagrams {
    /// Builds the renderer, the sentinel-themed presentation and the colour substitution
    /// table once, so every page's render call is a cheap, shared, read-only operation.
    #[must_use]
    pub fn new() -> Self {
        let mut color_map: HashMap<(u8, u8, u8), String> = HashMap::new();

        // Colour substitution (R8.2): merman rejects `var(--x)` as a colour outright, so
        // each diagram is rendered once with every themeable role and series slot set to a
        // unique, implausible sentinel colour. The rendered SVG is then scanned and every
        // sentinel occurrence rewritten into the matching CSS custom property, so one
        // render serves both the light and dark theme with no browser-side re-render.
        let mut theme = HostTheme::new().with_appearance(HostThemeAppearance::Light);
        for (i, role) in ThemeRole::ALL.into_iter().enumerate() {
            let (r, g, b) = sentinel_channels(i);
            let hex = format!("#{r:02x}{g:02x}{b:02x}");
            let variable = format!("ohm-dg-{}", role.id());
            theme = theme
                .try_with_role(role, &hex)
                .expect("sentinel colours are valid hex");
            let previous = color_map.insert((r, g, b), variable);
            assert!(previous.is_none(), "sentinel collision building role table");
        }

        let mut series_hex: Vec<String> = Vec::with_capacity(12);
        for n in 0..12usize {
            let (r, g, b) = sentinel_channels(22 + n);
            let hex = format!("#{r:02x}{g:02x}{b:02x}");
            let variable = format!("ohm-dg-series-{n}");
            let previous = color_map.insert((r, g, b), variable);
            assert!(
                previous.is_none(),
                "sentinel collision building series table"
            );
            series_hex.push(hex);
        }
        let series_refs: Vec<&str> = series_hex.iter().map(String::as_str).collect();
        theme = theme
            .try_with_series_palette(series_refs)
            .expect("sentinel colours are valid hex");
        theme = theme
            .try_with_font_family("var(--ohm-font-sans)")
            .expect("font family is a valid CSS value")
            .try_with_font_size("14px")
            .expect("font size is a valid CSS value");

        // Literals merman hard-codes independently of the host theme (observed in the
        // integration spike). Inserted after the sentinel table and never overwriting it;
        // the two ranges are numerically disjoint by construction.
        for &(rgb, variable) in FALLBACK_LITERALS {
            color_map.entry(rgb).or_insert_with(|| variable.to_string());
        }

        let resolved = Presentation::new().with_theme(theme).resolve();
        // Layout, merged after the theme so it wins. merman measures label text with a
        // family-independent heuristic, so only the font size changes node sizes and
        // `flowchart.padding` is the sole margin against a label overflowing its node: do
        // not lower it below 12. Gantt charts are drawn by `crate::gantt` from merman's
        // layout, so their block only sets the default tick label format (`Jan 05`); an
        // `axisFormat` line in a diagram still overrides it.
        let layout = MermaidConfig::from_value(serde_json::json!({
            "fontSize": 14,
            "flowchart": { "nodeSpacing": 30, "rankSpacing": 32, "padding": 12 },
            "state": { "nodeSpacing": 30, "rankSpacing": 32 },
            "sequence": {
                "actorMargin": 32,
                "width": 120,
                "height": 44,
                "diagramMarginX": 16,
                "diagramMarginY": 8
            },
            "gantt": { "axisFormat": "%b %d" }
        }));
        let engine = resolved
            .materialize_engine(Engine::new())
            .with_site_config(layout);
        let renderer = Renderer::new().with_engine(engine);
        let render_policy = resolved.render_policy();

        let (canvas_r, canvas_g, canvas_b) = sentinel_channels(0);
        let canvas_hex = format!("#{canvas_r:02x}{canvas_g:02x}{canvas_b:02x}");
        let policy = SvgOutputPolicy {
            preset: SvgPipelinePreset::Parity,
            css_override_policy: CssOverridePolicy::Preserve,
            root_background_color: Some(canvas_hex),
            ..Default::default()
        };

        Self {
            renderer,
            policy,
            render_policy,
            color_map,
        }
    }

    /// Renders one Mermaid source to themed SVG, or a reason it could not be rendered
    /// (R7.7). `unique_id` scopes the SVG's own `<style>` block and marker ids so several
    /// diagrams can share one HTML document.
    fn render(&self, source: &str, unique_id: &str) -> Result<String, String> {
        // Gantt charts are drawn by `crate::gantt` from merman's computed layout. Whatever
        // that path cannot draw falls through to merman's own SVG, so a gantt chart never
        // fails harder than any other diagram.
        if is_gantt_source(source)
            && let Some(svg) = self.render_gantt(source, unique_id)
        {
            return Ok(svg);
        }
        match self.renderer.render(RenderRequest::svg(
            source,
            OperationControl::new(),
            self.request(unique_id),
        )) {
            Ok(RenderOutput::Svg(Some(svg))) => {
                Ok(substitute_svg_colors(svg.svg(), &self.color_map))
            }
            Ok(_) => Err("no diagram detected".to_string()),
            Err(error) => Err(error.to_string()),
        }
    }

    /// Draws a gantt chart from merman's layout, or `None` when merman produced no usable
    /// layout for it.
    fn render_gantt(&self, source: &str, unique_id: &str) -> Option<String> {
        let request =
            RenderRequest::layout_json(source, OperationControl::new(), self.request(unique_id));
        let Ok(RenderOutput::LayoutJson(Some(output))) = self.renderer.render(request) else {
            return None;
        };
        crate::gantt::render(output.layout(), &sanitize_svg_id(unique_id))
    }

    fn request(&self, unique_id: &str) -> SvgRequest {
        SvgRequest {
            pipeline: Some(self.policy.pipeline()),
            presentation: self.render_policy,
            options: SvgRenderOptions {
                diagram_id: Some(sanitize_svg_id(unique_id)),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// Whether a Mermaid source declares a gantt chart: its first meaningful line starts with
/// the word `gantt`, once blank lines, `%%` comments and directives and a leading `---`
/// front-matter block are skipped. Purely textual, so no other diagram pays for a parse.
fn is_gantt_source(source: &str) -> bool {
    let mut lines = source.lines().map(str::trim).peekable();
    while lines.next_if(|line| line.is_empty()).is_some() {}
    if lines.next_if_eq(&"---").is_some() {
        // An unterminated block consumes every line, which leaves nothing to match.
        lines
            .by_ref()
            .take_while(|line| *line != "---")
            .for_each(drop);
    }
    let mut in_directive = false;
    for line in lines {
        let rest = if in_directive || line.starts_with("%%{") {
            // A directive may span lines and may share its last one with the declaration.
            match line.split_once("}%%") {
                Some((_, rest)) => {
                    in_directive = false;
                    rest.trim()
                }
                None => {
                    in_directive = true;
                    continue;
                }
            }
        } else {
            line
        };
        if rest.is_empty() || rest.starts_with("%%") {
            continue;
        }
        return rest
            .strip_prefix("gantt")
            .is_some_and(|after| !after.starts_with(|c: char| c.is_alphanumeric() || c == '_'));
    }
    false
}

impl Default for Diagrams {
    fn default() -> Self {
        Self::new()
    }
}

/// The `i`th sentinel colour: a dark, near-black ramp where every index yields a distinct
/// `(r, g, b)` triple by construction (each channel is strictly increasing in `i`).
fn sentinel_channels(i: usize) -> (u8, u8, u8) {
    let r = 1 + 3 * i;
    let g = 2 + 3 * i;
    let b = 3 + 3 * i;
    (r as u8, g as u8, b as u8)
}

/// Literal colours merman hard-codes regardless of the host theme, mapped so the active
/// theme still governs them. `(r, g, b)` keys, matched after decoding any recognised colour
/// syntax, so `black`, `#000`, `#000000` and `rgb(0, 0, 0)` all resolve identically.
const FALLBACK_LITERALS: &[((u8, u8, u8), &str)] = &[
    ((0x00, 0x00, 0x00), "ohm-dg-text"),
    ((0xff, 0xff, 0xff), "ohm-dg-canvas"),
    ((0x66, 0x66, 0x66), "ohm-dg-actor-border"),
    ((0x99, 0x99, 0x99), "ohm-dg-line"),
    ((0xea, 0xea, 0xea), "ohm-dg-actor-background"),
    ((0xe0, 0xe0, 0xe0), "ohm-dg-border"),
    ((0xf4, 0xf4, 0xf4), "ohm-dg-surface-alt"),
    ((0xed, 0xf2, 0xae), "ohm-dg-note-background"),
    ((0xff, 0xf4, 0xdd), "ohm-dg-note-background"),
    ((0xb9, 0xb9, 0xb9), "ohm-dg-border"),
];

// ---------------------------------------------------------------------------------------
// Colour substitution (R8.2).
//
// merman's theme system rejects `var(--x)` as a colour outright (it does arithmetic on
// role values), so a diagram cannot be rendered with CSS variables directly. Instead every
// themeable slot is rendered once with a unique sentinel colour (see `Diagrams::new`), and
// this section rewrites every sentinel occurrence in the emitted SVG text into the matching
// `var(--ohm-dg-*)` (or a `color-mix` when the source colour carried alpha). Anything not a
// recognised colour syntax, or a recognised colour outside the sentinel/fallback tables, is
// left exactly as-is: this scanner never guesses.
// ---------------------------------------------------------------------------------------

/// Rewrites every recognised colour literal in `svg` into its CSS custom property.
fn substitute_svg_colors(svg: &str, color_map: &HashMap<(u8, u8, u8), String>) -> String {
    let mut out = String::with_capacity(svg.len());
    let len = svg.len();
    let mut i = 0usize;

    while i < len {
        let rest = &svg[i..];
        let ch = rest.chars().next().expect("i < len implies a char");

        if ch == '#'
            && hex_is_color_context(&out)
            && let Some((consumed, (r, g, b, a))) = parse_hex_color(rest)
            && let Some(variable) = color_map.get(&(r, g, b))
        {
            push_substitution(&mut out, variable, a);
            i += consumed;
            continue;
        } else if ch == 'r' || ch == 'R' {
            // No context guard: no other CSS/SVG construct spells `rgb(`/`rgba(`, so these
            // are unambiguous wherever they appear (e.g. inside `filter:drop-shadow(...)`,
            // preceded by a space rather than `:` or `="`).
            if let Some((consumed, (r, g, b, a))) = parse_rgb_like(rest) {
                match color_map.get(&(r, g, b)) {
                    Some(variable) => push_substitution(&mut out, variable, a),
                    None => out.push_str(&rest[..consumed]),
                }
                i += consumed;
                continue;
            }
        } else if ch == 'w' || ch == 'W' {
            if let Some(consumed) = match_color_keyword(svg, &out, i, "white") {
                match color_map.get(&(0xff, 0xff, 0xff)) {
                    Some(variable) => push_substitution(&mut out, variable, 1.0),
                    None => out.push_str(&rest[..consumed]),
                }
                i += consumed;
                continue;
            }
        } else if (ch == 'b' || ch == 'B')
            && let Some(consumed) = match_color_keyword(svg, &out, i, "black")
        {
            match color_map.get(&(0x00, 0x00, 0x00)) {
                Some(variable) => push_substitution(&mut out, variable, 1.0),
                None => out.push_str(&rest[..consumed]),
            }
            i += consumed;
            continue;
        }

        out.push(ch);
        i += ch.len_utf8();
    }

    out
}

// A positive context rule, derived from a census of every `#` context in real `merman`
// output (flowchart and sequence diagrams). Colour positions are always preceded by `:`
// or by `="` (optionally with one space): `fill:#…`, `;fill:#…`, `stop-color="#…`,
// `background-color: white`. Non-colour positions - `url(#…)` marker references, and CSS
// id selectors after `}`, `,` or `{` (`…;}#flow .node`) - are preceded by something else
// entirely, so they are never touched. This is a positive allow-list, not a deny-list: it
// is strictly safer than trying to enumerate every non-colour context.

/// True when scanning backward from `out`'s end, through characters that can appear
/// inside a CSS declaration's value (ASCII letters/digits, space, `-`, `.`, `%`), we reach
/// a `:` before reaching anything else. This covers the direct case (`fill:#…`,
/// `background-color: white`) and CSS shorthand, where the colour is not the token
/// immediately after `:` (`border:1px solid #1f2021`). Any declaration-terminating or
/// selector character (`;`, `"`, `{`, `}`, `(`, `,`, ...) stops the scan and reports "not a
/// colour position", so this can never cross into an unrelated declaration or a CSS id
/// selector. Bounded so a very long, unrelated run of text cannot force a long scan.
fn preceded_by_colon(out: &str) -> bool {
    const MAX_SCAN: usize = 80;
    for (steps, ch) in out.chars().rev().enumerate() {
        if steps >= MAX_SCAN {
            return false;
        }
        match ch {
            ':' => return true,
            c if c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '.' || c == '%' => {}
            _ => return false,
        }
    }
    false
}

/// True when `out` ends with `="` (skipping at most one trailing space before the `"`) -
/// the start of an HTML/SVG attribute value. Returns the text preceding the `=`, so the
/// caller can inspect the attribute name.
fn preceded_by_attr_equals(out: &str) -> Option<&str> {
    let bytes = out.as_bytes();
    let mut pos = bytes.len();
    if pos == 0 {
        return None;
    }
    if bytes[pos - 1] == b' ' {
        pos -= 1;
        if pos == 0 {
            return None;
        }
    }
    if bytes[pos - 1] != b'"' || pos < 2 || bytes[pos - 2] != b'=' {
        return None;
    }
    Some(&out[..pos - 2])
}

/// True when the attribute name immediately preceding `prefix`'s end is `href` or
/// `xlink:href`, so `href="#anchor"` is never mistaken for a colour.
fn attr_name_is_href(prefix: &str) -> bool {
    let name: String = prefix
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == ':')
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    name.eq_ignore_ascii_case("href") || name.eq_ignore_ascii_case("xlink:href")
}

/// True when the `#` about to be scanned is in a colour position: preceded by `:` or by
/// `="` (optionally with one space), and - in the `="` case - not an `href`/`xlink:href`
/// attribute (so `href="#anchor"` passes through untouched).
fn hex_is_color_context(out: &str) -> bool {
    if preceded_by_colon(out) {
        return true;
    }
    match preceded_by_attr_equals(out) {
        Some(prefix) => !attr_name_is_href(prefix),
        None => false,
    }
}

/// True when a bare colour keyword (`white`, `black`) about to be scanned is in a colour
/// position: preceded by `:` or by `="` (optionally with one space). Unlike `#hex`, there
/// is no attribute name to exclude - `href="white"` is not a meaningful reference.
fn keyword_is_color_context(out: &str) -> bool {
    preceded_by_colon(out) || preceded_by_attr_equals(out).is_some()
}

fn push_substitution(out: &mut String, variable: &str, alpha: f64) {
    if alpha >= 1.0 {
        out.push_str(&format!("var(--{variable})"));
    } else {
        out.push_str(&format!(
            "color-mix(in srgb, var(--{variable}) {}%, transparent)",
            format_alpha_percent(alpha)
        ));
    }
}

fn format_alpha_percent(alpha: f64) -> String {
    let pct = (alpha * 100.0 * 100.0).round() / 100.0;
    let mut s = format!("{pct:.2}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

/// Parses `#rgb`, `#rrggbb` or `#rrggbbaa` (case-insensitive) at the start of `rest`.
/// Returns the number of bytes consumed and the decoded colour.
fn parse_hex_color(rest: &str) -> Option<(usize, (u8, u8, u8, f64))> {
    let bytes = rest.as_bytes();
    if bytes.first() != Some(&b'#') {
        return None;
    }
    let mut n = 0usize;
    while n < 8
        && bytes
            .get(1 + n)
            .is_some_and(|b| (*b as char).is_ascii_hexdigit())
    {
        n += 1;
    }
    // A longer run than 8 hex digits is not a valid colour token; leave it alone.
    if bytes
        .get(1 + n)
        .is_some_and(|b| (*b as char).is_ascii_hexdigit())
    {
        return None;
    }
    let hex = rest.get(1..1 + n)?;
    match n {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            Some((4, (r * 17, g * 17, b * 17, 1.0)))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((7, (r, g, b, 1.0)))
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some((9, (r, g, b, f64::from(a) / 255.0)))
        }
        _ => None,
    }
}

fn starts_with_ci(rest: &str, needle: &str) -> bool {
    match rest.get(..needle.len()) {
        Some(s) => s.eq_ignore_ascii_case(needle),
        None => false,
    }
}

/// Parses `rgb(r, g, b)`, `rgb(r g b)`, `rgba(r, g, b, a)` and `rgb(r g b / a)`
/// (case-insensitive) at the start of `rest`.
fn parse_rgb_like(rest: &str) -> Option<(usize, (u8, u8, u8, f64))> {
    let prefix_len = if starts_with_ci(rest, "rgba(") {
        5
    } else if starts_with_ci(rest, "rgb(") {
        4
    } else {
        return None;
    };
    let after = &rest[prefix_len..];
    let close = after.find(')')?;
    let inner = &after[..close];
    let total_len = prefix_len + close + 1;
    let (nums, alpha) = parse_rgb_inner(inner)?;
    if nums.len() != 3 {
        return None;
    }
    Some((total_len, (nums[0], nums[1], nums[2], alpha.unwrap_or(1.0))))
}

fn parse_rgb_inner(s: &str) -> Option<(Vec<u8>, Option<f64>)> {
    let (main, alpha_part) = match s.find('/') {
        Some(idx) => (&s[..idx], Some(s[idx + 1..].trim())),
        None => (s, None),
    };
    let mut tokens: Vec<&str> = main
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();

    let mut alpha_from_comma = None;
    if tokens.len() == 4 {
        let a_tok = tokens.pop().expect("length checked above");
        alpha_from_comma = Some(parse_alpha_token(a_tok)?);
    }
    if tokens.len() != 3 {
        return None;
    }

    let mut nums = Vec::with_capacity(3);
    for t in tokens {
        nums.push(parse_channel_token(t)?);
    }

    let alpha = match alpha_part {
        Some(a) => Some(parse_alpha_token(a)?),
        None => alpha_from_comma,
    };
    Some((nums, alpha))
}

fn parse_channel_token(t: &str) -> Option<u8> {
    t.parse::<u16>().ok().map(|v| v.min(255) as u8)
}

fn parse_alpha_token(t: &str) -> Option<f64> {
    let v: f64 = t.parse().ok()?;
    if (0.0..=1.0).contains(&v) {
        Some(v)
    } else {
        None
    }
}

/// Matches the bare colour keyword `kw` (`white`/`black`) at byte offset `pos` in `svg`,
/// case-insensitively. Deliberately does **not** rely on word boundaries alone: a diagram
/// node labelled `black` renders as `>black<` in a `<text>` element, and a bare
/// word-boundary match would rewrite the reader's own label text. Requiring the positive
/// `:`/`="` colour context (via `out`, which mirrors `svg` up to `pos`) confines the match
/// to `stroke="black"` and `background-color: white`, the only forms the census observed.
/// Still rejects a longer identifier immediately after (`white-space`, `whitesmoke`).
fn match_color_keyword(svg: &str, out: &str, pos: usize, kw: &str) -> Option<usize> {
    let rest = svg.get(pos..)?;
    let candidate = rest.get(..kw.len())?;
    if !candidate.eq_ignore_ascii_case(kw) {
        return None;
    }
    let after_ok = rest[kw.len()..]
        .chars()
        .next()
        .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    if after_ok && keyword_is_color_context(out) {
        Some(kw.len())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Page, Site, SourceKind, SourceRel, TargetPath};

    fn page() -> Page {
        Page {
            source: SourceRel::new("page.md"),
            target: TargetPath::new("page.html"),
            title: "Page".to_string(),
            times: Default::default(),
            is_root: false,
        }
    }

    /// Builds a `Site` whose lookup map holds exactly `entries`, keyed the same way
    /// `links::resolve` folds them, so a heading's buffered links/images have something
    /// to resolve against.
    fn site_with(entries: &[(&str, SourceKind)], pages: Vec<Page>) -> Site {
        let mut by_source = HashMap::new();
        for (path, kind) in entries {
            by_source.insert(links::fold(path), kind.clone());
        }
        Site {
            pages,
            by_source,
            nav: Vec::new(),
            todo: None,
        }
    }

    #[test]
    fn filename_subtitle_follows_title_with_inline_markup_and_escaped_basename() {
        let diagrams = Diagrams::new();
        let site = Site::default();
        let page = Page {
            source: SourceRel::new("docs/Notes & <draft>.MD"),
            ..page()
        };
        let rendered = render_page(
            &page,
            "# A *small* `title` [link](https://example.com)\n\nBody.\n",
            &site,
            &diagrams,
        );

        assert_eq!(
            rendered.html,
            "<h1 id=\"a-small-title-link\">A <em>small</em> <code>title</code> <a href=\"https://example.com\">link</a></h1>\n\
             <div class=\"ohm-page-filename\">Notes &amp; &lt;draft&gt;.MD</div>\n\
             <p>Body.</p>\n"
        );
        assert_eq!(rendered.outline.len(), 1);
        assert_eq!(rendered.outline[0].id, "a-small-title-link");
        assert_eq!(
            rendered.outline[0].text,
            "A <em>small</em> <code>title</code> link"
        );
        assert!(rendered.warnings.is_empty());
    }

    #[test]
    fn filename_subtitle_skips_nested_h1_and_only_uses_first_top_level_h1() {
        let diagrams = Diagrams::new();
        let site = Site::default();
        let rendered = render_page(
            &page(),
            "> # Quoted\n\n\
             - # Listed\n\n\
             - [ ] Loose task\n\n\
               Another paragraph.\n\n\
             [^note]:\n    # Footnote\n\n\
             Reference[^note].\n\n\
             ```text\n# Code\n```\n\n\
             # Main\n\n\
             # Later\n",
            &site,
            &diagrams,
        );

        assert!(rendered.html.contains(
            "<h1 id=\"main\">Main</h1>\n<div class=\"ohm-page-filename\">page.md</div>\n"
        ));
        assert_eq!(rendered.html.matches("ohm-page-filename").count(), 1);
        for id in ["quoted", "listed", "footnote", "main", "later"] {
            assert!(rendered.outline.iter().any(|entry| entry.id == id));
        }
    }

    #[test]
    fn filename_subtitle_falls_back_before_body_once_across_parts() {
        let diagrams = Diagrams::new();
        let site = Site::default();
        let mut ids = PageIds::default();
        let first = render_part(
            &page(),
            "<h1>Raw title</h1>\n\nBody.\n",
            &site,
            &diagrams,
            &mut ids,
        );
        let second = render_part(
            &page(),
            "# Collected Tasks\n\n- [ ] Later task\n",
            &site,
            &diagrams,
            &mut ids,
        );

        assert_eq!(
            first.html,
            "<div class=\"ohm-page-filename\">page.md</div>\n\
             <h1>Raw title</h1>\n<p>Body.</p>\n"
        );
        assert!(first.outline.is_empty());
        assert!(!second.html.contains("ohm-page-filename"));
        assert_eq!(second.outline[0].id, "collected-tasks");
    }

    #[test]
    fn r6_1_heading_link_to_another_page_is_rewritten() {
        let diagrams = Diagrams::new();
        let pages = vec![Page {
            source: SourceRel::new("guide.md"),
            target: TargetPath::new("guide.html"),
            title: String::new(),
            times: Default::default(),
            is_root: false,
        }];
        let site = site_with(&[("guide.md", SourceKind::Page(0))], pages);
        let rendered = render_page(&page(), "# [Guide](guide.md)\n", &site, &diagrams);
        assert!(rendered.warnings.is_empty());
        assert!(rendered.html.contains("<a href=\"guide.html\">Guide</a>"));
    }

    #[test]
    fn r6_2_heading_link_to_root_file_resolves_to_index() {
        let diagrams = Diagrams::new();
        let pages = vec![Page {
            source: SourceRel::new("README.md"),
            target: TargetPath::new("index.html"),
            title: String::new(),
            times: Default::default(),
            is_root: true,
        }];
        let site = site_with(&[("README.md", SourceKind::Page(0))], pages);
        let rendered = render_page(&page(), "# [Home](README.md)\n", &site, &diagrams);
        assert!(rendered.warnings.is_empty());
        assert!(rendered.html.contains("<a href=\"index.html\">Home</a>"));
    }

    #[test]
    fn r5_1_heading_image_registers_asset_and_rewrites_src() {
        let diagrams = Diagrams::new();
        let site = site_with(
            &[("shot.png", SourceKind::Asset(SourceRel::new("shot.png")))],
            Vec::new(),
        );
        let rendered = render_page(&page(), "# ![shot](shot.png)\n", &site, &diagrams);
        assert!(rendered.warnings.is_empty());
        assert!(rendered.assets.contains(&SourceRel::new("shot.png")));
        assert!(rendered.html.contains("src=\"shot.png\""));
    }

    #[test]
    fn r5_2_unresolvable_heading_reference_warns_once_and_is_unchanged() {
        let diagrams = Diagrams::new();
        let site = Site::default();
        let rendered = render_page(&page(), "# [Guide](missing.md)\n", &site, &diagrams);
        assert_eq!(rendered.warnings.len(), 1);
        assert!(rendered.html.contains("href=\"missing.md\""));
    }

    #[test]
    fn r8_3_outline_entry_for_heading_link_has_no_nested_anchor() {
        let diagrams = Diagrams::new();
        let pages = vec![Page {
            source: SourceRel::new("guide.md"),
            target: TargetPath::new("guide.html"),
            title: String::new(),
            times: Default::default(),
            is_root: false,
        }];
        let site = site_with(&[("guide.md", SourceKind::Page(0))], pages);
        let rendered = render_page(&page(), "# [Guide](guide.md)\n", &site, &diagrams);
        assert_eq!(rendered.outline.len(), 1);
        assert!(rendered.outline[0].text.contains("Guide"));
        assert!(!rendered.outline[0].text.contains("<a"));
        // The heading itself keeps its anchor; only the outline's copy drops it.
        assert!(rendered.html.contains("<a href=\"guide.html\">Guide</a>"));
    }

    #[test]
    fn r6_8_duplicate_headings_get_numeric_suffix() {
        let diagrams = Diagrams::new();
        let site = Site::default();
        let rendered = render_page(&page(), "## Usage\n\n## Usage\n", &site, &diagrams);
        assert_eq!(rendered.outline.len(), 2);
        assert_eq!(rendered.outline[0].id, "usage");
        assert_eq!(rendered.outline[1].id, "usage-1");
    }

    #[test]
    fn r6_8_anchor_is_stable() {
        let diagrams = Diagrams::new();
        let site = Site::default();
        let a = render_page(&page(), "## Usage\n", &site, &diagrams);
        let b = render_page(&page(), "## Usage\n", &site, &diagrams);
        assert_eq!(a.outline[0].id, b.outline[0].id);
        assert_eq!(a.outline[0].id, "usage");
    }

    #[test]
    fn r7_2_table_alignment() {
        let diagrams = Diagrams::new();
        let site = Site::default();
        let md = "| L | C | R |\n|:--|:-:|--:|\n| a | b | c |\n";
        let rendered = render_page(&page(), md, &site, &diagrams);
        assert!(rendered.html.contains("<table>"));
        assert!(rendered.html.contains("text-align: left"));
        assert!(rendered.html.contains("text-align: center"));
        assert!(rendered.html.contains("text-align: right"));
    }

    #[test]
    fn r7_3_fence_class_and_escaping() {
        let diagrams = Diagrams::new();
        let site = Site::default();
        let md = "```rust\n<script>\n```\n";
        let rendered = render_page(&page(), md, &site, &diagrams);
        assert!(
            rendered
                .html
                .contains("<pre><code class=\"language-rust\">")
        );
        assert!(rendered.html.contains("&lt;script&gt;"));
        assert!(!rendered.html.contains("<script>"));
    }

    #[test]
    fn r7_3_bare_fence_has_no_language_class() {
        let diagrams = Diagrams::new();
        let site = Site::default();
        let md = "```\nplain\n```\n";
        let rendered = render_page(&page(), md, &site, &diagrams);
        assert!(rendered.html.contains("<pre><code>plain"));
        assert!(!rendered.html.contains("<pre><code class="));
    }

    #[test]
    fn r7_5_raw_html_passes_through() {
        let diagrams = Diagrams::new();
        let site = Site::default();
        let md = "<div>\n\nUse <kbd>Ctrl</kbd>.\n\n</div>\n";
        let rendered = render_page(&page(), md, &site, &diagrams);
        assert!(rendered.html.contains("<div>"));
        assert!(rendered.html.contains("<kbd>Ctrl</kbd>"));
    }

    #[test]
    fn r7_4_mermaid_renders_to_svg() {
        let diagrams = Diagrams::new();
        let site = Site::default();
        let md = "```mermaid\nflowchart TD\nA --> B\n```\n";
        let rendered = render_page(&page(), md, &site, &diagrams);
        assert!(rendered.warnings.is_empty());
        assert!(rendered.html.contains("<figure class=\"ohm-diagram\">"));
        assert!(rendered.html.contains("<svg"));
        assert!(rendered.html.contains("flowchart TD"));
    }

    #[test]
    fn r7_7_broken_mermaid_falls_back() {
        let diagrams = Diagrams::new();
        let site = Site::default();
        let md = "```mermaid\nthis is not a valid diagram {{{\n```\n";
        let rendered = render_page(&page(), md, &site, &diagrams);
        assert_eq!(rendered.warnings.len(), 1);
        assert!(
            rendered
                .html
                .contains("<pre><code class=\"language-mermaid\">")
        );
        assert!(!rendered.html.contains("<svg"));
    }

    #[test]
    fn r7_8_task_lists_render_read_only_checkboxes() {
        let diagrams = Diagrams::new();
        let site = Site::default();
        let md = "- [x] done\n- [ ] open\n  - [x] nested\n- plain\n\n1. [ ] ordered\n";
        let rendered = render_page(&page(), md, &site, &diagrams);
        assert!(
            rendered
                .html
                .contains(r#"<input disabled="" type="checkbox" checked=""/>"#)
        );
        assert!(
            rendered
                .html
                .contains(r#"<input disabled="" type="checkbox"/>"#)
        );
        // Four markers: the three unordered items and the ordered one. Each is
        // disabled, so no reader can drive a page out of step with its source.
        assert_eq!(rendered.html.matches(r#"type="checkbox""#).count(), 4);
        assert_eq!(rendered.html.matches("disabled").count(), 4);
        // Each task item carries the class the stylesheet drops the marker on.
        assert_eq!(rendered.html.matches(r#"<li class="ohm-task">"#).count(), 4);
        // A plain item in the same list is untouched, and a task item inside an
        // ordered list still produces the ordered list.
        assert!(rendered.html.contains("<li>plain</li>"));
        assert!(rendered.html.contains("<ol>"));
    }

    #[test]
    fn r7_8_loose_task_item_is_tagged_through_its_paragraph() {
        let diagrams = Diagrams::new();
        let site = Site::default();
        // Blank lines make the list loose, so `pulldown-cmark` wraps each item's
        // content in a `<p>` and the marker becomes a grandchild of the `<li>`.
        let md = "- [x] done\n\n- [ ] open\n";
        let rendered = render_page(&page(), md, &site, &diagrams);
        assert_eq!(rendered.html.matches(r#"<li class="ohm-task">"#).count(), 2);
        assert!(
            rendered
                .html
                .contains(r#"<p><input disabled="" type="checkbox" checked=""/>"#)
        );
    }

    #[test]
    fn task_tags_render_on_checked_and_open_items_without_todo_collection() {
        let rendered = render_page(
            &page(),
            "- [x] done {Travel}\n- [ ] open {buy}\n- [ ] tagless\n- plain {literal}\n",
            &Site::default(),
            &Diagrams::new(),
        );
        assert_eq!(rendered.html.matches("data-ohm-tags=").count(), 2);
        assert_eq!(rendered.html.matches("class=\"ohm-task-tags\"").count(), 2);
        assert_eq!(rendered.html.matches(r#"<li class="ohm-task">"#).count(), 1);
        assert!(
            rendered
                .html
                .contains(r#"<input disabled="" type="checkbox" checked=""/>"#),
            "{}",
            rendered.html
        );
        assert!(
            rendered
                .html
                .contains(r#"class="ohm-task-tag">Travel</span>"#)
        );
        assert!(rendered.html.contains("plain {literal}"));
        assert!(!rendered.html.contains("{Travel}"));
        assert!(!rendered.html.contains("{buy}"));
    }

    #[test]
    fn r8_2_diagram_colours_become_css_variables() {
        let diagrams = Diagrams::new();
        let site = Site::default();
        let md = "```mermaid\nflowchart TD\nA --> B\n```\n";
        let rendered = render_page(&page(), md, &site, &diagrams);
        assert!(rendered.html.contains("var(--ohm-dg-"));
        let lower = rendered.html.to_lowercase();
        for i in 0..34 {
            let (r, g, b) = sentinel_channels(i);
            let hex = format!("#{r:02x}{g:02x}{b:02x}");
            assert!(!lower.contains(&hex), "sentinel {hex} leaked into output");
        }
    }

    /// The width and height of an SVG's `viewBox`.
    fn view_box_size(svg: &str) -> (f64, f64) {
        let start = svg.find("viewBox=\"").expect("svg has a viewBox") + "viewBox=\"".len();
        let end = start + svg[start..].find('"').expect("viewBox is closed");
        let values: Vec<f64> = svg[start..end]
            .split_whitespace()
            .map(|value| value.parse().expect("viewBox value is a number"))
            .collect();
        assert_eq!(values.len(), 4, "viewBox has four values");
        (values[2], values[3])
    }

    #[test]
    fn diagram_text_uses_the_site_font() {
        let diagrams = Diagrams::new();
        let svg = diagrams
            .render("flowchart TD\nA --> B", "font")
            .expect("flowchart renders");
        assert!(svg.contains("var(--ohm-font-sans)"));
        assert!(svg.contains("14px"));
        assert!(!svg.to_lowercase().contains("trebuchet"));
    }

    #[test]
    fn diagrams_are_more_compact_than_merman_defaults() {
        let source = "flowchart TD\nA[read the sources] -->|every page| B[render the markdown]\nB --> C[write the site]\nB --> D[copy the assets]\n";
        let themed = Diagrams::new()
            .render(source, "compact")
            .expect("flowchart renders");
        let request = RenderRequest::svg(source, OperationControl::new(), SvgRequest::default());
        let Ok(RenderOutput::Svg(Some(default))) = Renderer::new().render(request) else {
            panic!("default renderer renders the flowchart");
        };
        let (width, height) = view_box_size(&themed);
        let (default_width, default_height) = view_box_size(default.svg());
        assert!(
            width < default_width,
            "{width} is not narrower than {default_width}"
        );
        assert!(
            height < default_height,
            "{height} is not shorter than {default_height}"
        );
    }

    /// The `x` and content of every `<text>` in a gantt chart that carries `class`.
    fn gantt_texts<'a>(svg: &'a str, class: &str) -> Vec<(f64, &'a str)> {
        let open = format!("<text class=\"{class}\" x=\"");
        svg.match_indices(&open)
            .map(|(at, _)| {
                let rest = &svg[at + open.len()..];
                let x = rest[..rest.find('"').expect("x is closed")]
                    .parse()
                    .expect("x is a number");
                let content = &rest[rest.find('>').expect("tag is closed") + 1..];
                (x, &content[..content.find('<').expect("text is closed")])
            })
            .collect()
    }

    /// Whether `svg` holds a `#rgb` or `#rrggbb` colour literal.
    fn has_hex_colour(svg: &str) -> bool {
        svg.match_indices('#').any(|(at, _)| {
            let digits = svg[at + 1..]
                .bytes()
                .take_while(u8::is_ascii_hexdigit)
                .count();
            digits == 3 || digits == 6
        })
    }

    #[test]
    fn gantt_charts_are_drawn_by_the_site_renderer() {
        let source = "gantt\ntitle Plan\ndateFormat YYYY-MM-DD\nexcludes weekends\nLoose :l1, 2026-01-02, 2d\nsection Design\nRequirements :done, a1, 2026-01-05, 10d\nReview :active, after a1, 5d\nFreeze :vert, v1, 2026-01-12, 0d\nsection Build\nRenderer :crit, 2026-01-20, 14d\nTheme :crit, active, 2026-01-22, 3d\nArchive :crit, done, 2026-01-26, 2d\nShip :milestone, 2026-02-12, 0d\n";
        let svg = Diagrams::new()
            .render(source, "gantt")
            .expect("gantt chart renders");
        assert!(svg.starts_with("<svg id=\"gantt\" class=\"ohm-gantt\""));

        // All the writing sits in the column left of the vertical axis.
        let axis = "<line class=\"ohm-gantt-axis\" x1=\"";
        let axis = &svg[svg.find(axis).expect("chart has an axis") + axis.len()..];
        let axis_x: f64 = axis[..axis.find('"').expect("x1 is closed")]
            .parse()
            .expect("x1 is a number");
        let labels = |class: &str| -> Vec<&str> {
            gantt_texts(&svg, class)
                .into_iter()
                .map(|(_, label)| label)
                .collect()
        };
        assert_eq!(labels("ohm-gantt-title"), ["Plan"]);
        assert_eq!(labels("ohm-gantt-section"), ["Design", "Build"]);
        assert_eq!(
            labels("ohm-gantt-task"),
            [
                "Loose",
                "Requirements",
                "Review",
                "Renderer",
                "Theme",
                "Archive",
                "Ship"
            ]
        );
        for class in ["ohm-gantt-section", "ohm-gantt-task"] {
            for (x, label) in gantt_texts(&svg, class) {
                assert!(x < axis_x, "{label} at {x} is not left of the axis");
            }
        }

        // One bar per task, a diamond for the milestone; the marker is a line with its
        // own label and takes no row.
        assert_eq!(svg.matches("class=\"ohm-gantt-bar").count(), 7);
        assert_eq!(svg.matches("<path class=\"ohm-gantt-bar\"").count(), 1);
        assert_eq!(svg.matches("<line class=\"ohm-gantt-marker\"").count(), 1);
        assert_eq!(labels("ohm-gantt-marker-label"), ["Freeze"]);
        assert_eq!(svg.matches("ohm-gantt-bar--done").count(), 2);
        assert_eq!(svg.matches("ohm-gantt-bar--active").count(), 2);
        assert_eq!(svg.matches("ohm-gantt-bar--crit").count(), 3);
        assert!(svg.contains("class=\"ohm-gantt-bar ohm-gantt-bar--done ohm-gantt-bar--crit\""));
        assert!(svg.contains("class=\"ohm-gantt-exclude\""));

        // Colour and outline come from the stylesheet alone.
        assert!(!svg.contains("stroke") && !svg.contains("fill"));
        assert!(!svg.contains("<style") && !has_hex_colour(&svg));
    }

    #[test]
    fn gantt_tick_labels_read_month_and_day_unless_the_chart_sets_a_format() {
        let diagrams = Diagrams::new();
        let source =
            "gantt\ndateFormat YYYY-MM-DD\nsection Design\nRequirements :2026-01-05, 10d\n";
        let svg = diagrams
            .render(source, "ticks")
            .expect("gantt chart renders");
        let ticks = gantt_texts(&svg, "ohm-gantt-tick");
        assert!(!ticks.is_empty());
        for (_, label) in ticks {
            let (month, day) = label.split_once(' ').expect("month and day");
            assert!(
                month.len() == 3 && month.bytes().all(|b| b.is_ascii_alphabetic()),
                "{label}"
            );
            assert!(
                day.len() == 2 && day.bytes().all(|b| b.is_ascii_digit()),
                "{label}"
            );
        }

        let explicit = source.replace("section", "axisFormat %Y-%m-%d\nsection");
        let svg = diagrams
            .render(&explicit, "ticks")
            .expect("gantt chart renders");
        let ticks = gantt_texts(&svg, "ohm-gantt-tick");
        assert!(!ticks.is_empty());
        for (_, label) in ticks {
            assert!(label.starts_with("2026-01-"), "{label}");
        }
    }

    #[test]
    fn gantt_labels_are_escaped() {
        let source = "gantt\ndateFormat YYYY-MM-DD\nsection R&D\nQ&A < 5 :2026-01-05, 3d\n";
        let svg = Diagrams::new()
            .render(source, "escape")
            .expect("gantt chart renders");
        assert!(svg.contains(">R&amp;D</text>"));
        assert!(svg.contains(">Q&amp;A &lt; 5</text>"));
    }

    #[test]
    fn gantt_accessible_title_and_description_are_kept() {
        let source = "gantt\naccTitle: Release plan\naccDescr: Dates for the release\ndateFormat YYYY-MM-DD\nsection Design\nRequirements :2026-01-05, 10d\n";
        let svg = Diagrams::new()
            .render(source, "acc")
            .expect("gantt chart renders");
        assert!(svg.contains(" aria-labelledby=\"chart-title-acc\""));
        assert!(svg.contains(" aria-describedby=\"chart-desc-acc\""));
        assert!(svg.contains("<title id=\"chart-title-acc\">Release plan</title>"));
        assert!(svg.contains("<desc id=\"chart-desc-acc\">Dates for the release</desc>"));
    }

    #[test]
    fn gantt_chart_without_tasks_falls_back_to_merman() {
        let diagrams = Diagrams::new();
        let source = "gantt\ntitle Empty\ndateFormat YYYY-MM-DD\n";
        assert!(diagrams.render_gantt(source, "empty").is_none());
        let svg = diagrams
            .render(source, "empty")
            .expect("merman still renders it");
        assert!(!svg.contains("ohm-gantt"));
    }

    #[test]
    fn only_gantt_charts_leave_the_merman_renderer() {
        let source = "flowchart TD\nA --> B";
        let diagrams = Diagrams::new();
        assert!(diagrams.render_gantt(source, "flow").is_none());
        let svg = diagrams.render(source, "flow").expect("flowchart renders");
        assert!(!svg.contains("ohm-gantt"));
        assert!(svg.contains("var(--ohm-dg-"));

        for gantt in [
            "gantt\n",
            "  gantt\ntitle Plan\n",
            "\n%% a comment\n%%{init: {\"gantt\": {\"barHeight\": 20}}}%%\ngantt\n",
            "%%{init: {\n  \"gantt\": {\"barHeight\": 20}\n}}%%\ngantt\n",
            "%%{init: {\"theme\": \"dark\"}}%% gantt\n",
            "---\ntitle: Plan\n---\ngantt\n",
        ] {
            assert!(is_gantt_source(gantt), "{gantt:?}");
        }
        for other in [
            "",
            source,
            "flowchart TD\ngantt --> B",
            "ganttish\n",
            "---\ntitle: gantt\n---\nflowchart TD\nA --> B",
            "---\ngantt\n",
        ] {
            assert!(!is_gantt_source(other), "{other:?}");
        }
    }
}
