//! The page shell: wraps a rendered page body in the site's HTML document,
//! sidebar navigation (R4) and per-page outline (R8.3). Also bundles the
//! minimal base CSS and JS, embedded at compile time so the built site needs
//! no network access at runtime (R8.7).

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use camino::Utf8Path;

use crate::cli::DEFAULT_MAX_GROUP_SIZE;
use crate::links;
use crate::model::{NavNode, OutlineEntry, Page, PageTimes, Site, TargetPath, escape_html};

/// The site's minimal base stylesheet, embedded at compile time (R8.7:
/// nothing is fetched at runtime).
pub const STYLE_CSS: &str = include_str!("assets/ohimark.css");
/// The optional default preset, embedded at compile time.
pub const DEFAULT_THEME_CSS: &str = include_str!("../themes/default.css");
/// The optional bookish preset, embedded at compile time.
pub const BOOKISH_THEME_CSS: &str = include_str!("../themes/bookish.css");
/// The site script, embedded at compile time.
pub const APP_JS: &str = include_str!("assets/ohimark.js");

/// Output-relative path the stylesheet is written to.
pub const STYLE_PATH: &str = "assets/ohimark.css";
/// Output-relative path the selected built-in theme is written to.
pub const THEME_STYLE_PATH: &str = "assets/theme.css";
/// Output-relative path the optional custom stylesheet is copied to.
pub const CUSTOM_STYLE_PATH: &str = "assets/custom.css";
/// Output-relative path the script is written to.
pub const SCRIPT_PATH: &str = "assets/ohimark.js";

const PANEL_ICONS: &str = "<svg class=\"ohm-icon ohm-icon-collapse\" viewBox=\"0 0 24 24\" aria-hidden=\"true\" focusable=\"false\"><rect x=\"3\" y=\"3\" width=\"18\" height=\"18\" rx=\"2\"/><path d=\"M9 3v18 M16 9l-3 3 3 3\"/></svg><svg class=\"ohm-icon ohm-icon-expand\" viewBox=\"0 0 24 24\" aria-hidden=\"true\" focusable=\"false\"><rect x=\"3\" y=\"3\" width=\"18\" height=\"18\" rx=\"2\"/><path d=\"M9 3v18 M14 9l3 3-3 3\"/></svg>";

/// Render the sidebar recursively. Hrefs are computed relative to `dir`, the
/// directory of the page currently being rendered, so the same call from two
/// different pages necessarily yields different hrefs by depth (R11.1) while
/// the structure, order and labels stay invariant (R4.7) — only the
/// current-page marker moves.
fn render_nav_list(
    nodes: &[NavNode],
    page: &Page,
    dir: &Utf8Path,
    parent_path: &str,
    times_by_target: &HashMap<&TargetPath, &PageTimes>,
) -> (String, PageTimes) {
    let mut out = String::new();
    let mut times = PageTimes::default();
    for node in nodes {
        let node_times = render_nav_node(node, page, dir, parent_path, times_by_target, &mut out);
        merge_times(&mut times, &node_times);
    }
    (out, times)
}

/// The class suffix and `aria-current` attribute for a nav link, present only
/// when it points at the page currently being rendered.
fn current_marker(is_current: bool) -> (&'static str, &'static str) {
    if is_current {
        (" ohm-nav-link--current", " aria-current=\"page\"")
    } else {
        ("", "")
    }
}

fn render_nav_link(
    title: &str,
    target: &crate::model::TargetPath,
    page: &Page,
    dir: &Utf8Path,
) -> String {
    let href = links::relative_href(dir, target.as_path());
    let title = escape_html(title);
    let (current_class, current_attr) = current_marker(*target == page.target);
    format!("<a class=\"ohm-nav-link{current_class}\"{current_attr} href=\"{href}\">{title}</a>")
}

fn render_nav_node(
    node: &NavNode,
    page: &Page,
    dir: &Utf8Path,
    parent_path: &str,
    times_by_target: &HashMap<&TargetPath, &PageTimes>,
    out: &mut String,
) -> PageTimes {
    match node {
        NavNode::Page { title, target } => {
            let link = render_nav_link(title, target, page, dir);
            let times = times_by_target
                .get(target)
                .copied()
                .cloned()
                .unwrap_or_default();
            let attrs = page_time_attrs(Some(&times));
            out.push_str(&format!("<li class=\"ohm-nav-item\"{attrs}>{link}</li>"));
            times
        }
        NavNode::Group {
            label,
            link,
            children,
        } => {
            // Raw directory components cannot contain '/', so their joined
            // path identifies each group independently of page depth or nav order.
            let group_path = if parent_path.is_empty() {
                label.clone()
            } else {
                format!("{parent_path}/{label}")
            };
            let (children_html, mut times) =
                render_nav_list(children, page, dir, &group_path, times_by_target);
            if let Some(landing_times) =
                link.as_ref().and_then(|target| times_by_target.get(target))
            {
                merge_times(&mut times, landing_times);
            }
            let attrs = page_time_attrs(Some(&times));
            let group_attr = escape_html(&group_path);
            let label = escape_html(label);
            match link {
                // R4.2: link the same-name page first, falling back to README.
                Some(target) => {
                    let href = links::relative_href(dir, target.as_path());
                    let (current_class, current_attr) = current_marker(*target == page.target);
                    out.push_str(&format!(
                        "<li class=\"ohm-nav-group\"{attrs}><details class=\"ohm-nav-details\" data-ohm-nav-group=\"{group_attr}\" open><summary class=\"ohm-nav-summary\"><a class=\"ohm-nav-link ohm-nav-label{current_class}\"{current_attr} href=\"{href}\">{label}</a></summary><ul class=\"ohm-nav-list\">{children_html}</ul></details></li>"
                    ));
                }
                // R4.3: otherwise the node is a plain label with no link.
                None => {
                    out.push_str(&format!(
                        "<li class=\"ohm-nav-group\"{attrs}><details class=\"ohm-nav-details\" data-ohm-nav-group=\"{group_attr}\" open><summary class=\"ohm-nav-summary\"><span class=\"ohm-nav-label\">{label}</span></summary><ul class=\"ohm-nav-list\">{children_html}</ul></details></li>"
                    ));
                }
            }
            times
        }
    }
}

