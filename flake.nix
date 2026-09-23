{
  description = "Build and serve a repository's Markdown documentation as an offline mdBook site";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, rust-overlay, ... }:
  let
    supportedSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
    forEachSupportedSystem = f: nixpkgs.lib.genAttrs supportedSystems (system: f {
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default overlay ];
      };
    });
    manifest = nixpkgs.lib.importTOML ./Cargo.toml;
    pname = manifest.package.name;
    version = manifest.package.version;
    overlay = final: prev: {
      rustToolchain = final.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      ${pname} = final.callPackage ./nix/package.nix {
        rustPlatform = final.makeRustPlatform {
          cargo = final.rustToolchain;
          rustc = final.rustToolchain;
        };
      };
    };
  in {
    overlays.default = overlay;

    packages = forEachSupportedSystem ({ pkgs }:
    let
      docsOutputs = pkgs.callPackage ./nix/docs.nix {
        inherit pname;
        package = pkgs.${pname};
      };
    in {
      default = pkgs.${pname};
      app = pkgs.${pname};
      ${pname} = pkgs.${pname};
      docs = docsOutputs.docs;
      api-docs = docsOutputs.apiDocs;
    } // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
      container = pkgs.dockerTools.buildLayeredImage {
        name = pname;
        tag = version;
        contents = [ pkgs.${pname} pkgs.cacert ];
        config = {
          Entrypoint = [ "${pkgs.${pname}}/bin/${pname}" ];
          Env = [ "SSL_CERT_FILE=/etc/ssl/certs/ca-bundle.crt" ];
        };
      };
    });

      apps = forEachSupportedSystem ({ pkgs }:
        let
          docsOutputs = pkgs.callPackage ./nix/docs.nix {
            inherit pname;
            package = pkgs.${pname};
          };
          docsServe = pkgs.writeShellApplication {
            name = "${pname}-docs-serve";
            text = ''
              exec ${pkgs.lib.getExe pkgs.${pname}} --project-name ${pname} --api-docs ${docsOutputs.apiDocs} serve "$@"
            '';
          };
          docsWatch = pkgs.writeShellApplication {
            name = "${pname}-docs-watch";
            text = ''
              exec ${pkgs.lib.getExe pkgs.${pname}} --project-name ${pname} --api-docs ${docsOutputs.apiDocs} watch "$@"
            '';
          };
        in {
          default = {
            type = "app";
            program = "${pkgs.${pname}}/bin/${pname}";
          };
          ${pname} = {
            type = "app";
            program = "${pkgs.${pname}}/bin/${pname}";
          };
          docs = {
            type = "app";
            program = "${docsServe}/bin/${pname}-docs-serve";
          };
          docs-watch = {
            type = "app";
            program = "${docsWatch}/bin/${pname}-docs-watch";
          };
        });

      checks = forEachSupportedSystem ({ pkgs }:
        let
          docsOutputs = pkgs.callPackage ./nix/docs.nix {
            inherit pname;
            package = pkgs.${pname};
          };
        in
        {
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
          docs = docsOutputs.docs;
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
