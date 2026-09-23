# TODO

Behaviour is specified in [docs/requirements.md](docs/requirements.md); the component
layout is in [docs/architecture.md](docs/architecture.md). Requirement IDs below point at
the former.

## Implementation

[ ] `model`: core types — SourceRel, TargetPath, Page, Site, NavNode — plus errors and the warning sink
[ ] `cli`: argv to a validated Config; --out required, --root relative to DIR, repeatable --exclude (R9)
[ ] `plan`: ignore-aware walk, --exclude overrides, depth-7 warning, output-directory exclusion (R1)
[ ] `plan`: root-file resolution, source-to-target map, collision detection, titles, nav tree (R2, R3, R4)
[ ] `links`: relative reference resolution, referenced-asset registration, POSIX path helpers (R5, R6)
[ ] `render`: pulldown-cmark event pass — headings and anchors, outline, code fences, raw HTML (R3, R6.8, R7)
[ ] `render`: Mermaid via merman 0.8.0-alpha.6 — one SVG per diagram, colours rewritten to CSS custom properties, code-block fallback on failure (R7.4, R7.7, R8.2)
[ ] `theme`: page shell, sidebar, outline rail, dark/light toggle, responsive nav, diagram view toggle, diagram scaling (R8)
[ ] `emit`: temp staging outside DIR, referenced-asset copy, move into --out on success (R10, R11)
[ ] Parallelise the read-and-title and render phases with rayon, keeping page order and warning order deterministic (R11.3)
[ ] Remove the mdBook-era modules: stage, summary, mermaid, apidocs, serve, watch, and src/assets/mermaid-init.js
[ ] Update flake.nix: packages.docs, apps.docs and apps.docs-watch still invoke removed subcommands
[ ] Retire the tests written against the old pipeline and add tests citing requirement IDs

## Deferred

[ ] Site-wide search
[ ] Syntax highlighting for code blocks

## Open

[ ] Whether a merman version bump that changes SVG output is an acceptable break of byte-identical rebuilds (R11.3)
