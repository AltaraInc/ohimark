//! `ohimark`: a single-shot static site generator that turns a directory of
//! Markdown into HTML, CSS and JS. See `docs/architecture.md` for the shape
//! of the pipeline this crate implements.

pub mod cli;
pub mod emit;
mod gantt;
pub mod links;
pub mod model;
pub mod plan;
pub mod render;
mod task_tags;
pub mod theme;
pub mod todo;

pub use cli::{Args, Config, PresetTheme};
pub use model::{Error, Result, Warnings};

use rayon::prelude::*;

use render::{Diagrams, Rendered};

/// Run one build end to end: plan, render, emit.
///
/// Returns the warnings raised along the way. They never affect the exit
/// code (R10.5) — the caller is expected to print them to standard error and
/// still exit successfully.
pub fn build(config: &Config) -> Result<Warnings> {
    let planned = plan::plan(config)?;
    let mut warnings = planned.warnings;

    // Render every page in parallel. Nothing in this phase may write to
    // stderr: each task's own `Warnings` is collected and only absorbed,
    // in page order, once the whole phase is done (R11.3).
    let diagrams = Diagrams::new();
    let mut rendered: Vec<Rendered> = planned
        .site
        .pages
        .par_iter()
        .zip(planned.sources.par_iter())
        .map(|(page, source)| render::render_page(page, source, &planned.site, &diagrams))
        .collect();

    // R12: with `--todo`, the TODO page is rendered again with every other page's
    // open tasks appended. The re-render's warnings replace the first render's, so
    // none is reported twice.
    if let Some(todo_index) = planned.site.todo {
        rendered[todo_index] = todo::render_page(
            &planned.site,
            &planned.sources,
            todo_index,
            !planned.todo_synthetic,
            &diagrams,
        );
    }

    // Order-sensitive from here on: absorb warnings in page order so stderr
    // is byte-stable across runs of an unchanged tree (R11.3).
    for page_rendered in &mut rendered {
        warnings.absorb(std::mem::take(&mut page_rendered.warnings));
    }

    let emit_warnings = emit::emit(config, &planned.site, &rendered)?;
    warnings.absorb(emit_warnings);

    Ok(warnings)
}
