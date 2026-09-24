//! The page shell: wraps a rendered page body in the site's HTML document,
//! sidebar navigation (R4) and per-page outline (R8.3). Also bundles the
//! theme's CSS and JS, embedded at compile time so the built site needs no
//! network access at runtime (R8.7).

use camino::Utf8Path;

use crate::links;
use crate::model::{NavNode, OutlineEntry, Page, Site, escape_html};

/// The site stylesheet, embedded at compile time (R8.7: nothing is fetched
/// at runtime).
pub const STYLE_CSS: &str = include_str!("assets/ohimark.css");
/// The site script, embedded at compile time.
pub const APP_JS: &str = include_str!("assets/ohimark.js");

/// Output-relative path the stylesheet is written to.
pub const STYLE_PATH: &str = "assets/ohimark.css";
/// Output-relative path the script is written to.
pub const SCRIPT_PATH: &str = "assets/ohimark.js";

/// Render the sidebar recursively. Hrefs are computed relative to `dir`, the
/// directory of the page currently being rendered, so the same call from two
/// different pages necessarily yields different hrefs by depth (R11.1) while
/// the structure, order and labels stay invariant (R4.7) — only the
/// current-page marker moves.
fn render_nav_list(nodes: &[NavNode], page: &Page, dir: &Utf8Path) -> String {
    let mut out = String::new();
    for node in nodes {
        render_nav_node(node, page, dir, &mut out);
    }
    out
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

fn render_nav_node(node: &NavNode, page: &Page, dir: &Utf8Path, out: &mut String) {
    match node {
        NavNode::Page { title, target } => {
            let href = links::relative_to(dir, target.as_path());
            let title = escape_html(title);
            let (current_class, current_attr) = current_marker(*target == page.target);
            out.push_str(&format!(
                "<li class=\"ohm-nav-item\"><a class=\"ohm-nav-link{current_class}\"{current_attr} href=\"{href}\">{title}</a></li>"
            ));
        }
        NavNode::Group {
            label,
            link,
            children,
        } => {
            let label = escape_html(label);
            let children_html = render_nav_list(children, page, dir);
            match link {
                // R4.2: a directory whose own page's stem folds to the
                // directory name is a link to that page.
                Some(target) => {
                    let href = links::relative_to(dir, target.as_path());
                    let (current_class, current_attr) = current_marker(*target == page.target);
                    out.push_str(&format!(
                        "<li class=\"ohm-nav-group\"><a class=\"ohm-nav-link ohm-nav-label{current_class}\"{current_attr} href=\"{href}\">{label}</a><ul class=\"ohm-nav-list\">{children_html}</ul></li>"
                    ));
                }
                // R4.3: otherwise the node is a plain label with no link.
                None => {
                    out.push_str(&format!(
                        "<li class=\"ohm-nav-group\"><span class=\"ohm-nav-label\">{label}</span><ul class=\"ohm-nav-list\">{children_html}</ul></li>"
                    ));
                }
            }
        }
    }
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
        "<aside class=\"ohm-outline\">\n<nav class=\"ohm-outline-nav\" aria-label=\"On this page\">\n<p class=\"ohm-outline-title\">On this page</p>\n<ul class=\"ohm-outline-list\">{items}</ul>\n</nav>\n</aside>\n"
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

    let nav = render_nav_list(&site.nav, page, dir);
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
    // The only inline script on the page: applies the stored theme before
    // first paint so the page does not flash (R8.1). Kept byte-for-byte.
    out.push_str(
        "<script>(function(){try{var t=sessionStorage.getItem(\"ohimark-theme\");if(t===\"dark\"||t===\"light\"){document.documentElement.setAttribute(\"data-theme\",t);}}catch(e){}})();</script>\n",
    );
    out.push_str("</head>\n<body>\n");
    out.push_str("<input type=\"checkbox\" id=\"ohm-nav-state\" class=\"ohm-nav-state\" hidden>\n");
    out.push_str("<header class=\"ohm-topbar\">\n");
    out.push_str("<div class=\"ohm-topbar-lead\">\n");
    // The hamburger is inline SVG, not JS-injected, so it works with
    // scripting off (R8.8).
    out.push_str(
        "<label class=\"ohm-nav-button\" for=\"ohm-nav-state\" aria-label=\"Toggle navigation\"><svg class=\"ohm-icon\" viewBox=\"0 0 24 24\" aria-hidden=\"true\" focusable=\"false\"><path d=\"M3 6h18M3 12h18M3 18h18\"/></svg></label>\n",
    );
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
    out.push_str("<aside class=\"ohm-sidebar\">\n");
    out.push_str("<nav class=\"ohm-nav\" aria-label=\"Site navigation\">\n");
    out.push_str("<ul class=\"ohm-nav-list\">");
    out.push_str(&nav);
    out.push_str("</ul>\n");
    out.push_str("</nav>\n");
    out.push_str("</aside>\n");
    out.push_str("<main class=\"ohm-main\">\n");
    out.push_str("<article class=\"ohm-content\">");
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
            is_root,
        }
    }

    fn site_with(pages: Vec<Page>, nav: Vec<NavNode>) -> Site {
        Site {
            pages,
            by_source: Default::default(),
            nav,
        }
    }

    /// The contents of the `<nav class="ohm-nav" …>…</nav>` sidebar region.
    /// R4.7 constrains the sidebar, not the whole document, so tests must
    /// compare this slice rather than the full rendered page.
    fn extract_sidebar(html: &str) -> &str {
        let open = "<nav class=\"ohm-nav\" aria-label=\"Site navigation\">";
        let start = html.find(open).expect("sidebar nav opening tag not found") + open.len();
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
    fn r8_3_no_headings_means_no_outline() {
        let root = page("README.md", "index.html", "Home", true);
        let site = site_with(vec![root.clone()], vec![]);
        let html = shell(&root, &site, "<p>hi</p>", &[]);
        assert!(
            !html.contains("ohm-outline"),
            "outline present with no headings"
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
    fn r8_8_hamburger_is_inline_svg() {
        let root = page("README.md", "index.html", "Home", true);
        let site = site_with(vec![root.clone()], vec![]);
        let html = shell(&root, &site, "", &[]);

        assert!(
            html.contains(
                "<svg class=\"ohm-icon\" viewBox=\"0 0 24 24\" aria-hidden=\"true\" focusable=\"false\"><path d=\"M3 6h18M3 12h18M3 18h18\"/></svg>"
            ),
            "hamburger svg missing, so navigation toggle would not work with scripting off: {html}"
        );
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
