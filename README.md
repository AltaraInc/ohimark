# ohimark

`ohimark` builds markdown documentation into static HTML, CSS and JS. The tool walks the directory structure from the provided root directory collecting all markdown files (while honoring the provided ignore files; e.g. `.gitignore`) and renders each markdown file to its' respective HTML page.

The directory structure is reflected in navigation: directories become groups for their pages and subdirectories. A directory heading links to its own same-name Markdown page when one exists (case-insensitive); otherwise, it links to a case-insensitive `README.md` or `README.markdown` match. The selected page appears on the heading instead of again among the directory's child pages.

## Rendering

Each markdown file that is collected during the directory traversal will be translated into its' own HTML page. All the [basic syntax](https://www.markdownguide.org/basic-syntax), tables, [task lists](https://www.markdownguide.org/extended-syntax/#task-lists) and mermaid diagrams will be supported.

Mermaid diagrams are rendered to SVG at build time by [`merman`](https://github.com/Latias94/merman), a headless Rust implementation of Mermaid. Nothing is fetched while building and no diagram runtime is shipped to the browser, so diagrams are visible even with JavaScript disabled.

With JavaScript enabled, rectangular tables with one header row can be resized in
the browser. Drag a header edge to give that column more or less space while the
other columns keep their widths. Grips show two short, muted bars. Use the final
column's grip to adjust the table's overall width while earlier columns stay
fixed. The **Reset widths** icon in the footer restores automatic sizing. All
controls stay within the table.

Each table's widths are remembered for the current browsing session across page
navigation and reloads. A table that grows beyond the reading column uses the
available main pane; larger tables scroll horizontally within their own frame.
Tables placed in lists or blockquotes stay within their local scrolling area.
Spanning and nested HTML tables stay unchanged. Changing or removing a header
starts a new saved layout for that table; tables with the same headers keep
separate settings by their order on the page. Without JavaScript, tables render
as usual without resize controls.

Task list items (`- [ ]` and `- [x]`) render as checkboxes reflecting the marker in the source. They are read-only: the rendered page always matches the Markdown it was built from, and ticking a box in a browser is not a way to edit the file.

Add tags to a task by placing a brace group in its text, such as `- [ ] Book a ticket {travel, buy}` or `- [ ] Pack {travel}`. Each tag appears as a badge with square corners and a faded theme color beside that task on every page, whether or not `--todo` is enabled. A tag keeps the same color across pages and reloads, ignoring case. Tags belong to the task whose text contains them; prose in nested tasks has its own tags.

## Usage

`ohimark` requires a root directory to start from and a directory to write to. It optionally accepts a root file (defaulting to `README.md`), exclusion globs, standalone HTML pages, a built-in theme, a custom stylesheet, a sidebar group size, and a flag to collect open tasks.

```sh
ohimark --out OUTDIR [--root FILENAME] [--exclude GLOB]... [--endpoint TITLE PATH]... [--todo] [--max-group-size N] [--theme default|bookish] [--stylesheet FILE] DIR
```

- `--out` or `-o`: directory to write the generated static files to. Required; it must be empty or not yet exist.
- `--root` or `-r`: the site's home page, given as a path relative to `DIR`. Defaults to `README.md`. It is emitted as `index.html`.
- `--exclude`: a glob of paths to skip, in addition to the ignore files. Repeatable.
- `--endpoint TITLE PATH`: copy a standalone HTML page and add a top-level sidebar link using `TITLE`. Zero or more occurrences are accepted: omit the option for no additional pages, or repeat it for each page. Each occurrence takes exactly `TITLE PATH`; links appear in command-line order after the Markdown entries. Quote titles or paths containing spaces. Paths resolve from the current working directory and may be absolute, outside `DIR`, ignored, or excluded. Each title must be nonblank UTF-8 text, and each supplied and canonical path must be UTF-8 and name a readable regular file. No filename extension or HTML parsing is required.
- `--todo`: collect every unchecked task list item (`- [ ]`) from all pages into the root-level TODO page, under a heading linking each file, in page order. If `DIR` has a top-level `TODO.md`, the tasks are appended to it under a `Collected Tasks` heading and its own tasks are not repeated; otherwise a `TODO.html` page is generated. With at least two collected files, a sidebar-style sort chooser offers filename order, Name A–Z/Z–A, and Created or Updated newest/oldest, using source dates captured at build time. It works with tag filtering, remembers the choice per site for the current browser session, and keeps the outline in sync. Without JavaScript, files stay in filename order. The final TODO page also adds a compact **Filter by tags** search line with fuzzy tag-name matching and up to eight suggestions; typing alone leaves the task list unchanged. Choose tags with the pointer or Arrow Up/Down and Enter; Escape closes the suggestions. The line shows up to two removable selected tags, a `+N` overflow count, Clear and an `N/T` direct-match count. Selecting several tags shows tasks matching any selected tag, along with their parent tasks for context. Clearing the selection shows every task, including untagged tasks. Filtering is available with JavaScript; without it, the complete list and tag badges remain visible. Selections reset when the page reloads. When there are no open tasks to collect, nothing is added and no `TODO.html` is generated. This only affects the HTML output: nothing is written to `DIR`. Off by default.
- `--max-group-size N`: maximum number of immediate page and subgroup rows initially shown in each main-sidebar group before vertical scrolling. Defaults to `10`; N must be a positive 32-bit unsigned integer, and the last repeated value wins. Small `...` cues appear independently above and below the list while more content remains in that direction: bottom only at the start, both in the middle, and top only at the end. Nested groups are capped independently. With JavaScript disabled, all navigation rows remain available without a group cap.
- `--theme default|bookish`: use an embedded visual preset. If repeated, the last value is used; other values are rejected.
- `--stylesheet FILE`: add a stylesheet after the minimal bundled base and any selected preset. The file must be an existing, readable regular file; its supplied and canonical paths must be UTF-8. Its contents are copied unchanged. It may be inside or outside `DIR`, including a file ignored or excluded from discovery. Relative paths are resolved from the current working directory; absolute paths are also accepted. If repeated, the last value is used.

### Standalone HTML pages

```sh
ohimark --out site --endpoint "Project report" docs/reports/report.html --endpoint "API reference" ../reference.html docs
```

This copies the first page to `site/reports/report.html` and the external page to
`site/reference.html`. A source whose canonical path is inside `DIR` keeps its
`DIR`-relative path and case; an external source uses the supplied filename at the
output root. Sidebar links work from nested Markdown pages and a relocated home
page, with URL characters in filenames encoded.

The explicit HTML copy preserves bytes without a page shell or task collection.
Their links, images, stylesheets, and scripts are not discovered, copied, or
rewritten; provide those dependencies separately at paths that work from each
page's output location. A selected page cannot share an output path, or create a
file/directory conflict, with another page, a referenced asset, or bundled or
selected CSS and JS. Checks ignore case. Selecting the same source already
referenced as a Markdown asset is allowed. An invalid selection or collision
fails the build and leaves output absent or empty; input files stay untouched.

### Custom styles

The bundled CSS is a minimal base. Without `--theme` and `--stylesheet`, pages use
browser-native fonts, colors and element defaults; the base keeps navigation, panels
and diagrams functional. Built-in presets are opt-in:

- `default` gives a modern, neutral system-ui look, using the embedded
  `themes/default.css` preset.
- `bookish` gives a warm-paper, serif look with red accents, using the embedded
  `themes/bookish.css` preset.

Choose either preset explicitly:

```sh
ohimark --out site --theme default docs
ohimark --out site --theme bookish docs
ohimark --out site --theme default --stylesheet overrides.css docs
```

The normal flake app accepts the same flags; this uses the embedded preset and needs
no preset file in the working directory:

```sh
nix run .# -- --theme bookish --out site docs
```

`--theme` copies the selected preset's embedded bytes to `assets/theme.css`, so the
runtime does not need a `themes/` directory or preset file. When both options are
used, pages link the minimal base, then the preset, then `assets/custom.css`; each
link is relative to the page, including nested pages. A later rule wins when
specificity is equal. You can override the `--ohm-*` custom properties and match the
site's light and dark theme selectors. Presets apply presentation rules to common
elements; custom CSS can style native elements explicitly, as in this example:

```css
:root {
  --ohm-font-sans: "Trebuchet MS", sans-serif;
  --ohm-accent: #9c2c77;
  --ohm-link: var(--ohm-accent);
  --ohm-link-hover: #6d1c52;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --ohm-accent: #ec9dd1;
    --ohm-link: var(--ohm-accent);
    --ohm-link-hover: #f5c1e4;
  }
}
:root[data-theme="dark"] {
  --ohm-accent: #ec9dd1;
  --ohm-link: var(--ohm-accent);
  --ohm-link-hover: #f5c1e4;
}
body { font-family: var(--ohm-font-sans); }
a { color: var(--ohm-link); }
a:hover { color: var(--ohm-link-hover); }
```

The custom stylesheet is included as-is: ohimark does not process `url()` or
`@import`, copy their dependencies, or fetch them. Relative URLs in custom CSS are
resolved from `assets/custom.css`; include any referenced resources yourself if the
built site must work offline and remain portable. Rebuild the site after changing
the source CSS file.

## Navigation, titles, and ignoring

- Navigation mirrors the directory structure. A directory heading links to an immediate Markdown page whose stem matches the directory name (case-insensitive); if none exists, it links to an immediate case-insensitive `README.md` or `README.markdown` match. The same-name page has priority, and the selected page appears only on the heading, not again among child pages. Other pages remain reachable under the directory; without either candidate, it stays an unlinked group.
- By default, a directory's immediate pages appear first in case-insensitive filename order, followed by its subdirectories in the same order. With JavaScript enabled, a compact chooser appears beside any group with at least two immediate rows (pages or subgroups), including the top-level list. It sorts those rows together by displayed page title or group heading (natural, case-insensitive order), or by creation or modification time. A group's dates aggregate its descendant pages, including its linked landing page: creation uses the earliest known date and modification the latest known date; unknown dates are ignored, and a group with no known dates sorts as unknown. Unknown dates sort last. Moving a subgroup preserves its contents and its independent sort choice; the root page stays pinned. Each group's choice is remembered in `sessionStorage` per site for the current browsing session and survives navigation and reloads. Dates are captured when the site is built. Without JavaScript, navigation remains usable in the default order.
- With JavaScript enabled, each directory group's child list initially shows at most `--max-group-size` immediate page or subgroup rows. The limit is measured against rendered row heights, including theme styling and text wrapping; remaining links stay available by vertical scrolling. Independent top and bottom `...` cues show which direction has more content and update as the list scrolls, without moving its rows. Nested groups are limited independently, and sorting, reopening a group, or resizing recalculates the visible area. The directory heading is not counted as a child row. The root page stays pinned and the top-level navigation keeps its existing viewport scrolling. Without JavaScript, the full navigation remains available uncapped.
- Page titles: the first Markdown ATX heading (`^\s*#\s+...`, matched
  line-by-line, not markdown-aware) in the file; if none is found, the
  target's file stem.
- `.gitignore` and `.ignore` files are honored, and `.git` is always skipped.
- Two files that fold to the same target path (e.g. `Readme.md` and
  `readme.md` in the same directory) is an error.

## Documentation

- [Requirements](docs/requirements.md) — the behavioural specification, and the acceptance suite the implementation is built against.
- [Architecture](docs/architecture.md) — the components and how they interact.
- [Links & Assets](docs/design/links.md) — how relative references between pages are resolved.

## Development

```sh
cargo build
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings

nix build path:.
nix run path:. -- --help
nix flake check path:.
nix develop path:.
```

The repository's own Markdown can be rendered and served locally by `ohimark`
itself, with `README.md` as the site's home page:

```sh
nix build path:.#docs         # build the documentation site
nix run path:.#docs            # render once, serve on http://127.0.0.1:8080
nix run path:.#docs -- 9000    # the same, on port 9000
nix run path:.#watchdocs       # serve, re-rendering as files change
```

All three include a main **API** sidebar link to `cargo-docs.html`, a
landing page that redirects to `cargo-docs/ohimark/index.html` and provides a
fallback link. Nix bundles the complete rustdoc tree under `cargo-docs/` for this
repository's library API; dependency APIs are not included.

`docs` renders the working tree into a cache directory and serves it with
[`static-web-server`](https://static-web-server.net); `watchdocs` starts that
server and uses [`watchexec`](https://watchexec.github.io) to re-render on
every change. Both take the port to listen on as an optional argument,
defaulting to 8080; `nix run` needs `--` before it. The cache directory is
`$XDG_CACHE_HOME/ohimark/docs`, or `~/.cache/ohimark/docs` when that variable
is unset; it is reused across runs, and `OHIMARK_DOCS_STATE` overrides it.

Choose the Nix docs theme in `flake.nix`, in the argument set for the `docs`
output's `callPackage ./nix/docs.nix` call:

```nix
theme = "bookish";
```

Use `"default"` for the modern preset or `null` for native browser defaults. The
normal flake app accepts `--theme`; the configured theme is passed to both the `docs`
and `watchdocs` builders. Presets are embedded, so no theme file is needed at runtime.
The optional `stylesheet` argument remains available for a custom override, supplied as
an in-repository Nix path or a repository-relative string, and is linked after the preset.
This configuration controls `nix build path:.#docs`, `nix run path:.#docs`, and
`nix run path:.#watchdocs`. Cargo API docs are an immutable snapshot built by Nix.
Restart a running `docs` or `watchdocs` app after changing Rust sources, the configured
theme, or embedded preset CSS; a new `nix run` invocation rebuilds the executable,
Cargo API docs, or wrapper as needed. `watchdocs` still reflects Markdown and
configured custom stylesheet edits without a restart.

### Nightly toolchain and Polonius

This crate pins a dated nightly toolchain (`rust-toolchain.toml`, currently
`nightly-2026-09-15`) to use
[Polonius](https://rust-lang.github.io/polonius/), the next-generation
borrow checker, enabled via `-Zpolonius=next` in `.cargo/config.toml`'s
`build.rustflags`. Because `buildRustPackage`'s `cargo-auditable` wrapper
does not honor `.cargo/config.toml`'s `build.rustflags`, the same flag is
set again via `env.RUSTFLAGS` in `nix/package.nix` so the Nix build applies
it; the dev shell picks it up from `.cargo/config.toml` alone.

Nightly toolchains and unstable compiler flags are not covered by Rust's
stability guarantees. To bump the toolchain, update the `channel` date in
`rust-toolchain.toml`, then run `nix flake update rust-overlay --flake
path:.` (or `nix flake lock path:.` to refresh all inputs) so the pinned
`rust-overlay` revision knows about that nightly build.