/// Combine source-page dates while the navigation is rendered bottom-up.
fn merge_times(aggregate: &mut PageTimes, times: &PageTimes) {
    aggregate.created = aggregate.created.into_iter().chain(times.created).min();
    aggregate.modified = aggregate.modified.into_iter().chain(times.modified).max();
}

/// Epoch milliseconds that JavaScript can compare without losing integer precision.
fn timestamp_millis(time: SystemTime) -> Option<i128> {
    let millis = match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i128::try_from(duration.as_millis()).ok()?,
        Err(error) => -i128::try_from(error.duration().as_millis()).ok()?,
    };
    const MAX_SAFE_INTEGER: i128 = 9_007_199_254_740_991;
    (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER)
        .contains(&millis)
        .then_some(millis)
}

pub(crate) fn page_time_attrs(times: Option<&PageTimes>) -> String {
    let mut attrs = String::new();
    if let Some(times) = times {
        for (name, time) in [("created", times.created), ("updated", times.modified)] {
            if let Some(millis) = time.and_then(timestamp_millis) {
                attrs.push_str(&format!(" data-ohm-{name}=\"{millis}\""));
            }
        }
    }
    attrs
}

/// Render the on-this-page outline. R8.3: a page with no headings shows no
/// outline at all, so an empty `outline` yields an empty string rather than
/// an empty `<aside>`.
fn render_outline(outline: &[OutlineEntry]) -> String {
    if outline.is_empty() {
        return String::new();
    }

    let mut items = String::new();
    for entry in outline {
        let level = entry.level;
        let id = &entry.id;
        // `text` is already rendered inline HTML; emit it verbatim, do not
        // escape it again.
        let text = &entry.text;
        items.push_str(&format!(
            "<li class=\"ohm-outline-item ohm-outline-item--h{level}\"><a href=\"#{id}\">{text}</a></li>"
        ));
    }

    format!(
        "<aside class=\"ohm-outline\" id=\"ohm-outline\">\n<div class=\"ohm-panel-header ohm-outline-header\"><p class=\"ohm-outline-title\">On this page</p><label class=\"ohm-panel-button ohm-outline-button\" for=\"ohm-outline-state\" aria-label=\"Toggle page outline\" aria-controls=\"ohm-outline\" title=\"Toggle page outline\">{PANEL_ICONS}</label></div>\n<nav class=\"ohm-outline-nav\" aria-label=\"On this page\">\n<ul class=\"ohm-outline-list\">{items}</ul>\n</nav>\n</aside>\n"
    )
}

/// Wrap one page's body in the complete HTML document.
///
/// R7.6: every page is a complete document declaring `lang`, a UTF-8
/// charset, a viewport, and a `<title>` derived from the page title. R11.1:
/// every internal href is relative, computed via `links::relative_to` from
/// this page's own target directory, never root-absolute. R11.2: links
/// always name an explicit `.html` file, never a directory, so the site
/// works over `file://`.
pub fn shell(page: &Page, site: &Site, body: &str, outline: &[OutlineEntry]) -> String {
    shell_with_stylesheets(
        page,
        site,
        body,
        outline,
        false,
        false,
        DEFAULT_MAX_GROUP_SIZE,
    )
}

