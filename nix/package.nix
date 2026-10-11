{ lib, rustPlatform }:

let
  manifest = lib.importTOML ../Cargo.toml;
  pname = manifest.package.name;
  version = manifest.package.version;
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

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../src
      ../themes/default.css
      ../themes/bookish.css
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
