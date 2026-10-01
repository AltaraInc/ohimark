# ohimark

`ohimark` builds markdown documentation into static HTML, CSS and JS. The tool walks the directory structure from the provided root directory collecting all markdown files (while honoring the provided ignore files; e.g. `.gitignore`) and renders each markdown file to its' respective HTML page.

The directory structure is respected while conducting the directory traversal; directories become anchoring points for all its' sub-directories and files. If a markdown file with the same name as the directory (case-insensitive) is found within the directory, then that file acts as the root page instead.

## Rendering

Each markdown file that is collected during the directory traversal will be translated into its' own HTML page. All the [basic syntax](https://www.markdownguide.org/basic-syntax), tables, [task lists](https://www.markdownguide.org/extended-syntax/#task-lists) and mermaid diagrams will be supported.

Mermaid diagrams are rendered to SVG at build time by [`merman`](https://github.com/Latias94/merman), a headless Rust implementation of Mermaid. Nothing is fetched while building and no diagram runtime is shipped to the browser, so diagrams are visible even with JavaScript disabled.

Task list items (`- [ ]` and `- [x]`) render as checkboxes reflecting the marker in the source. They are read-only: the rendered page always matches the Markdown it was built from, and ticking a box in a browser is not a way to edit the file.

## Usage

`ohimark` requires a root directory to start from and a directory to write to. It optionally accepts a root file (defaulting to `README.md`), exclusion globs, and a flag to collect open tasks.

```sh
ohimark --out OUTDIR [--root FILENAME] [--exclude GLOB]... [--todo] DIR
```

- `--out` or `-o`: directory to write the generated static files to. Required; it must be empty or not yet exist.
- `--root` or `-r`: the site's home page, given as a path relative to `DIR`. Defaults to `README.md`. It is emitted as `index.html`.
- `--exclude`: a glob of paths to skip, in addition to the ignore files. Repeatable.
- `--todo`: collect every unchecked task list item (`- [ ]`) from all pages into the root-level TODO page, under a heading linking each file, in page order. If `DIR` has a top-level `TODO.md`, the tasks are appended to it under a `Collected Tasks` heading and its own tasks are not repeated; otherwise a `TODO.html` page is generated. This only affects the HTML output: nothing is written to `DIR`. Off by default.

## Navigation, titles, and ignoring

- Navigation mirrors the directory structure. A directory holding a markdown file whose name matches the directory (case-insensitively) becomes a link to that page; otherwise it is a group label with no page of its own.
- Within a directory, its own pages come first in case-insensitive ascending order, followed by its subdirectories in the same order.
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
