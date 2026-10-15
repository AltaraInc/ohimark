# ohimark

`ohimark` builds markdown documentation into static HTML, CSS and JS. The tool walks the directory structure from the provided root directory collecting all markdown files (while honoring the provided ignore files; e.g. `.gitignore`) and renders each markdown file to its' respective HTML page.

The directory structure is reflected in navigation: directories become groups for their pages and subdirectories. A directory heading links to its own same-name Markdown page when one exists (case-insensitive); otherwise, it links to a case-insensitive `README.md` or `README.markdown` match. The selected page appears on the heading instead of again among the directory's child pages.

## Rendering

Each markdown file that is collected during the directory traversal will be translated into its' own HTML page. All the [basic syntax](https://www.markdownguide.org/basic-syntax), tables, [task lists](https://www.markdownguide.org/extended-syntax/#task-lists) and mermaid diagrams will be supported.

Mermaid diagrams are rendered to SVG at build time by [`merman`](https://github.com/Latias94/merman), a headless Rust implementation of Mermaid. Nothing is fetched while building and no diagram runtime is shipped to the browser, so diagrams are visible even with JavaScript disabled.

Task list items (`- [ ]` and `- [x]`) render as checkboxes reflecting the marker in the source. They are read-only: the rendered page always matches the Markdown it was built from, and ticking a box in a browser is not a way to edit the file.

## Usage

`ohimark` requires a root directory to start from and a directory to write to. It optionally accepts a root file (defaulting to `README.md`), exclusion globs, a built-in theme, a custom stylesheet, and a flag to collect open tasks.

```sh
ohimark --out OUTDIR [--root FILENAME] [--exclude GLOB]... [--todo] [--theme default|bookish] [--stylesheet FILE] DIR
```

- `--out` or `-o`: directory to write the generated static files to. Required; it must be empty or not yet exist.
- `--root` or `-r`: the site's home page, given as a path relative to `DIR`. Defaults to `README.md`. It is emitted as `index.html`.
- `--exclude`: a glob of paths to skip, in addition to the ignore files. Repeatable.
- `--todo`: collect every unchecked task list item (`- [ ]`) from all pages into the root-level TODO page, under a heading linking each file, in page order. If `DIR` has a top-level `TODO.md`, the tasks are appended to it under a `Collected Tasks` heading and its own tasks are not repeated; otherwise a `TODO.html` page is generated. When there are no open tasks to collect, nothing is added and no `TODO.html` is generated. This only affects the HTML output: nothing is written to `DIR`. Off by default.
- `--theme default|bookish`: use an embedded visual preset. If repeated, the last value is used; other values are rejected.
- `--stylesheet FILE`: add a stylesheet after the minimal bundled base and any selected preset. The file must be an existing, readable regular file; its supplied and canonical paths must be UTF-8. Its contents are copied unchanged. It may be inside or outside `DIR`, including a file ignored or excluded from discovery. Relative paths are resolved from the current working directory; absolute paths are also accepted. If repeated, the last value is used.

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
nix run path:.#docs            # render once, serve on http://127.0.0.1:8080
nix run path:.#docs -- 9000    # the same, on port 9000
nix run path:.#watchdocs       # serve, re-rendering as files change
```

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
`nix run path:.#watchdocs`. Restart a running `docs` or `watchdocs` app after changing
the configured theme or embedded preset CSS; a new `nix run` invocation rebuilds the
immutable executable or wrapper as needed. `watchdocs` still reflects Markdown and
configured custom stylesheet edits.

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
