//! A minimal SVG renderer for Mermaid gantt charts.
//!
//! `merman` works out the schedule - bar positions, axis ticks, excluded ranges - and this
//! module redraws it in the site's own shape: every label in a left column, plain
//! unoutlined bars, and one axis along the bottom. The SVG carries no colours, only
//! classes that section 7 of `ohimark.css` styles, so a single render is correct under
//! both themes.

use std::f64::consts::FRAC_1_SQRT_2;
use std::fmt::Write as _;

use serde_json::Value;

use crate::model::escape_html;

/// Width of the area the bars are drawn in; merman's own time scale is mapped onto it.
const CHART_W: f64 = 640.0;
/// Room right of the chart area, so a milestone on the last day is not clipped.
const RIGHT_MARGIN: f64 = 16.0;
/// The label column is never narrower than this, however short the labels.
const MIN_LABEL_COL: f64 = 96.0;
/// Gap between the widest label and the vertical axis.
const LABEL_PAD: f64 = 12.0;
const SECTION_X: f64 = 4.0;
/// Tasks sit indented beneath their section name.
const TASK_INDENT: f64 = 14.0;
const ROW_H: f64 = 24.0;
const BAR_H: f64 = 16.0;
const BAR_RADIUS: f64 = 3.0;
const MIN_BAR_W: f64 = 2.0;
/// Side of the square a milestone's diamond is cut from.
const MILESTONE_SIZE: f64 = 12.0;
const TITLE_H: f64 = 28.0;
/// Height below the last row for the axis line, its ticks and their labels.
const AXIS_H: f64 = 28.0;
const TICK_LEN: f64 = 5.0;
/// Centre of a tick label, measured down from the end of its tick.
const TICK_LABEL_DROP: f64 = 10.0;
/// A marker's label sits this far beside its line and this far below the line's top.
const MARKER_LABEL_GAP: f64 = 4.0;
const MARKER_LABEL_DROP: f64 = 8.0;
const LABEL_FONT: f64 = 13.0;
const SMALL_FONT: f64 = 11.0;
const TITLE_FONT: f64 = 14.0;
/// Estimated advance of one character, in ems. Deliberately generous: an overestimate
/// only widens the label column, an underestimate would run a label into the bars.
const CHAR_EM: f64 = 0.6;
const BOLD_CHAR_EM: f64 = 0.64;
/// Distance from a line's vertical centre down to its baseline, in ems.
const BASELINE_EM: f64 = 0.35;

/// One task as merman laid it out, with its bar already mapped into the chart area.
struct Task<'a> {
    label: &'a str,
    section: &'a str,
    order: i64,
    /// Left edge of the bar, measured from the vertical axis.
    offset: f64,
    width: f64,
    milestone: bool,
    vert: bool,
    classes: String,
}

impl<'a> Task<'a> {
    fn parse(value: &'a Value, left: f64, scale: f64) -> Option<Self> {
        let bar = value.get("bar")?;
        Some(Self {
            label: value.get("task")?.as_str()?.trim(),
            section: value.get("section")?.as_str()?,
            order: value.get("order")?.as_i64()?,
            offset: (number(bar, "x")? - left) * scale,
            width: number(bar, "width")? * scale,
            milestone: value.get("milestone")?.as_bool()?,
            vert: value.get("vert")?.as_bool()?,
            classes: bar_classes(bar.get("class")?.as_str()?),
        })
    }

    fn label_x(&self) -> f64 {
        if self.section.is_empty() {
            SECTION_X
        } else {
            SECTION_X + TASK_INDENT
        }
    }
}

/// One band of the chart: a section name on its own, or a task and its bar.
enum Row<'a> {
    Section(&'a str),
    Task(&'a Task<'a>),
}

