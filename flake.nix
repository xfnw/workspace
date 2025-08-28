{
  inputs = {
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, crane, flake-utils, nixpkgs }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        inherit (pkgs.lib) attrValues concatLines fileset genAttrs map optionalString;
        crane' = crane.mkLib pkgs;
        # simplified crane'.buildDepsOnly that allows artifacts
        buildDepsOnly =
          {
            pname,
            version,
            cargoExtraArgs ? "--locked",
            cargoBuildCommand ? "cargoWithProfile build",
            cargoCheckCommand ? "cargoWithProfile check",
            cargoCheckExtraArgs ? "--all-targets",
            cargoTestCommand ? "cargoWithProfile test",
            cargoTestExtraArgs ? "--no-run",
            ...
          }@args: let
            src = crane'.mkDummySrc args;
            dargs = args // { inherit src; };
            doCheck = args.doCheck or true;
          in crane'.mkCargoDerivation (args // {
            inherit src doCheck;
            pnameSuffix = "-deps";
            cargoArtifacts = args.cargoArtifacts or null;
            cargoVendorDir = args.cargoVendorDir or (crane'.vendorCargoDeps dargs);
            env.CRANE_BUILD_DEPS_ONLY = 1;
            buildPhaseCargoCommand = ''
              ${optionalString doCheck "${cargoCheckCommand} ${cargoExtraArgs} ${cargoCheckExtraArgs}"}
              ${cargoBuildCommand} ${cargoExtraArgs}
            '';
            checkPhaseCargoCommand = ''
              ${cargoTestCommand} ${cargoExtraArgs} ${cargoTestExtraArgs}
            '';
            doInstallCargoArtifacts = true;
          });
        src = crane'.cleanCargoSource ./.;
        common = {
          pname = "workspace";
          version = "0";
          inherit src;
        };
        cargoArtifacts = buildDepsOnly common;
        commonCrates = [
          "foxerror"
        ];
        commonFileSet = fileset.unions ([
          ./Cargo.toml
          ./Cargo.lock
        ] ++ map (p: crane'.fileset.commonCargoSources ./crates/${p}) commonCrates);
        commonSrc = fileset.toSource {
          root = ./.;
          fileset = commonFileSet;
        };
        buildPackage = pname: let
          inherit (crane'.crateNameFromCargoToml {
            src = "${src}/crates/${pname}";
          }) version;
          cargoExtraArgs = "--offline -p ${pname}";
          src = fileset.toSource {
            root = ./.;
            fileset = fileset.union
              commonFileSet
              (crane'.fileset.commonCargoSources ./crates/${pname});
          };
        in crane'.buildPackage (common // {
          inherit pname version src cargoExtraArgs;
          cargoArtifacts = buildDepsOnly (common // {
            inherit pname version src cargoExtraArgs cargoArtifacts;
            extraDummyScript = ''
              # mkDummySrc tries to eat workspace lints. put them back
              ln -sf ${commonSrc}/Cargo.toml $out/Cargo.toml

              ${concatLines (map (crate: ''
                rm -r $out/crates/${crate}
                ln -s ${commonSrc}/crates/${crate} $out/crates
              '') commonCrates)}
            '';
            doCheck = false; # the workspace deps build checks deps already
          });
          doCheck = false; # tests are run as a flake check
        });
        members = genAttrs [
          "maw"
          "vasm"
        ] buildPackage;
      in {
        checks = {
          clippy = crane'.cargoClippy (common // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- -D warnings";
          });
          doc = crane'.cargoDoc (common // {
            inherit cargoArtifacts;
            env.RUSTDOCFLAGS = "-D warnings";
          });
          test = crane'.cargoTest (common // {
            inherit cargoArtifacts;
          });
        };

        packages = members // {
          default = pkgs.symlinkJoin {
            name = "all";
            paths = attrValues members;
          };
        };

        devShells.default = crane'.devShell {
          checks = self.checks.${system};
        };
      });
}