/// Render the shell with the base, optional preset, then optional custom stylesheet.
pub(crate) fn shell_with_stylesheets(
    page: &Page,
    site: &Site,
    body: &str,
    outline: &[OutlineEntry],
    include_theme: bool,
    include_stylesheet: bool,
    max_group_size: u32,
) -> String {
    let dir = page.target.dir();
    let style_href = links::relative_to(dir, Utf8Path::new(STYLE_PATH));
    let script_href = links::relative_to(dir, Utf8Path::new(SCRIPT_PATH));
    let index_href = links::relative_to(dir, Utf8Path::new("index.html"));

    let site_title = escape_html(&site.root_page().title);
    // On the root page itself, emit just the site title, not "X · X".
    let title = if page.is_root {
        site_title.clone()
    } else {
        format!("{} · {}", escape_html(&page.title), site_title)
    };

    let (sidebar_root_link, sidebar_nav_nodes) = match site.nav.split_first() {
        Some((NavNode::Page { title, target }, remaining))
            if *target == site.root_page().target =>
        {
            (Some(render_nav_link(title, target, page, dir)), remaining)
        }
        _ => (None, site.nav.as_slice()),
    };
    let times_by_target = site
        .pages
        .iter()
        .map(|page| (&page.target, &page.times))
        .collect();
    let (nav, _) = render_nav_list(sidebar_nav_nodes, page, dir, "", &times_by_target);
    let outline_html = render_outline(outline);

    let mut out = String::new();
    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<title>");
    out.push_str(&title);
    out.push_str("</title>\n");
    out.push_str("<link rel=\"stylesheet\" href=\"");
    out.push_str(&style_href);
    out.push_str("\">\n");
    if include_theme {
        let theme_style_href = links::relative_to(dir, Utf8Path::new(THEME_STYLE_PATH));
        out.push_str("<link rel=\"stylesheet\" href=\"");
        out.push_str(&theme_style_href);
        out.push_str("\">\n");
    }
    if include_stylesheet {
        let custom_style_href = links::relative_to(dir, Utf8Path::new(CUSTOM_STYLE_PATH));
        out.push_str("<link rel=\"stylesheet\" href=\"");
        out.push_str(&custom_style_href);
        out.push_str("\">\n");
    }
    // The only inline script on the page: applies the stored theme before
    // first paint so the page does not flash (R8.1). Kept byte-for-byte.
    out.push_str(
        "<script>(function(){try{var t=sessionStorage.getItem(\"ohimark-theme\");if(t===\"dark\"||t===\"light\"){document.documentElement.setAttribute(\"data-theme\",t);}}catch(e){}})();</script>\n",
    );
    out.push_str("</head>\n<body>\n");
    out.push_str(
        "<input type=\"checkbox\" id=\"ohm-nav-state\" class=\"ohm-nav-state ohm-panel-state\" aria-label=\"Toggle navigation\" aria-controls=\"ohm-sidebar\">\n",
    );
    if !outline.is_empty() {
        out.push_str(
            "<input type=\"checkbox\" id=\"ohm-outline-state\" class=\"ohm-panel-state ohm-outline-state\" aria-label=\"Toggle page outline\" aria-controls=\"ohm-outline\">\n",
        );
    }
    out.push_str("<header class=\"ohm-topbar\">\n");
    out.push_str("<div class=\"ohm-topbar-lead\">\n");
    out.push_str("<a class=\"ohm-brand\" href=\"");
    out.push_str(&index_href);
    out.push_str("\">");
    out.push_str(&site_title);
    out.push_str("</a>\n");
    out.push_str("</div>\n");
    out.push_str("<div class=\"ohm-topbar-actions\">\n");
    // Hidden and empty by default: ohimark.js removes `hidden` and injects
    // its own icon once it runs, so with scripting off the button never
    // appears dead (R8.8).
    out.push_str(
        "<button type=\"button\" class=\"ohm-icon-button ohm-theme-button\" id=\"ohm-theme-button\" hidden aria-label=\"Switch colour theme\"></button>\n",
    );
    out.push_str("</div>\n");
    out.push_str("</header>\n");
    out.push_str("<div class=\"ohm-body\">\n");
    out.push_str("<aside class=\"ohm-sidebar\" id=\"ohm-sidebar\">\n");
    out.push_str(&format!(
        "<nav class=\"ohm-nav\" aria-label=\"Site navigation\" data-ohm-max-group-size=\"{max_group_size}\">\n"
    ));
    out.push_str("<div class=\"ohm-panel-header ohm-sidebar-header\">");
    if let Some(root_link) = sidebar_root_link {
        out.push_str(&root_link);
    }
    out.push_str("<label class=\"ohm-panel-button ohm-nav-button\" for=\"ohm-nav-state\" aria-label=\"Toggle navigation\" aria-controls=\"ohm-sidebar\" title=\"Toggle navigation\">");
    out.push_str(PANEL_ICONS);
    out.push_str("</label></div>\n");
    out.push_str("<ul class=\"ohm-nav-list\">");
    out.push_str(&nav);
    out.push_str("</ul>\n");
    out.push_str("</nav>\n");
    out.push_str("</aside>\n");
    out.push_str("<main class=\"ohm-main\">\n");
    let is_todo_page = site.todo.is_some_and(|todo_index| {
        site.pages
            .get(todo_index)
            .is_some_and(|todo_page| todo_page.target == page.target)
    });
    if is_todo_page {
        out.push_str("<article class=\"ohm-content\" data-ohm-todo>");
    } else {
        out.push_str("<article class=\"ohm-content\">");
    }
    out.push_str(body);
    out.push_str("</article>\n");
    out.push_str("</main>\n");
    out.push_str(&outline_html);
    out.push_str("</div>\n");
    out.push_str("<script src=\"");
    out.push_str(&script_href);
    out.push_str("\" defer></script>\n");
    out.push_str("</body>\n</html>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NavNode, Page, Site, SourceRel, TargetPath};

    fn page(source: &str, target: &str, title: &str, is_root: bool) -> Page {
        Page {
            source: SourceRel::new(source),
            target: TargetPath::new(target),
            title: title.to_string(),
            times: PageTimes::default(),
            is_root,
        }
    }

    fn site_with(pages: Vec<Page>, nav: Vec<NavNode>) -> Site {
        Site {
            pages,
            by_source: Default::default(),
            nav,
            todo: None,
        }
    }

    /// The contents of the `<nav class="ohm-nav" …>…</nav>` sidebar region.
    /// R4.7 constrains the sidebar, not the whole document, so tests must
    /// compare this slice rather than the full rendered page.
    fn extract_sidebar(html: &str) -> &str {
        let open = "<nav class=\"ohm-nav\" aria-label=\"Site navigation\"";
        let nav_start = html.find(open).expect("sidebar nav opening tag not found");
        let start = nav_start
            + html[nav_start..]
                .find('>')
                .expect("sidebar nav tag must close")
            + 1;
        let rest = &html[start..];
        let end = rest
            .find("</nav>")
            .expect("sidebar nav closing tag not found");
        &rest[..end]
    }

    /// Every `href="..."` value appearing anywhere in `html`, in order.
    fn all_hrefs(html: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut rest = html;
        while let Some(pos) = rest.find("href=\"") {
            rest = &rest[pos + "href=\"".len()..];
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

    #[test]
    fn sidebar_encodes_leaf_and_group_targets_relative_to_the_rendered_page() {
        let root = page("README.md", "index.html", "Home", true);
        let current = page("deep/note.md", "deep/note.html", "Note", false);
        let site = site_with(
            vec![root, current.clone()],
            vec![
                NavNode::Page {
                    title: "Raw <HTML>".into(),
                    target: TargetPath::new("café #?%\".html"),
                },
                NavNode::Group {
                    label: "Group".into(),
                    link: Some(TargetPath::new("group #?%\"/start here.html")),
                    children: Vec::new(),
                },
            ],
        );
        let html = shell(&current, &site, "", &[]);
        let sidebar = extract_sidebar(&html);
        assert!(sidebar.contains("Raw &lt;HTML&gt;"), "{sidebar}");
        assert_eq!(
            all_hrefs(sidebar),
            [
                "../caf%C3%A9%20%23%3F%25%22.html",
                "../group%20%23%3F%25%22/start%20here.html"
            ]
        );
    }

    #[test]
    fn sidebar_timestamp_millis_handle_signed_dates_and_javascript_precision() {
        use std::time::Duration;

        assert_eq!(timestamp_millis(UNIX_EPOCH), Some(0));
        assert_eq!(
            timestamp_millis(UNIX_EPOCH + Duration::from_micros(1_234_567)),
            Some(1234)
        );
        assert_eq!(
            timestamp_millis(UNIX_EPOCH - Duration::from_millis(1500)),
            Some(-1500)
        );
        for (millis, expected) in [
            (9_007_199_254_740_991, Some(9_007_199_254_740_991)),
            (9_007_199_254_740_992, None),
            (u64::MAX, None),
        ] {
            let duration = Duration::from_millis(millis);
            if let Some(time) = UNIX_EPOCH.checked_add(duration) {
                assert_eq!(timestamp_millis(time), expected);
            }
            if let Some(time) = UNIX_EPOCH.checked_sub(duration) {
                assert_eq!(timestamp_millis(time), expected.map(|value| -value));
            }
        }

        assert_eq!(page_time_attrs(None), "");
        assert_eq!(page_time_attrs(Some(&PageTimes::default())), "");
        assert_eq!(
            page_time_attrs(Some(&PageTimes {
                created: Some(UNIX_EPOCH),
                modified: Some(UNIX_EPOCH - Duration::from_millis(1500)),
            })),
            " data-ohm-created=\"0\" data-ohm-updated=\"-1500\""
        );
        if let Some(unsafe_time) =
            UNIX_EPOCH.checked_add(Duration::from_millis(9_007_199_254_740_992))
        {
            assert_eq!(
                page_time_attrs(Some(&PageTimes {
                    created: Some(unsafe_time),
                    modified: None,
                })),
                ""
            );
        }
    }

    #[test]
    fn sidebar_page_and_group_timestamps_are_identical_across_depths() {
        let mut root = page("README.md", "index.html", "Home", true);
        root.times.created = Some(UNIX_EPOCH);
        let mut guide = page("docs/guide.md", "docs/guide.html", "Guide", false);
        guide.times = PageTimes {
            created: Some(UNIX_EPOCH),
            modified: Some(UNIX_EPOCH - std::time::Duration::from_millis(1500)),
        };
        let mut landing = page("docs/docs.md", "docs/docs.html", "Docs", false);
        landing.times.created = Some(UNIX_EPOCH);
        let unknown = page("unknown.md", "unknown.html", "Unknown", false);
        let nav = vec![
            NavNode::Page {
                title: root.title.clone(),
                target: root.target.clone(),
            },
            NavNode::Page {
                title: unknown.title.clone(),
                target: unknown.target.clone(),
            },
            NavNode::Group {
                label: "docs".into(),
                link: Some(landing.target.clone()),
                children: vec![NavNode::Page {
                    title: guide.title.clone(),
                    target: guide.target.clone(),
                }],
            },
        ];
        let site = site_with(vec![root.clone(), guide.clone(), landing, unknown], nav);
        for page in [&root, &guide] {
            let html = shell(page, &site, "", &[]);
            let sidebar = extract_sidebar(&html);
            assert_eq!(sidebar.matches("data-ohm-created=\"0\"").count(), 2);
            assert_eq!(sidebar.matches("data-ohm-updated=\"-1500\"").count(), 2);
            assert!(sidebar.contains(
                "<li class=\"ohm-nav-item\" data-ohm-created=\"0\" data-ohm-updated=\"-1500\">"
            ));
            assert!(sidebar.contains("<li class=\"ohm-nav-item\"><a"));
            assert!(sidebar.contains(
                "<li class=\"ohm-nav-group\" data-ohm-created=\"0\" data-ohm-updated=\"-1500\"><details"
            ));
            let hrefs = all_hrefs(sidebar);
            assert_eq!(hrefs.len(), 4);
            let prefix = if page.is_root { "" } else { "../" };
            assert_eq!(hrefs[0], format!("{prefix}index.html"));
            assert_eq!(hrefs[1], format!("{prefix}unknown.html"));
            assert_eq!(
                hrefs[2],
                if page.is_root {
                    "docs/docs.html"
                } else {
                    "docs.html"
                }
            );
            assert_eq!(
                hrefs[3],
                if page.is_root {
                    "docs/guide.html"
                } else {
                    "guide.html"
                }
            );
        }
    }

    #[test]
    fn sidebar_groups_aggregate_all_descendant_and_landing_dates() {
        fn dated_page(name: &str, created: Option<i64>, modified: Option<i64>) -> Page {
            let time = |millis: i64| {
                let duration = std::time::Duration::from_millis(millis.unsigned_abs());
                if millis < 0 {
                    UNIX_EPOCH - duration
                } else {
                    UNIX_EPOCH + duration
                }
            };
            let mut page = page(&format!("{name}.md"), &format!("{name}.html"), name, false);
            page.times = PageTimes {
                created: created.map(time),
                modified: modified.map(time),
            };
            page
        }

        let root = page("README.md", "index.html", "Home", true);
        let landing = dated_page("docs/README", Some(-5000), Some(15000));
        let chapter = dated_page("docs/chapter", Some(0), Some(-1500));
        let deep_landing = dated_page("docs/nested/deep/README", Some(-1500), None);
        let leaf = dated_page("docs/nested/deep/leaf", None, Some(7000));
        let unknown = dated_page("docs/unknown/note", None, None);
        let updated_only = dated_page("updated/note", None, Some(0));
        let created_only = dated_page("created/README", Some(-2000), None);
        let nav_page = |page: &Page| NavNode::Page {
            title: page.title.clone(),
            target: page.target.clone(),
        };
        let nav = vec![
            nav_page(&root),
            NavNode::Group {
                label: "docs".into(),
                link: Some(landing.target.clone()),
                children: vec![
                    nav_page(&chapter),
                    NavNode::Group {
                        label: "nested".into(),
                        link: None,
                        children: vec![NavNode::Group {
                            label: "deep".into(),
                            link: Some(deep_landing.target.clone()),
                            children: vec![nav_page(&leaf)],
                        }],
                    },
                    NavNode::Group {
                        label: "unknown".into(),
                        link: None,
                        children: vec![nav_page(&unknown)],
                    },
                ],
            },
            NavNode::Group {
                label: "updated".into(),
                link: None,
                children: vec![nav_page(&updated_only)],
            },
            NavNode::Group {
                label: "created".into(),
                link: Some(created_only.target.clone()),
                children: vec![],
            },
        ];
        let site = site_with(
            vec![
                root.clone(),
                landing,
                chapter,
                deep_landing,
                leaf.clone(),
                unknown,
                updated_only,
                created_only,
            ],
            nav,
        );
        for page in [&root, &leaf] {
            let html = shell(page, &site, "", &[]);
            let group_rows: Vec<_> = extract_sidebar(&html)
                .split("<li class=\"ohm-nav-group\"")
                .skip(1)
                .map(|row| {
                    let attrs = row.split_once('>').unwrap().0;
                    let key = row
                        .split_once("data-ohm-nav-group=\"")
                        .unwrap()
                        .1
                        .split_once('"')
                        .unwrap()
                        .0;
                    (key, attrs)
                })
                .collect();
            assert_eq!(
                group_rows,
                vec![
                    (
                        "docs",
                        " data-ohm-created=\"-5000\" data-ohm-updated=\"15000\""
                    ),
                    (
                        "docs/nested",
                        " data-ohm-created=\"-1500\" data-ohm-updated=\"7000\""
                    ),
                    (
                        "docs/nested/deep",
                        " data-ohm-created=\"-1500\" data-ohm-updated=\"7000\""
                    ),
                    ("docs/unknown", ""),
                    ("updated", " data-ohm-updated=\"0\""),
                    ("created", " data-ohm-created=\"-2000\""),
                ]
            );
        }
    }

    #[test]
    fn optional_stylesheets_follow_base_with_page_relative_hrefs() {
        let root = page("docs/home.md", "index.html", "Home", true);
        let nested = page("docs/deep/guide.md", "docs/deep/guide.html", "Guide", false);
        let todo = page("TODO.md", "TODO.html", "TODO", false);
        let site = site_with(vec![root.clone(), nested.clone(), todo.clone()], vec![]);
        for (page, prefix) in [(&root, ""), (&nested, "../../"), (&todo, "")] {
            for (include_theme, include_stylesheet) in
                [(false, false), (true, false), (false, true), (true, true)]
            {
                let html = shell_with_stylesheets(
                    page,
                    &site,
                    "<p>Body</p>",
                    &[],
                    include_theme,
                    include_stylesheet,
                    DEFAULT_MAX_GROUP_SIZE,
                );
                let mut expected = vec![format!(
                    "<link rel=\"stylesheet\" href=\"{prefix}assets/ohimark.css\">"
                )];
                if include_theme {
                    expected.push(format!(
                        "<link rel=\"stylesheet\" href=\"{prefix}assets/theme.css\">"
                    ));
                }
                if include_stylesheet {
                    expected.push(format!(
                        "<link rel=\"stylesheet\" href=\"{prefix}assets/custom.css\">"
                    ));
                }
                let actual: Vec<_> = html
                    .lines()
                    .filter(|line| line.starts_with("<link rel=\"stylesheet\""))
                    .collect();
                assert_eq!(actual, expected);
                assert!(!html.contains("docs/home.md"));
            }

            let default_html = shell(page, &site, "<p>Body</p>", &[]);
            assert!(!default_html.contains("custom.css"));
            assert!(!default_html.contains("assets/theme.css"));
            assert_eq!(
                default_html,
                shell_with_stylesheets(
                    page,
                    &site,
                    "<p>Body</p>",
                    &[],
                    false,
                    false,
                    DEFAULT_MAX_GROUP_SIZE,
                )
            );
        }
    }

    #[test]
    fn r7_6_document_declares_required_metadata() {
        let root = page("README.md", "index.html", "Home", true);
        let site = site_with(vec![root.clone()], vec![]);
        let html = shell(&root, &site, "<p>hi</p>", &[]);

        assert!(html.contains("<html lang=\"en\">"), "missing lang");
        assert!(html.contains("<meta charset=\"utf-8\">"), "missing charset");
        assert!(
            html.contains(
                "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">"
            ),
            "missing viewport"
        );
        assert!(html.contains("<title>Home</title>"), "missing title");
    }

    #[test]
    fn r4_7_sidebar_identical_apart_from_current_marker() {
        let root = page("README.md", "index.html", "Home", true);
        let guide = page("guide.md", "guide.html", "Guide", false);
        let nav = vec![
            NavNode::Page {
                title: "Home".into(),
                target: root.target.clone(),
            },
            NavNode::Page {
                title: "Guide".into(),
                target: guide.target.clone(),
            },
        ];
        let site = site_with(vec![root.clone(), guide.clone()], nav);

        let on_root = shell(&root, &site, "", &[]);
        let on_guide = shell(&guide, &site, "", &[]);

        // Both pages sit at the output root, so hrefs are identical too;
        // only the current-page marker should differ. Compare the sidebar
        // region only — R4.7 constrains the sidebar, not the whole
        // document (the two documents also differ in <title>, which is
        // correct per R7.6 and no part of this requirement).
        let sidebar_root = extract_sidebar(&on_root);
        let sidebar_guide = extract_sidebar(&on_guide);

        assert_eq!(sidebar_root.matches("href=\"index.html\"").count(), 1);
        assert_eq!(sidebar_guide.matches("href=\"index.html\"").count(), 1);
        let root_link = sidebar_root
            .find("aria-current=\"page\" href=\"index.html\"")
            .expect("root page link missing from navigation header");
        let root_toggle = sidebar_root
            .find("class=\"ohm-panel-button ohm-nav-button\"")
            .expect("navigation collapse control missing from header");
        assert!(
            root_link < root_toggle,
            "root link must precede the panel control"
        );

        let strip = |h: &str| {
            h.replace(" ohm-nav-link--current", "")
                .replace(" aria-current=\"page\"", "")
        };
        // Identical on every page, once the marker is stripped.
        assert_eq!(strip(sidebar_root), strip(sidebar_guide));
        // The marker genuinely moved: the sidebars differ before stripping.
        assert_ne!(sidebar_root, sidebar_guide);

        // Exactly one current entry per sidebar, and it sits on the entry
        // whose href is that page's own target.
        assert_eq!(sidebar_root.matches("ohm-nav-link--current").count(), 1);
        assert_eq!(sidebar_guide.matches("ohm-nav-link--current").count(), 1);
        assert!(
            sidebar_root.contains(
                "ohm-nav-link ohm-nav-link--current\" aria-current=\"page\" href=\"index.html\""
            ),
            "current marker not on root's own entry: {sidebar_root}"
        );
        assert!(
            sidebar_guide.contains(
                "ohm-nav-link ohm-nav-link--current\" aria-current=\"page\" href=\"guide.html\""
            ),
            "current marker not on guide's own entry: {sidebar_guide}"
        );
    }

    #[test]
    fn r8_3_no_headings_means_no_outline_or_control() {
        let root = page("README.md", "index.html", "Home", true);
        let site = site_with(vec![root.clone()], vec![]);
        let html = shell(&root, &site, "<p>hi</p>", &[]);
        assert!(
            !html.contains("id=\"ohm-outline\""),
            "outline present with no headings"
        );
        assert!(
            !html.contains("id=\"ohm-outline-state\""),
            "outline control present with no headings"
        );
    }

    #[test]
    fn r3_4_titles_are_escaped_but_metacharacters_survive() {
        let root = page("README.md", "index.html", "[Weird] & Title", true);
        let site = site_with(vec![root.clone()], vec![]);
        let html = shell(&root, &site, "", &[]);

        assert!(html.contains("<title>[Weird] &amp; Title</title>"));
        assert!(!html.contains("<title>[Weird] & Title</title>"));
    }

    #[test]
    fn r11_1_no_href_starts_with_slash() {
        let root = page("README.md", "index.html", "Home", true);
        let guide = page("docs/guide.md", "docs/guide.html", "Guide", false);
        let nav = vec![
            NavNode::Page {
                title: "Home".into(),
                target: root.target.clone(),
            },
            NavNode::Group {
                label: "docs".into(),
                link: Some(guide.target.clone()),
                children: vec![],
            },
        ];
        let site = site_with(vec![root.clone(), guide.clone()], nav);
        let outline = vec![OutlineEntry {
            level: 2,
            id: "usage".into(),
            text: "Usage".into(),
        }];
        let html = shell(&guide, &site, "<h2 id=\"usage\">Usage</h2>", &outline);

        for href in all_hrefs(&html) {
            assert!(!href.starts_with('/'), "href starts with /: {href}");
        }
    }

    #[test]
    fn r8_5_groups_collapsible_and_start_expanded() {
        let root = page("README.md", "index.html", "Home", true);
        let guide = page("docs/guide.md", "docs/guide.html", "Guide", false);
        let note = page("notes/note.md", "notes/note.html", "Note", false);
        let nav = vec![
            NavNode::Page {
                title: "Home".into(),
                target: root.target.clone(),
            },
            NavNode::Group {
                label: "docs".into(),
                link: Some(guide.target.clone()),
                children: vec![],
            },
            NavNode::Group {
                label: "notes".into(),
                link: None,
                children: vec![NavNode::Page {
                    title: "Note".into(),
                    target: note.target.clone(),
                }],
            },
        ];
        let site = site_with(vec![root.clone(), guide.clone(), note.clone()], nav);

        // Without stored state or scripting, every group starts expanded (R8.5).
        let html = shell(&root, &site, "", &[]);
        let sidebar = extract_sidebar(&html);

        assert!(
            sidebar.contains(
                "<li class=\"ohm-nav-group\"><details class=\"ohm-nav-details\" data-ohm-nav-group=\"docs\" open><summary class=\"ohm-nav-summary\"><a class=\"ohm-nav-link ohm-nav-label\" href=\"docs/guide.html\">docs</a></summary>"
            ),
            "linked group not a details/summary starting open: {sidebar}"
        );
        assert!(
            sidebar.contains(
                "<li class=\"ohm-nav-group\"><details class=\"ohm-nav-details\" data-ohm-nav-group=\"notes\" open><summary class=\"ohm-nav-summary\"><span class=\"ohm-nav-label\">notes</span></summary>"
            ),
            "unlinked group not a details/summary starting open: {sidebar}"
        );
        assert_eq!(
            sidebar
                .matches("\" open><summary class=\"ohm-nav-summary\">")
                .count(),
            2,
            "every group must render open: {sidebar}"
        );
        assert_eq!(sidebar.matches("</details>").count(), 2);
    }

    #[test]
    fn r8_5_group_keys_follow_directory_paths_across_pages_and_nav_edits() {
        fn group_keys(html: &str) -> Vec<&str> {
            extract_sidebar(html)
                .split("data-ohm-nav-group=\"")
                .skip(1)
                .map(|rest| rest.split_once('"').unwrap().0)
                .collect()
        }

        let root = page("README.md", "index.html", "Home", true);
        let nested = page(
            r#"a & "b"/shared/leaf <one> & 'two'/page.md"#,
            r#"a & "b"/shared/leaf <one> & 'two'/page.html"#,
            "Nested",
            false,
        );
        let nav = vec![
            NavNode::Group {
                label: "a & \"b\"".into(),
                link: None,
                children: vec![NavNode::Group {
                    label: "shared".into(),
                    link: None,
                    children: vec![NavNode::Group {
                        label: "leaf <one> & 'two'".into(),
                        link: Some(nested.target.clone()),
                        children: vec![],
                    }],
                }],
            },
            NavNode::Group {
                label: "other".into(),
                link: None,
                children: vec![NavNode::Group {
                    label: "shared".into(),
                    link: None,
                    children: vec![],
                }],
            },
        ];
        let mut site = site_with(vec![root.clone(), nested.clone()], nav);
        let expected = vec![
            "a &amp; &quot;b&quot;",
            "a &amp; &quot;b&quot;/shared",
            "a &amp; &quot;b&quot;/shared/leaf &lt;one&gt; &amp; &#39;two&#39;",
            "other",
            "other/shared",
        ];

        // Ancestor labels are escaped only at emission, and repeated labels
        // in separate branches retain separate site-root-relative identities.
        let on_root = shell(&root, &site, "", &[]);
        let on_nested = shell(&nested, &site, "", &[]);
        assert_eq!(group_keys(&on_root), expected);
        assert_eq!(group_keys(&on_nested), expected);

        site.nav.reverse();
        site.nav.insert(
            0,
            NavNode::Group {
                label: "new".into(),
                link: None,
                children: vec![],
            },
        );
        let after_nav_edits = shell(&nested, &site, "", &[]);
        let edited_keys = group_keys(&after_nav_edits);
        assert_eq!(edited_keys.len(), expected.len() + 1);
        for key in expected {
            assert_eq!(
                edited_keys
                    .iter()
                    .filter(|&&candidate| candidate == key)
                    .count(),
                1,
                "existing group key changed after sorting or insertion: {key}"
            );
        }
    }

    #[test]
    fn r8_4_panel_controls_have_inline_collapse_and_expand_icons() {
        let root = page("README.md", "index.html", "Home", true);
        let site = site_with(vec![root.clone()], vec![]);
        let outline = vec![OutlineEntry {
            level: 2,
            id: "usage".into(),
            text: "Usage".into(),
        }];
        let html = shell(&root, &site, "", &outline);

        assert_eq!(
            html.matches("class=\"ohm-icon ohm-icon-collapse\"").count(),
            2
        );
        assert_eq!(
            html.matches("class=\"ohm-icon ohm-icon-expand\"").count(),
            2
        );
        assert!(!html.contains("M3 6h18M3 12h18M3 18h18"));
    }

    #[test]
    fn r8_4_navigation_control_remains_accessible_without_javascript() {
        let root = page("README.md", "index.html", "Home", true);
        let site = site_with(vec![root.clone()], vec![]);
        let html = shell(&root, &site, "", &[]);

        let checkbox = "<input type=\"checkbox\" id=\"ohm-nav-state\" class=\"ohm-nav-state ohm-panel-state\" aria-label=\"Toggle navigation\" aria-controls=\"ohm-sidebar\">";
        let checkbox_pos = html
            .find(checkbox)
            .expect("navigation checkbox missing or not keyboard accessible");
        let header_pos = html.find("<header class=\"ohm-topbar\">").unwrap();
        let body_pos = html.find("<div class=\"ohm-body\">").unwrap();
        assert!(checkbox_pos < header_pos && checkbox_pos < body_pos);
        let topbar_start = html.find("<header class=\"ohm-topbar\">").unwrap();
        let topbar_end = html.find("</header>").unwrap();
        assert!(
            !html[topbar_start..topbar_end].contains("ohm-nav-button"),
            "navigation control remains in the topbar: {html}"
        );
        assert!(html.contains("<aside class=\"ohm-sidebar\" id=\"ohm-sidebar\">"));
        assert!(html.contains("class=\"ohm-panel-header ohm-sidebar-header\""));
        assert!(html.contains("for=\"ohm-nav-state\" aria-label=\"Toggle navigation\" aria-controls=\"ohm-sidebar\" title=\"Toggle navigation\""));
        assert!(
            html.contains("<nav class=\"ohm-nav\" aria-label=\"Site navigation\" data-ohm-max-group-size=\"10\">"),
            "sidebar navigation opening markup changed: {html}"
        );

        let outline = vec![OutlineEntry {
            level: 2,
            id: "usage".into(),
            text: "Usage".into(),
        }];
        let html = shell(&root, &site, "<h2 id=\"usage\">Usage</h2>", &outline);
        let outline_checkbox = "<input type=\"checkbox\" id=\"ohm-outline-state\" class=\"ohm-panel-state ohm-outline-state\" aria-label=\"Toggle page outline\" aria-controls=\"ohm-outline\">";
        let outline_checkbox_pos = html
            .find(outline_checkbox)
            .expect("outline checkbox missing");
        let outlined_header_pos = html.find("<header class=\"ohm-topbar\">").unwrap();
        assert!(outline_checkbox_pos < outlined_header_pos);
        assert!(html.contains("<aside class=\"ohm-outline\" id=\"ohm-outline\">"));
        assert!(html.contains("<div class=\"ohm-panel-header ohm-outline-header\"><p class=\"ohm-outline-title\">On this page</p>"));
        assert!(html.contains("class=\"ohm-panel-button ohm-outline-button\" for=\"ohm-outline-state\" aria-label=\"Toggle page outline\" aria-controls=\"ohm-outline\" title=\"Toggle page outline\""));
        assert!(html.contains("<nav class=\"ohm-outline-nav\" aria-label=\"On this page\">"));
    }

    #[test]
    fn r8_8_theme_button_hidden_and_empty() {
        let root = page("README.md", "index.html", "Home", true);
        let site = site_with(vec![root.clone()], vec![]);
        let html = shell(&root, &site, "", &[]);

        let open = "<button type=\"button\" class=\"ohm-icon-button ohm-theme-button\" id=\"ohm-theme-button\" hidden aria-label=\"Switch colour theme\">";
        let start = html
            .find(open)
            .expect("theme button not found or markup changed")
            + open.len();
        let rest = &html[start..];
        let end = rest.find("</button>").expect("theme button not closed");
        // Empty by default: ohimark.js injects the icon at runtime, so with
        // scripting off no dead control is shown (R8.8).
        assert_eq!(&rest[..end], "", "theme button is not empty: {html}");
    }
}
