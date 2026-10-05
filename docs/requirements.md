# Requirements

A minimal, testable specification of what `ohimark` must do. Each requirement is
atomic, observable from outside the tool, and paired with a way to verify it.

## How to read this document

- **ID** — stable identifier. Cite it from test names and commit messages.
- Requirements use **must**. Anything optional or undecided lives under
  [Open questions](#open-questions) or [Non-requirements](#non-requirements)
  instead.
- Requirements are written against **observable behaviour**, never against the
  mechanism that produces it. Nothing here names a parser, a template engine, or
  a crate. That is deliberate: `ohimark` renders Markdown to HTML, CSS and JS
  itself, and this specification must outlive the internals of that renderer.
- Throughout, `DIR` is the positional root directory argument, paths are POSIX
  and relative to `DIR` unless stated otherwise, and "casefolded" means compared
  case-insensitively.

## R1 — Discovery

| ID | Requirement | How to verify |
| --- | --- | --- |
| R1.1 | Every file under `DIR` whose extension casefolds to `md` or `markdown`, and which survives the exclusion rules below, becomes exactly one page. | Fixture tree containing `a.md`, `B.MD` and `c.markdown`; assert three pages. |
| R1.2 | Ignore files are honoured at every level: `.gitignore`, `.ignore`, and `.git/info/exclude`. This holds even when `DIR` contains no `.git` directory. | Fixture tree with a `.gitignore` and no `.git`; assert the ignored file produces no page. |
| R1.3 | The `.git` directory is always skipped, at any depth, whether or not any ignore rule mentions it. | Place a `.md` file under `.git/` and under `nested/.git/`; assert no pages. |
| R1.4 | `--exclude GLOB` is repeatable. A file whose `DIR`-relative path matches any supplied glob is excluded, and so is everything under a directory that matches. | Pass two globs; assert each excludes its matches and that both accumulate. |
| R1.5 | Hidden files are eligible. A leading dot alone does not exclude a file. | Fixture with `.hidden.md`; assert it produces a page. |
| R1.6 | Discovery is deterministic: the same tree always yields the same page set in the same order, sorted by casefolded `DIR`-relative path. | Run discovery twice over one fixture; assert identical ordered output. |
| R1.7 | When the walk descends more than seven directory levels below `DIR`, a warning naming the deepest path is written to standard error once, suggesting a missing ignore rule. The build still succeeds. | Fixture nested eight levels deep; assert one warning on stderr and a zero exit. |
| R1.8 | Symbolic links are not followed, for either files or directories. | Fixture containing a symlink to a `.md` file and one to a directory outside `DIR`; assert neither contributes a page. |
| R1.9 | The output directory is never discovered as input, even when it lies inside `DIR`. | Build twice with `--out DIR/site`; assert the second build discovers no page from the first build's output. |

## R2 — Root file and page paths

| ID | Requirement | How to verify |
| --- | --- | --- |
| R2.1 | `--root`/`-r` names the site's home file as a path relative to `DIR`, matched casefolded. It defaults to `README.md` at the top level of `DIR`. A `--root` resolving outside `DIR` is a hard error with a non-zero exit. | Fixture with `readme.md`; assert it is chosen by default. Fixture with `--root docs/home.md`; assert it is chosen. Pass `--root ../outside.md`; assert a non-zero exit. |
| R2.2 | If the root file is absent, or excluded by an ignore rule or `--exclude`, the build fails with an error naming it, exits non-zero, and produces no output. | Build a fixture with no `README.md`, and one whose `README.md` is git-ignored; assert both fail with nothing written. |
| R2.3 | The root file is emitted as `index.html` at the top of the output. | Assert `<out>/index.html` exists and carries the root file's content. |
| R2.4 | Every other page is emitted at its own `DIR`-relative path with the extension replaced by `.html`, and no other renaming. `.md` and `.markdown` are treated identically. | Fixture with `a/b/c.md` and `a/b/d.markdown`; assert `a/b/c.html` and `a/b/d.html`. |
| R2.5 | Two sources whose target paths fold to the same value are a hard error. The error names both sources, the command exits non-zero, and no output is produced. | Fixture with `Guide.md` and `guide.md` in one directory; assert non-zero exit and both paths on stderr. |
| R2.6 | No emitted path escapes the output directory. | Assert every emitted path is relative and contains no `..` component. |
| R2.7 | When the root file comes from a subdirectory, its own relative links and image references are re-based from its original directory to the output root, so they continue to resolve. | `--root docs/home.md` for a page linking `guide.md` and embedding `img/d.png`; assert the built href is `docs/guide.html` and the built src is `docs/img/d.png`. |

## R3 — Page titles

| ID | Requirement | How to verify |
| --- | --- | --- |
| R3.1 | A page's title is the content of the first line matching a leading `#` followed by at least one space, trimmed of surrounding whitespace. | Fixture with `#   Spaced Title   `; assert the title is `Spaced Title`. |
| R3.2 | A page with no such line takes its title from its file stem. | Fixture with no heading; assert the title is the stem. |
| R3.3 | Heading detection is line-based, not Markdown-aware: a `#` line inside a fenced code block still counts. | Fixture whose first `#` line is inside a fence; assert that line becomes the title. |
| R3.4 | A title containing Markdown metacharacters such as `[` or `]` appears literally in the rendered navigation and in the page's `<title>`. | Fixture titled `[Weird] Title]`; assert both render the text exactly. |

## R4 — Navigation

| ID | Requirement | How to verify |
| --- | --- | --- |
| R4.1 | The sidebar mirrors the directory structure of the page set. Every directory containing at least one page, at any depth, appears as a node in its parent's node. | Fixture with `a/b/c.md`; assert nested nodes `a` and `b`. |
| R4.2 | When a directory contains a page whose file stem casefolds to the directory's own name, that node is a link to that page, and that page does not also appear as a child entry of the node. | Fixture with `docs/docs.md`; assert the `docs` node links to `docs/docs.html` and lists it once, as the node itself. |
| R4.3 | Otherwise the node is a group label with no link of its own. Its children remain reachable. | Fixture with `docs/guide.md` and no `docs/docs.md`; assert the `docs` node carries no href. |
| R4.4 | Within a node, the directory's own pages come first in casefolded ascending order, followed by its subdirectory nodes in casefolded ascending order. | Fixture with `docs/zebra.md`, `docs/Alpha.md`, `docs/guides/x.md`; assert the order `Alpha`, `zebra`, `guides`. |
| R4.5 | At the top level the root page is the first entry, before every other page and node. | Assert position one for a multi-page fixture. |
| R4.6 | Every page produced under R1 is reachable from the sidebar. | Cross-check the page set against the sidebar entries of the built site. |
| R4.7 | The sidebar is identical on every page, and the entry for the page currently being viewed is distinguishable from the others. | Diff the sidebar markup across two pages ignoring the current-page marker; assert equality and that the marker moves. |

## R5 — Supporting assets

| ID | Requirement | How to verify |
| --- | --- | --- |
| R5.1 | A non-Markdown file is copied into the output, byte-identically and at its `DIR`-relative path, when and only when a rendered page references it. | Fixture with `docs/used.png` referenced by a page and `docs/unused.png` referenced by nothing; assert the first is present and byte-identical and the second is absent. |
| R5.2 | A reference to a file that does not exist, or that an ignore rule or `--exclude` removed, produces a warning on standard error naming the referencing page and the target. Nothing is copied, the reference is left unchanged, and the build succeeds. | Reference a missing image; assert one warning, a zero exit, and an unchanged reference. |
| R5.3 | A reference that resolves outside `DIR` is never copied. It produces a warning and is left unchanged. | Reference `../outside.png`; assert nothing is written outside the output and one warning is emitted. |
| R5.4 | An asset referenced by more than one page is copied exactly once. | Reference one image from two pages; assert a single copy at one path. |

## R6 — Links

| ID | Requirement | How to verify |
| --- | --- | --- |
| R6.1 | A relative link from one page to another page's source file resolves, in the built site, to that page's rendered `.html` target. | Fixture linking `docs/guide.md`; assert the built href is `docs/guide.html`. |
| R6.2 | A relative link to the root file resolves to `index.html`. | Fixture linking `../README.md` from a subdirectory; assert the built href is `../index.html`. |
| R6.3 | A rewritten link preserves its `?query` and `#fragment` unchanged. | Link `guide.md?x=1#frag`; assert `guide.html?x=1#frag`. |
| R6.4 | Percent-encoded paths are decoded for matching and re-encoded on output, never double-encoded. | Link `my%20file.md` to a page named `my file.md`; assert `my%20file.html`. |
| R6.5 | Links that are external, root-absolute, protocol-relative, pure fragments, `mailto:` or `data:` are left byte-for-byte unchanged. | Assert each form is identical before and after. |
| R6.6 | A relative link to a Markdown file that is not a page in the site produces a warning on standard error naming the referencing page and the target, and the href is left byte-for-byte unchanged. The build succeeds. | Link to `nowhere.md`; assert one warning, an untouched href, and a zero exit. |
| R6.7 | `..` traversal is resolved lexically against the linking page's own source directory, with no filesystem or symlink resolution. | Link `../b.md` from `a/sub/`; assert it resolves to `a/b.html`. |
| R6.8 | Every rendered heading carries an anchor identifier that is stable across builds. When two headings on one page derive the same identifier, every occurrence after the first takes a numeric suffix, so each heading stays individually addressable. Identical identifiers on different pages are not a conflict. | Build twice; assert the identifiers are unchanged. Fixture with two `## Usage` headings; assert `usage` and `usage-1`. Fixture with `## Usage` on two pages; assert both are `usage`. |
| R6.9 | Image sources are resolved by the same rules as links, and a resolved image counts as a reference for R5.1. | Fixture with `![d](../img/d.png)` from a subdirectory; assert the built `src` and that the file is copied. |

## R7 — Markdown rendering

| ID | Requirement | How to verify |
| --- | --- | --- |
| R7.1 | Basic Markdown syntax renders to its conventional HTML: headings, emphasis and strong emphasis, ordered and unordered lists including nesting, blockquotes, inline code, fenced and indented code blocks, horizontal rules, links, images, and hard line breaks. | One fixture per construct; assert the expected element appears. |
| R7.2 | Tables render as HTML tables, including per-column alignment. | Fixture with a left/centre/right aligned table; assert the cells and their alignment. |
| R7.3 | A fenced code block renders as `<pre><code>` carrying a language class derived from its info string, with its content HTML-escaped and otherwise verbatim. No syntax highlighting is required. | Fixture fence tagged `rust` containing `<script>`; assert the class, the escaping, and that the text is unaltered. |
| R7.4 | A fenced code block tagged `mermaid` is rendered to an inline SVG diagram at build time, with no browser, Node.js or JavaScript runtime involved, and the diagram's source is retained in the page. | Build a fixture containing one; assert an inline `<svg>` is present, that no literal `<code>` block remains, and that the source text is retained. |
| R7.5 | Raw inline and block HTML in a Markdown source passes through to the output verbatim. | Fixture containing a `<div>` and an inline `<kbd>`; assert both appear unescaped. |
| R7.6 | Every page is a complete HTML document declaring its language, a UTF-8 charset, a viewport, and a `<title>` derived from R3. | Assert each of the four on a built page. |
| R7.7 | A `mermaid` block that fails to render produces a warning on standard error naming the page, and falls back to rendering as an ordinary fenced code block. The build succeeds. | Fixture with a malformed diagram; assert one warning, a zero exit, and a `<pre><code>` fallback. |
| R7.8 | A list item whose content begins with `[ ]` or `[x]` renders as a task-list item: a checkbox reflecting the marker's state, read-only so a reader cannot change it, with the item's own list marker suppressed. Task items nest, may appear in ordered lists, and may sit alongside plain items in the same list. | Fixture with `- [x] a`, `- [ ] b`, a nested `- [x] c`, `1. [ ] d` and a plain `- e`; assert a checked and an unchecked checkbox, that each carries `disabled`, and that the plain item has none. Load the page; assert no bullet or number is shown beside a task item. |

## R8 — Theme and client behaviour

| ID | Requirement | How to verify |
| --- | --- | --- |
| R8.1 | The site offers a dark/light theme toggle. The chosen theme persists across page navigations and reloads for the remainder of the browsing session; on a first visit, and in a new session, the operating system preference is honoured. | Toggle, navigate and reload within one session; assert the theme holds. Start a fresh session; assert the system preference decides. |
| R8.2 | Diagrams carry no theme of their own: each is rendered once, with its colours expressed as CSS custom properties, so it matches whichever theme is active without anything being rendered in the browser. | Toggle the theme on a diagram page with scripting disabled; assert the diagram's colours change. |
| R8.3 | Each page shows an outline of its own headings, each linking to the corresponding anchor. A page with no headings shows no outline. A heading containing a link contributes its text to the outline without that link, so each outline entry is a single anchor rather than one nested inside another. | Assert one outline entry per heading and a working anchor; assert the outline is absent for a headingless page. Fixture a heading containing a link; assert the outline entry carries the link's text with no nested anchor. |
| R8.4 | The left navigation and right outline panels can collapse independently to control rails on desktop, giving the main area more space and comfortable horizontal padding. At narrow widths the left panel behaves as an off-canvas drawer, and the right outline remains hidden at its responsive breakpoint. Both panel controls work without JavaScript. | At desktop width, collapse each panel separately and assert the other stays open while the main area gains room and padding. At narrow width, assert the left drawer opens and closes and the right outline remains hidden. Repeat with JavaScript disabled. |
| R8.5 | Each sidebar group's expanded or collapsed state persists independently across page navigations and reloads for the browsing session using `sessionStorage`. Groups default to expanded in a fresh session and remain usable through native controls if JavaScript or storage is unavailable. | Collapse different groups, navigate and reload within one session; assert each group retains its state independently. Start a fresh session; assert all groups are expanded. Disable JavaScript or storage; assert native group controls remain usable. |
| R8.6 | With JavaScript enabled, each diagram offers an icon toggle between its rendered graphic and its source. The toggle applies to one diagram at a time and resets on navigation. | Toggle one of two diagrams to source view; assert only that one changed, then navigate away and back and assert both show graphics. |
| R8.7 | The built site renders completely with no network access: every stylesheet, script, font and image it loads is served from the output itself, and no diagram-rendering runtime is shipped at all. | Grep the output for `http://` and `https://` in `src`, `href` and `url()` positions; assert none. Load a diagram page with networking disabled; assert it renders. |
| R8.8 | With JavaScript disabled, a page's content, its diagrams, its sidebar links and its outline links remain readable and usable. | Load a page with scripting off; assert the content, diagrams, sidebar and outline are present and navigable. |
| R8.9 | A diagram scales to the width of its container without JavaScript, and never makes the page scroll horizontally. | Load a diagram page 400px wide with scripting disabled; assert the diagram fits and the page has no horizontal scroll. |
| R8.10 | With JavaScript enabled, every fenced code block and every diagram offers an icon copy control that puts the block's source text verbatim on the clipboard — for a diagram, its Mermaid source, not the rendered SVG. The reader gets visible confirmation that the copy happened. With JavaScript disabled, the control is absent, never present but inert. | Click the copy icon on a code block and on a diagram with scripting enabled; assert the clipboard holds the exact source text and a confirmation appears. Load the same page with scripting disabled; assert no copy control is present. |

## R9 — Command-line surface

| ID | Requirement | How to verify |
| --- | --- | --- |
| R9.1 | The invocation is `ohimark --out OUTDIR [--root FILENAME] [--exclude GLOB]... [--todo] DIR`. Both the positional root directory and `--out` are required; omitting either is an error with a non-zero exit. | Run with no positional argument, then with no `--out`; assert both exit non-zero with a usage message. |
| R9.2 | The positional `DIR` must resolve to an existing directory. One that does not is an error with a non-zero exit. | Pass a nonexistent path and a path to a regular file; assert both fail. |
| R9.3 | The output directory is refused when it exists and is not empty: the command exits non-zero and leaves the directory's existing contents untouched. | Pre-populate it; assert a non-zero exit and unchanged contents. |
| R9.4 | The output directory is created when it does not exist, and accepted when it exists and is empty. | Assert both cases succeed. |
| R9.5 | `--help` exits zero and describes the tool, the positional argument, and every flag. | Assert a zero exit and the presence of each flag name. |
| R9.6 | Flags are accepted before or after the positional argument. `--exclude` accumulates across occurrences; for `--root` and `--out` the last occurrence wins. | Parse each arrangement; assert the resolved values. |

## R10 — Process behaviour

| ID | Requirement | How to verify |
| --- | --- | --- |
| R10.1 | No temporary or intermediate file is ever written under `DIR`. The output directory is the only thing `ohimark` may write there, and only when `--out` points inside `DIR`. | Snapshot `DIR` before and after a successful build and a failed one; assert equality apart from the output directory. |
| R10.2 | The site is assembled outside `DIR` and placed at the output path only once it is complete, so a failed build never leaves a partially written output directory. | Force a mid-build failure; assert the output directory is either absent or empty. |
| R10.3 | No temporary directory survives the process, on success or on failure. | Assert the temporary path is gone after each case. |
| R10.4 | Any failure reports a message on standard error naming the failing stage, and exits non-zero. | Force a failure at each stage; assert a non-zero exit and a stderr message naming the stage. |
| R10.5 | Warnings are written to standard error only, never to standard output, and never change the exit code. | Trigger the R1.7, R5.2 and R6.6 warnings; assert an empty stdout and a zero exit. |

## R11 — Output properties

| ID | Requirement | How to verify |
| --- | --- | --- |
| R11.1 | The built site is relocatable: no absolute path of `DIR`, of the output directory, or of any temporary directory appears anywhere in it, and every internal link is relative. | Grep the output for each path; assert no matches and no root-absolute internal href. |
| R11.2 | The built site is usable both when served by any static file server and when opened directly from disk over `file://`. It requires no server-side logic, rewriting rules, or redirects. | Serve it and assert every page and asset loads; then open `index.html` from disk and navigate to a nested page. |

## R12 — Task collection

Throughout, the "root TODO file" is a Markdown file directly in `DIR` (not in a
subdirectory) whose name casefolds to `todo.md` or `todo.markdown`.

| ID | Requirement | How to verify |
| --- | --- | --- |
| R12.1 | `--todo` is off by default. Without it no task is collected and no TODO page is generated: a root TODO file renders like any other page. | Build a fixture with open tasks and no flag; assert no `TODO.html` and no `Collected Tasks` section in an existing `TODO.md`. Assert `--help` lists `--todo`. |
| R12.2 | With `--todo`, every unchecked task-list item on every page is collected, with its own content. Checked items are not collected. A checked item nested inside a collected one is dropped with everything under it; unchecked and plain items nested inside a collected one are kept with it. An unchecked item nested at any depth inside a checked one is collected on its own — including one under a checked item dropped from a collected one, which follows that collected item — so every unchecked item is collected exactly once, in source order. Content is collected as Markdown, not as its page renders it: a `mermaid` fence inside a collected task shows as an ordinary code block, not a diagram. | Fixture with an open item holding a checked child (itself holding an open grandchild), an open child and a plain child, and a checked item holding an open child; assert exactly the open items and the plain child appear, each once and in source order. Fixtures with a loose list and with a code block inside a task; assert both are collected intact. |
| R12.3 | With `--todo` and no root TODO file, a page `TODO.html` is generated whose title and first heading are `TODO`, carrying the collected tasks. It appears in the sidebar like any other top-level page. A root TODO file that is excluded, ignored or unreadable counts as absent, so the page is generated and that file's tasks are not collected. If a file in `DIR` already sits at `TODO.html`, the build fails at planning with an error naming it, and produces no output, rather than overwriting it. | Build with `--todo` and no `TODO.md`; assert `TODO.html`, its title, and a sidebar link to it. Repeat with `--exclude TODO.md`; assert the same. Add a `TODO.html` file; assert a non-zero exit naming it and no output. |
| R12.4 | With `--todo` and a root TODO file, that file renders as usual followed by a `Collected Tasks` section carrying the collected tasks; the file's own tasks are not collected into it. The file is matched casefolded with either Markdown extension, and this holds when it is also the `--root` file, emitted as `index.html`. A construct left open at the end of the file, such as an unclosed fence or HTML block, does not swallow the section, and heading anchors stay unique across the file and the section (R6.8). | Build a fixture with a `TODO.md` holding an open task; assert the section follows its content and the task appears once. Repeat with `Todo.md`, `TODO.markdown` and `--root TODO.md`. End `TODO.md` in an unclosed fence, and with its own `# Collected Tasks` heading; assert the section still renders, with the anchor `collected-tasks-1`. |
| R12.5 | Collected tasks are listed by file, in page order (R1.6), each file's tasks under a second-level heading linking to that file's page. There are no directory headings. Files with no open task are omitted. With no open task anywhere, the TODO page reads `No open tasks.` | Fixture with tasks at the top level and in `guides/`; assert the file order, an h2 file link per file with open tasks, and no directory headings. Fixture with no open task; assert the message. |
| R12.6 | A link or image inside a collected task resolves to the same target as on its own page, re-based to the TODO page, and an image so referenced counts as a reference for R5.1. A pure fragment link points at the task's own page; a link to a missing file keeps the path written, re-based from the task's own directory; a footnote reference, whose definition is not collected, is kept as plain text. | Task in `guides/api/ref.md` linking `../install.md` and embedding `img/d.png`; assert `guides/install.html`, `guides/api/img/d.png`, and the copied file. Task in `guides/a.md` linking `#notes` and `gone.md` with a footnote reference; assert `guides/a.html#notes`, `guides/gone.md` and the reference as text. |
| R12.7 | The collected tasks exist only in the built output. Nothing is written under `DIR`, and no generated `TODO.md` is written anywhere (R10.1). | Snapshot `DIR` around a `--todo` build, with and without a root TODO file; assert equality. |

## Non-requirements

The tool is not required to:

- serve the output over HTTP, watch for changes, live-reload, or rebuild incrementally — building is a single shot, and serving and watching are composed around it by other tools;
- mount additional pre-built HTML trees at extra endpoints;
- provide full-text search;
- apply syntax highlighting to code blocks — R7.3 emits a language class so this stays additive;
- support front matter, page weights, or any manual ordering mechanism — the directory tree and R4 determine the order;
- check external links for reachability, or validate Markdown;
- report a Markdown file that no other page links to — orphan pages are normal;
- preserve the mtimes or ownership of copied files;
- support Windows path semantics; paths are treated as POSIX throughout;
- produce multi-language or versioned output;
- produce byte-identical output across builds of an unchanged tree — rebuild identity is not promised, which is what allows the diagram renderer to be upgraded freely.

## Traceability

The suites currently in `tests/` were written against the previous, externally
rendered pipeline and do not yet cover this specification. No requirement above
should be considered verified until a test cites its ID.
