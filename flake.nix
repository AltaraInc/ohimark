{
  description = "Generate a static HTML site from a directory of Markdown";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, rust-overlay, ... }:
  let
    supportedSystems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ];
    forEachSupportedSystem = f: nixpkgs.lib.genAttrs supportedSystems (system: f {
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default overlay ];
      };
    });
    manifest = nixpkgs.lib.importTOML ./Cargo.toml;
    pname = manifest.package.name;
    version = manifest.package.version;
    # Named after the package so the overlay does not claim a generic `docs`
    # attribute in the package set of anyone who applies it.
    docsName = "${pname}-docs";
    overlay = final: prev: {
      rustToolchain = final.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      ${pname} = final.callPackage ./nix/package.nix {
        rustPlatform = final.makeRustPlatform {
          cargo = final.rustToolchain;
          rustc = final.rustToolchain;
        };
      };
      ${docsName} = final.callPackage ./nix/docs.nix {
        inherit pname;
        package = final.${pname};
        theme = "bookish";
      };
    };
  in {
    overlays.default = overlay;

    packages = forEachSupportedSystem ({ pkgs }: {
      default = pkgs.${pname};
      app = pkgs.${pname};
      ${pname} = pkgs.${pname};
      docs = pkgs.${docsName}.docs;
    } // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
      container = pkgs.dockerTools.buildLayeredImage {
        name = pname;
        tag = version;
        contents = [ pkgs.${pname} ];
        config = {
          Entrypoint = [ "${pkgs.${pname}}/bin/${pname}" ];
        };
      };
    });

    apps = forEachSupportedSystem ({ pkgs }: rec {
      default = {
        type = "app";
        program = "${pkgs.${pname}}/bin/${pname}";
      };
      ${pname} = default;
      # Render this repository's Markdown (root file `README.md`) and serve
      # it; both apps take the port as an argument, defaulting to 8080.
      docs = {
        type = "app";
        program = "${pkgs.${docsName}.serve}/bin/${pname}-docs";
      };
      # The same server, re-rendering whenever a watched file changes.
      watchdocs = {
        type = "app";
        program = "${pkgs.${docsName}.watch}/bin/${pname}-watchdocs";
      };
    });

    checks = forEachSupportedSystem ({ pkgs }: {
      default = pkgs.${pname};
      format = pkgs.runCommand "${pname}-format" {
        nativeBuildInputs = [ pkgs.rustToolchain ];
      } ''
        cp -r ${pkgs.${pname}.src} source
        chmod -R u+w source
        cd source
        cargo fmt --all -- --check
        touch $out
      '';
      clippy = pkgs.${pname}.overrideAttrs (old: {
        pname = "${pname}-clippy";
        doCheck = false;
        nativeBuildInputs = old.nativeBuildInputs ++ [ pkgs.rustToolchain ];
        buildPhase = ''
          runHook preBuild
          cargo clippy --offline --locked --all-targets -- -D warnings
          runHook postBuild
        '';
        installPhase = "touch $out";
      });
      docs = pkgs.${docsName}.docs;
    });

    devShells = forEachSupportedSystem ({ pkgs }: {
      default = pkgs.mkShell {
        inputsFrom = [ pkgs.${pname} ];
        packages = [
          pkgs.rustToolchain
        ];
        RUST_SRC_PATH = "${pkgs.rustToolchain}/lib/rustlib/src/rust/library";
      };
    });
  };
}