/// Draws the gantt chart in merman's layout JSON as a complete `<svg>` element, or
/// `None` when the JSON is not a gantt layout this renderer can draw (the caller then
/// falls back to merman's own SVG).
pub(crate) fn render(layout: &Value, id: &str) -> Option<String> {
    let chart = layout.pointer("/layout/GanttDiagram")?;
    let left = number(chart, "left_padding")?;
    let span = number(chart, "width")? - left - number(chart, "right_padding")?;
    if !span.is_finite() || span <= 0.0 {
        return None;
    }
    // merman's x becomes a distance into the chart area: `(x - left) * scale`.
    let scale = CHART_W / span;

    let mut tasks = chart
        .get("tasks")?
        .as_array()?
        .iter()
        .map(|task| Task::parse(task, left, scale))
        .collect::<Option<Vec<_>>>()?;
    tasks.sort_by_key(|task| task.order);

    let mut sections: Vec<&str> = Vec::new();
    let categories = list(chart, "categories").filter_map(Value::as_str);
    for section in categories.chain(tasks.iter().map(|task| task.section)) {
        if !sections.contains(&section) {
            sections.push(section);
        }
    }
    let mut rows = Vec::new();
    for section in sections {
        if !section.is_empty() {
            rows.push(Row::Section(section));
        }
        let in_section = |task: &&Task| !task.vert && task.section == section;
        rows.extend(tasks.iter().filter(in_section).map(Row::Task));
    }
    if !rows.iter().any(|row| matches!(row, Row::Task(_))) {
        return None;
    }

    // Wide labels widen the column, and with it the viewBox, instead of being clipped.
    let label_col = rows
        .iter()
        .map(|row| match row {
            Row::Section(name) => SECTION_X + text_width(name, LABEL_FONT, BOLD_CHAR_EM),
            Row::Task(task) => task.label_x() + text_width(task.label, LABEL_FONT, CHAR_EM),
        })
        .fold(MIN_LABEL_COL, |widest, end| {
            widest.max((end + LABEL_PAD).ceil())
        });
    let title = text_at(chart, "/title");
    let top = if title.is_some() { TITLE_H } else { 0.0 };
    let bottom = top + f64::from(u32::try_from(rows.len()).ok()?) * ROW_H;
    let chart_end = label_col + CHART_W;
    let width = chart_end + RIGHT_MARGIN;
    let height = bottom + AXIS_H;

    // The same root sizing merman emits, so the stylesheet's diagram rules apply as is.
    let id = escape_html(id);
    let acc_title = text_at(layout, "/semantic/accTitle");
    let acc_descr = text_at(layout, "/semantic/accDescr");
    let mut svg = String::new();
    let _ = write!(
        svg,
        r#"<svg id="{id}" class="ohm-gantt" width="100%" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" style="max-width: {w}px;" role="graphics-document document" aria-roledescription="gantt""#,
        w = px(width),
        h = px(height),
    );
    if acc_title.is_some() {
        let _ = write!(svg, r#" aria-labelledby="chart-title-{id}""#);
    }
    if acc_descr.is_some() {
        let _ = write!(svg, r#" aria-describedby="chart-desc-{id}""#);
    }
    svg.push('>');
    if let Some(text) = acc_title {
        let text = escape_html(text);
        let _ = write!(svg, r#"<title id="chart-title-{id}">{text}</title>"#);
    }
    if let Some(text) = acc_descr {
        let text = escape_html(text);
        let _ = write!(svg, r#"<desc id="chart-desc-{id}">{text}</desc>"#);
    }

    if let Some(title) = title {
        let attributes = r#" font-weight="600" text-anchor="middle""#;
        let at = (width / 2.0, TITLE_H / 2.0);
        push_text(
            &mut svg,
            "ohm-gantt-title",
            at,
            TITLE_FONT,
            attributes,
            title,
        );
    }

    // Excluded ranges first, so the bars are drawn over them.
    for exclude in list(chart, "excludes") {
        let (Some(x), Some(exclude_width)) = (number(exclude, "x"), number(exclude, "width"))
        else {
            continue;
        };
        let start = ((x - left) * scale).max(0.0);
        let end = ((x + exclude_width - left) * scale).min(CHART_W);
        if end > start {
            let _ = write!(
                svg,
                r#"<rect class="ohm-gantt-exclude" x="{}" y="{}" width="{}" height="{}"/>"#,
                px(label_col + start),
                px(top),
                px(end - start),
                px(bottom - top),
            );
        }
    }

    let mut mid = top + ROW_H / 2.0;
    for row in &rows {
        match row {
            Row::Section(name) => push_text(
                &mut svg,
                "ohm-gantt-section",
                (SECTION_X, mid),
                LABEL_FONT,
                r#" font-weight="600""#,
                name,
            ),
            Row::Task(task) => {
                let at = (task.label_x(), mid);
                push_text(&mut svg, "ohm-gantt-task", at, LABEL_FONT, "", task.label);
                let classes = &task.classes;
                if task.milestone {
                    // merman's milestone bar is a square centred on the task's time.
                    let x = label_col + task.offset + task.width / 2.0;
                    let reach = MILESTONE_SIZE * FRAC_1_SQRT_2;
                    let _ = write!(
                        svg,
                        r#"<path class="{classes}" d="M{x},{north}L{east},{y}L{x},{south}L{west},{y}Z"/>"#,
                        x = px(x),
                        y = px(mid),
                        north = px(mid - reach),
                        east = px(x + reach),
                        south = px(mid + reach),
                        west = px(x - reach),
                    );
                } else {
                    let _ = write!(
                        svg,
                        r#"<rect class="{classes}" x="{}" y="{}" width="{}" height="{}" rx="{}"/>"#,
                        px(label_col + task.offset),
                        px(mid - BAR_H / 2.0),
                        px(task.width.max(MIN_BAR_W)),
                        px(BAR_H),
                        px(BAR_RADIUS),
                    );
                }
            }
        }
        mid += ROW_H;
    }

    // `vert` tasks mark a date across every row rather than taking one of their own.
    for task in tasks.iter().filter(|task| task.vert) {
        if !(0.0..=CHART_W).contains(&task.offset) {
            continue;
        }
        let x = label_col + task.offset;
        push_line(&mut svg, "ohm-gantt-marker", (x, top), (x, bottom));
        // The label sits right of the line unless that would run it out of the viewBox.
        let label_end = x + MARKER_LABEL_GAP + text_width(task.label, SMALL_FONT, CHAR_EM);
        let (label_x, attributes) = if label_end <= width {
            (x + MARKER_LABEL_GAP, "")
        } else {
            (x - MARKER_LABEL_GAP, r#" text-anchor="end""#)
        };
        push_text(
            &mut svg,
            "ohm-gantt-marker-label",
            (label_x, top + MARKER_LABEL_DROP),
            SMALL_FONT,
            attributes,
            task.label,
        );
    }

    push_line(
        &mut svg,
        "ohm-gantt-axis",
        (label_col, top),
        (label_col, bottom),
    );
    push_line(
        &mut svg,
        "ohm-gantt-axis",
        (label_col, bottom),
        (chart_end, bottom),
    );
    for tick in list(chart, "bottom_ticks") {
        let (Some(x), Some(label)) = (number(tick, "x"), text_at(tick, "/label")) else {
            continue;
        };
        let offset = (x - left) * scale;
        if !(-0.5..=CHART_W + 0.5).contains(&offset) {
            continue;
        }
        let x = label_col + offset;
        let tick_end = bottom + TICK_LEN;
        push_line(&mut svg, "ohm-gantt-axis", (x, bottom), (x, tick_end));
        // Centred under its tick, pulled in only as far as keeps it inside the viewBox.
        let half = text_width(label, SMALL_FONT, CHAR_EM) / 2.0;
        push_text(
            &mut svg,
            "ohm-gantt-tick",
            (x.min(width - half), tick_end + TICK_LABEL_DROP),
            SMALL_FONT,
            r#" text-anchor="middle""#,
            label,
        );
    }

    svg.push_str("</svg>");
    Some(svg)
}

/// The classes for a task's bar. merman records a task's state only in the class it
/// would give its own bar: `done0`, `active1`, `crit2`, `doneCrit0` or `activeCrit1`, the
/// number being a section colour slot.
fn bar_classes(merman_class: &str) -> String {
    let mut classes = String::from("ohm-gantt-bar");
    for token in merman_class.split_whitespace() {
        let state = token.trim_end_matches(|c: char| c.is_ascii_digit());
        if matches!(state, "done" | "doneCrit") {
            classes.push_str(" ohm-gantt-bar--done");
        }
        if matches!(state, "active" | "activeCrit") {
            classes.push_str(" ohm-gantt-bar--active");
        }
        if matches!(state, "crit" | "doneCrit" | "activeCrit") {
            classes.push_str(" ohm-gantt-bar--crit");
        }
    }
    classes
}

/// Appends one line of text whose vertical centre is `at.1`. The baseline is set
/// explicitly rather than through `dominant-baseline`, which not every SVG consumer honours.
fn push_text(
    svg: &mut String,
    class: &str,
    at: (f64, f64),
    size: f64,
    attributes: &str,
    content: &str,
) {
    let _ = write!(
        svg,
        r#"<text class="{class}" x="{}" y="{}" font-size="{}"{attributes}>{}</text>"#,
        px(at.0),
        px(at.1 + size * BASELINE_EM),
        px(size),
        escape_html(content),
    );
}

fn push_line(svg: &mut String, class: &str, from: (f64, f64), to: (f64, f64)) {
    let _ = write!(
        svg,
        r#"<line class="{class}" x1="{}" y1="{}" x2="{}" y2="{}"/>"#,
        px(from.0),
        px(from.1),
        px(to.0),
        px(to.1),
    );
}

/// Estimates the rendered width of `text`: `em` per character, a full em for the wide
/// scripts (CJK and beyond) that a Latin average would badly undersize.
fn text_width(text: &str, size: f64, em: f64) -> f64 {
    let ems: f64 = text
        .chars()
        .map(|ch| if ch >= '\u{2E80}' { 1.0 } else { em })
        .sum();
    ems * size
}

/// Formats a coordinate with at most two decimals and no trailing zeros.
fn px(value: f64) -> String {
    ((value * 100.0).round() / 100.0).to_string()
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value.get(key)?.as_f64()
}

/// The items of an array field, or nothing when the field is absent.
fn list<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// The non-blank string at a JSON pointer.
fn text_at<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    let text = value.pointer(pointer)?.as_str()?.trim();
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A gantt layout in the shape merman emits, 650 wide between its paddings, with one
    /// excluded range, one tick inside the chart area and one tick outside it.
    fn chart(tasks: Value) -> Value {
        json!({ "layout": { "GanttDiagram": {
            "width": 800.0,
            "left_padding": 75.0,
            "right_padding": 75.0,
            "categories": ["", "Build"],
            "tasks": tasks,
            "excludes": [{ "x": 400.0, "width": 65.0 }],
            "bottom_ticks": [
                { "time_ms": 0, "x": 75.0, "label": "Jan 05" },
                { "time_ms": 0, "x": 900.0, "label": "Dec 31" }
            ],
            "title": null
        } } })
    }

    fn task(label: &str, section: &str, order: i64, x: f64, width: f64, class: &str) -> Value {
        json!({
            "task": label,
            "section": section,
            "order": order,
            "milestone": class.contains("milestone"),
            "vert": class.contains("vert"),
            "bar": { "x": x, "width": width, "class": class }
        })
    }

    #[test]
    fn a_layout_it_cannot_draw_is_declined() {
        assert!(render(&json!({}), "g").is_none());
        assert!(render(&json!({ "layout": { "FlowchartV2": {} } }), "g").is_none());
        assert!(render(&chart(json!([])), "g").is_none());
        let marker_only = json!([task("Freeze", "Build", -1, 200.0, 1.6, "task vert task0")]);
        assert!(render(&chart(marker_only), "g").is_none());
        let malformed = json!([{ "task": "No bar", "section": "Build" }]);
        assert!(render(&chart(malformed), "g").is_none());
    }

    #[test]
    fn bars_ticks_and_excludes_are_scaled_into_the_chart_area() {
        let tasks = json!([task("Short", "Build", 0, 75.0, 650.0, "task task0")]);
        let svg = render(&chart(tasks), "g").expect("chart renders");
        // Short labels leave the column at its minimum: 96 + 640 + 16 wide, with a
        // section row and a task row above the 28px axis band.
        assert!(svg.starts_with("<svg id=\"g\" class=\"ohm-gantt\" width=\"100%\""));
        assert!(svg.contains("viewBox=\"0 0 752 76\" style=\"max-width: 752px;\""));
        for element in [
            "<rect class=\"ohm-gantt-bar\" x=\"96\" y=\"28\" width=\"640\" height=\"16\" rx=\"3\"/>",
            "<rect class=\"ohm-gantt-exclude\" x=\"416\" y=\"0\" width=\"64\" height=\"48\"/>",
            "<line class=\"ohm-gantt-axis\" x1=\"96\" y1=\"0\" x2=\"96\" y2=\"48\"/>",
            "<line class=\"ohm-gantt-axis\" x1=\"96\" y1=\"48\" x2=\"736\" y2=\"48\"/>",
            "<line class=\"ohm-gantt-axis\" x1=\"96\" y1=\"48\" x2=\"96\" y2=\"53\"/>",
            "<text class=\"ohm-gantt-tick\" x=\"96\"",
            ">Jan 05</text>",
        ] {
            assert!(svg.contains(element), "{element} is missing from {svg}");
        }
        assert!(
            !svg.contains("Dec 31"),
            "a tick outside the chart is skipped"
        );
    }

    #[test]
    fn labels_are_escaped_and_widen_the_view_box() {
        let label = "A <very> long & winding task name that no fixed column would hold";
        let tasks = json!([task(label, "R&D", 0, 75.0, 100.0, "task task0")]);
        let mut layout = chart(tasks);
        layout["layout"]["GanttDiagram"]["categories"] = json!(["R&D"]);
        layout["layout"]["GanttDiagram"]["title"] = json!("Plan <1>");
        let svg = render(&layout, "g").expect("chart renders");
        assert!(svg.contains(">A &lt;very&gt; long &amp; winding task name"));
        assert!(svg.contains(">R&amp;D</text>"));
        assert!(svg.contains(">Plan &lt;1&gt;</text>"));
        assert!(!svg.contains("<very>") && !svg.contains("R&D"));
        let width: f64 = svg
            .split("viewBox=\"0 0 ")
            .nth(1)
            .and_then(|rest| rest.split(' ').next())
            .and_then(|width| width.parse().ok())
            .expect("viewBox has a width");
        assert!(width > 1100.0, "{width} did not grow with the label");
    }

    #[test]
    fn tasks_without_a_section_get_no_header_row_and_no_indent() {
        let tasks = json!([
            task("Nested", "Build", 1, 300.0, 100.0, "task task1"),
            task("Loose", "", 0, 75.0, 100.0, "task task0"),
        ]);
        let svg = render(&chart(tasks), "g").expect("chart renders");
        let loose = svg
            .find("<text class=\"ohm-gantt-task\" x=\"4\" y=\"16.55\" font-size=\"13\">Loose<")
            .expect("the loose task is on the first row, flush left");
        let section = svg
            .find("<text class=\"ohm-gantt-section\" x=\"4\" y=\"40.55\"")
            .expect("the section name is on the second row");
        let nested = svg
            .find("<text class=\"ohm-gantt-task\" x=\"18\" y=\"64.55\" font-size=\"13\">Nested<")
            .expect("the section's task is indented on the third row");
        assert!(loose < section && section < nested);
        assert_eq!(svg.matches("ohm-gantt-section").count(), 1);
    }

    #[test]
    fn milestones_and_markers_are_drawn_at_their_time() {
        let tasks = json!([
            task("Work", "Build", 0, 75.0, 2.0, "task task0"),
            task("Ship", "Build", 1, 390.0, 20.0, "task milestone crit0"),
            task("Freeze", "Build", -1, 400.0, 1.6, "task vert task0"),
        ]);
        let svg = render(&chart(tasks), "g").expect("chart renders");
        for element in [
            // The milestone's square spans 390..410 for merman, so its centre maps to 416.
            "<path class=\"ohm-gantt-bar ohm-gantt-bar--crit\" d=\"M416,51.51L424.49,60L416,68.49L407.51,60Z\"/>",
            "<line class=\"ohm-gantt-marker\" x1=\"416\" y1=\"0\" x2=\"416\" y2=\"72\"/>",
            "<text class=\"ohm-gantt-marker-label\" x=\"420\"",
            // A bar thinner than the minimum is still drawn 2px wide.
            "x=\"96\" y=\"28\" width=\"2\" height=\"16\"",
        ] {
            assert!(svg.contains(element), "{element} is missing from {svg}");
        }
        assert_eq!(svg.matches("class=\"ohm-gantt-bar").count(), 2);
    }

    #[test]
    fn bar_state_is_read_from_merman_bar_classes() {
        for (merman, ours) in [
            ("task task0", "ohm-gantt-bar"),
            ("task done1", "ohm-gantt-bar ohm-gantt-bar--done"),
            ("task active11", "ohm-gantt-bar ohm-gantt-bar--active"),
            ("task crit2", "ohm-gantt-bar ohm-gantt-bar--crit"),
            (
                "task activeCrit2",
                "ohm-gantt-bar ohm-gantt-bar--active ohm-gantt-bar--crit",
            ),
            (
                "task milestone doneCrit0",
                "ohm-gantt-bar ohm-gantt-bar--done ohm-gantt-bar--crit",
            ),
        ] {
            assert_eq!(bar_classes(merman), ours);
        }
    }
}
