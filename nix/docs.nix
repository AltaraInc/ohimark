{ lib
, runCommand
, writeShellApplication
, writeTextDir
, coreutils
, findutils
, flock
, git
, static-web-server
, watchexec
, pname
, package
, theme ? null
, stylesheet ? null
}:

let
  manifest = lib.importTOML ../Cargo.toml;
  crateName = lib.replaceStrings [ "-" ] [ "_" ] (manifest.lib.name or manifest.package.name);

  # Reuse the package's source, pinned compiler, vendored dependencies and
  # RUSTFLAGS. Override installation too, so Cargo's hooks do not try to
  # install a binary from this documentation-only build.
  cargoDocs = package.overrideAttrs (_: {
    pname = "${pname}-cargo-docs";
    doCheck = false;
    buildPhase = ''
      runHook preBuild
      cargo doc --offline --locked --no-deps --lib
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      mkdir -p "$out"
      cp -R target/doc/. "$out/"
      runHook postInstall
    '';
  });

  # `--endpoint` copies one file. A stable filename avoids both index.html's
  # home-page collision and a store hash appearing in the sidebar URL.
  cargoDocsLanding = writeTextDir "cargo-docs.html" ''
    <!DOCTYPE html>
    <html lang="en">
    <head>
      <meta charset="utf-8">
      <meta name="viewport" content="width=device-width, initial-scale=1">
      <meta http-equiv="refresh" content="0; url=cargo-docs/${crateName}/index.html">
      <title>Cargo docs</title>
    </head>
    <body>
      <p><a href="cargo-docs/${crateName}/index.html">Open Cargo docs</a></p>
    </body>
    </html>
  '';

  # `$site` is the completed Markdown render. Never merge rustdoc with an
  # existing path; its entire tree, including shared assets, belongs here.
  bundleCargoDocs = ''
    conflict="$(find "$site" -mindepth 1 -maxdepth 1 -iname cargo-docs -print -quit)"
    if [ -n "$conflict" ]; then
      echo "${pname}: cannot bundle Cargo docs: $conflict already exists" >&2
      exit 1
    fi
    mkdir -p "$site/cargo-docs"
    cp -R ${cargoDocs}/. "$site/cargo-docs/"
    # Store directories are read-only; live rebuilds must be able to remove
    # the previous bundle when replacing the served site's contents.
    chmod -R u+w "$site/cargo-docs"
  '';

  # Optional embedded theme preset. `null` leaves ohimark on its native
  # default; the stylesheet argument below remains an optional custom file.
  themeOption =
    if theme == null then
      ""
    else if builtins.elem theme [ "default" "bookish" ] then
      " --theme ${lib.escapeShellArg theme}"
    else
      throw "unsupported docs theme; expected null, 'default', or 'bookish'";
  renderCommand = lib.concatStringsSep " " [
    "${lib.getExe package}${themeOption}"
    "--todo"
    "--endpoint" (lib.escapeShellArg "API") (lib.escapeShellArg "${cargoDocsLanding}/cargo-docs.html")
  ];

  # Optional stylesheet applied to both docs outputs. Accept a Nix path within
  # this repository or a repo-relative string such as `styles/overrides.css`.
  stylesheetRelative =
    if stylesheet == null then
      null
    else if builtins.isPath stylesheet then
      let
        repoPrefix = "${toString ../.}/";
        selectedPath = toString stylesheet;
      in
      if lib.hasPrefix repoPrefix selectedPath then
        lib.removePrefix repoPrefix selectedPath
      else
        throw "docs stylesheet path must be inside this repository"
    else
      stylesheet;
  stylesheetOption =
    if stylesheetRelative == null then
      ""
    else let
      source = builtins.path {
        path = ../. + "/${stylesheetRelative}";
        name = "${pname}-docs.css";
      };
    in
    " --stylesheet ${lib.escapeShellArg "${source}"}";
  stylesheetRepoOption =
    if stylesheetRelative == null then
      ""
    else
      " --stylesheet \"$repo\"/${lib.escapeShellArg stylesheetRelative}";

  # The Markdown sources this site renders. Listed explicitly rather than
  # handing `ohimark` the whole repository: this fileset decides what reaches
  # the build sandbox, so a broader one would copy `target/` into the store
  # and rebuild the site on unrelated source changes. New top-level Markdown
  # has to be added here to appear in `packages.docs`; the `docs` and
  # `watchdocs` apps read the working tree directly and need no such list.
  docsRoot = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../README.md
      ../TODO.md
      ../docs
    ];
  };

  # `ohimark`'s own documentation site: this repository's Markdown, rendered
  # by the very tool it describes ("dogfooding"). `--todo` collects the open
  # task-list items into the site's `TODO.html` page.
  docs = runCommand "${pname}-docs" { nativeBuildInputs = [ findutils ]; } ''
    ${renderCommand}${stylesheetOption} --out $out ${docsRoot}
    site="$out"
    ${bundleCargoDocs}
  '';

  # The wrappers below serve the *working tree* rather than the sealed `docs`
  # derivation above, so an edit is visible without a Nix rebuild. Both take
  # the port to listen on as an optional positional argument, and agree on
  # what to render through the environment:
  #
  #   OHIMARK_DOCS_REPO   working tree to render (default: the git toplevel)
  #   OHIMARK_DOCS_STATE  cache directory holding the served site
  #
  # `OHIMARK_DOCS_STATE` defaults to a stable path reused across runs. Each
  # render replaces its site's contents; concurrent servers can use distinct
  # state directories.

  # `PORT` handling, shared so both wrappers present the same command line:
  # `NAME [PORT]`, defaulting to 8080. Validated here rather than left to
  # static-web-server so that `watchdocs`, which only passes the port on to
  # the server it backgrounds, still rejects a bad one up front.
  portArg = name: ''
    if [ "$#" -gt 1 ]; then
      echo "usage: ${name} [PORT]" >&2
      exit 2
    fi
    port="''${1-8080}"
    case "$port" in
      "" | *[!0-9]*)
        echo "${name}: PORT must be a number, got '$port'" >&2
        exit 2
        ;;
    esac
  '';

  # One render of the working tree into `$OHIMARK_DOCS_STATE/site`.
  build = writeShellApplication {
    name = "${pname}-docs-build";
    runtimeInputs = [ coreutils findutils flock git ];
    text = ''
      repo="''${OHIMARK_DOCS_REPO:-$(git rev-parse --show-toplevel)}"
      state="''${OHIMARK_DOCS_STATE:?must be set by ${pname}-docs}"

      # Initial renders and watcher updates share the same staging directory.
      # The descriptor lock is released automatically when the build exits.
      mkdir -p "$state"
      exec 9>"$state/build.lock"
      flock -x 9

      # `--out` has to be empty or absent, so each render goes to a fresh
      # staging directory before replacing the served directory's contents:
      # static-web-server resolves its root once at startup, so that directory
      # has to keep its identity across rebuilds.
      mkdir -p "$state/site"
      rm -rf "$state/next"
      # Configured stylesheet paths are literal filenames, including dollar signs.
      # shellcheck disable=SC2016
      ${renderCommand} --out "$state/next" --root README.md "$repo"${stylesheetRepoOption}
      site="$state/next"
      ${bundleCargoDocs}
      find "$state/site" -mindepth 1 -delete
      cp -R "$state/next/." "$state/site/"
      rm -rf "$state/next"

      echo "${pname}: rendered $repo into $state/site" >&2
    '';
  };

  # `nix run .#docs [-- PORT]`: render the working tree once, then serve it.
  serve = writeShellApplication {
    name = "${pname}-docs";
    runtimeInputs = [ build git static-web-server ];
    text = ''
      ${portArg "${pname}-docs"}
      OHIMARK_DOCS_REPO="''${OHIMARK_DOCS_REPO:-$(git rev-parse --show-toplevel)}"
      OHIMARK_DOCS_STATE="''${OHIMARK_DOCS_STATE:-''${XDG_CACHE_HOME:-$HOME/.cache}/${pname}/docs}"
      export OHIMARK_DOCS_REPO OHIMARK_DOCS_STATE

      ${pname}-docs-build

      echo "${pname}: serving http://127.0.0.1:$port" >&2
      # Cache headers are on by default, which lets a browser hold a stale page
      # or stylesheet across a rebuild — exactly what `watchdocs` exists to
      # avoid, and misleading even for a one-shot `docs` run after an edit.
      exec static-web-server --host 127.0.0.1 --port "$port" \
        --cache-control-headers false \
        --root "$OHIMARK_DOCS_STATE/site"
    '';
  };

  # `nix run .#watchdocs [-- PORT]`: the server above, plus a re-render on
  # every change.
  watch = writeShellApplication {
    name = "${pname}-watchdocs";
    runtimeInputs = [ build git serve watchexec ];
    text = ''
      ${portArg "${pname}-watchdocs"}
      OHIMARK_DOCS_REPO="''${OHIMARK_DOCS_REPO:-$(git rev-parse --show-toplevel)}"
      OHIMARK_DOCS_STATE="''${OHIMARK_DOCS_STATE:-''${XDG_CACHE_HOME:-$HOME/.cache}/${pname}/docs}"
      export OHIMARK_DOCS_REPO OHIMARK_DOCS_STATE

      # The watcher depends on the server: start it in the background (it
      # performs the first render), then re-render on change. `--postpone`
      # suppresses watchexec's own run at startup, which that first render
      # already covered; its default ignores honor `.gitignore`/`.ignore`,
      # and the scratch directory lives outside the tree, so neither feeds
      # the watcher its own output.
      ${pname}-docs "$port" &
      server=$!
      trap 'kill "$server" 2>/dev/null || true' EXIT INT TERM

      watchexec \
        --watch "$OHIMARK_DOCS_REPO" \
        --debounce 200ms \
        --postpone \
        --on-busy-update restart \
        -- ${pname}-docs-build
    '';
  };
in
{
  inherit docs serve watch;
}
