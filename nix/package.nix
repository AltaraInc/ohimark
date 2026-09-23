{ lib, rustPlatform, makeWrapper, mdbook, mdbook-mermaid, static-web-server, watchexec }:

let
  manifest = lib.importTOML ../Cargo.toml;
  pname = manifest.package.name;
  version = manifest.package.version;
  runtimeTools = [ mdbook mdbook-mermaid static-web-server watchexec ];
in
rustPlatform.buildRustPackage {
  inherit pname version;

  cargoLock.lockFile = ../Cargo.lock;
  doCheck = true;
  strictDeps = true;

  # `.cargo/config.toml`'s `build.rustflags` is not honored by
  # `cargo-auditable` (used internally by `buildRustPackage`), so the
  # Polonius flag is set again here via RUSTFLAGS to reach the Nix build.
  env.RUSTFLAGS = "-Zpolonius=next";

  # `mdbook`, `mdbook-mermaid`, `static-web-server`, and `watchexec` are not
  # linked dependencies; they are external tools the binary shells out to at
  # runtime (`build`/`serve`/`watch`), so they are provided via a wrapped
  # `PATH` rather than `buildInputs`.
  nativeBuildInputs = [ makeWrapper ];
  nativeCheckInputs = runtimeTools;

  postInstall = ''
    wrapProgram $out/bin/${pname} --prefix PATH : ${lib.makeBinPath runtimeTools}
  '';

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../src
      (lib.fileset.maybeMissing ../.cargo)
      (lib.fileset.maybeMissing ../build.rs)
      (lib.fileset.maybeMissing ../tests)
      (lib.fileset.maybeMissing ../examples)
      (lib.fileset.maybeMissing ../benches)
    ];
  };

  meta = {
    description = manifest.package.description;
    license = lib.licenses.mit;
    mainProgram = pname;
  };
}
