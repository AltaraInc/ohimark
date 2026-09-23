{ lib, runCommand, pname, package }:

let
  # `cargo doc`'s own crate documentation for `package`, mounted at /api/ by
  # the site build below. Built by overriding the same package derivation so
  # it reuses its already-resolved dependency graph and lockfile.
  apiDocs = package.overrideAttrs (_old: {
    pname = "${pname}-api-docs";
    doCheck = false;

    # The base package's `postInstall` wraps `$out/bin/${pname}` with
    # `wrapProgram`, but this derivation's `installPhase` below never
    # produces a `bin/` at all -- it only copies `cargo doc`'s HTML output.
    # Disable it so `wrapProgram` does not fail on a missing binary.
    postInstall = "";

    buildPhase = ''
      runHook preBuild
      RUSTDOCFLAGS="-D warnings" cargo doc --offline --no-deps
      runHook postBuild
    '';

    installPhase = ''
      runHook preInstall
      mkdir -p $out
      cp -R target/doc/. $out/
      crateDir=$(printf '%s' "${pname}" | tr - _)
      cat > $out/index.html <<REDIRECT
      <!DOCTYPE html>
      <meta http-equiv="refresh" content="0; url=$crateDir/index.html">
      REDIRECT
      runHook postInstall
    '';
  });

  # The Markdown sources this site renders: the repository's README and
  # docs/*.md. Narrower than nix/package.nix's fileset, which only needs the
  # crate sources.
  docsRoot = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../README.md
      ../docs
    ];
  };

  # `merdoc`'s own documentation site: this repository's Markdown, rendered
  # by the very tool it describes ("dogfooding"), with the `cargo doc` API
  # reference mounted at /api/.
  docs = runCommand "${pname}-docs" { } ''
    ${lib.getExe package} --project-name ${pname} --root ${docsRoot} --api-docs ${apiDocs} build --out $out
  '';
in
{
  inherit apiDocs docs;
}
