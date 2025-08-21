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
        inherit (pkgs.lib) attrValues genAttrs optionalString;
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
        buildPackage = pname: let
          inherit (crane'.crateNameFromCargoToml {
            src = "${src}/${pname}";
          }) version;
          cargoExtraArgs = "--locked -p ${pname}";
        in crane'.buildPackage (common // {
          inherit pname version src cargoExtraArgs;
          cargoArtifacts = buildDepsOnly (common // {
            inherit pname version src cargoExtraArgs cargoArtifacts;
            doCheck = false; # the workspace deps build checks deps already
          });
          doCheck = false; # tests are run as a flake check
        });
        members = genAttrs [
          "maw"
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
