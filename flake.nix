{
  inputs = {
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, crane, flake-utils, nixpkgs }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = (import nixpkgs) { inherit system; };
        inherit (pkgs.lib) map listToAttrs attrValues;
        crane' = crane.mkLib pkgs;
        src = crane'.cleanCargoSource ./.;
        common = {
          pname = "workspace";
          version = "0";
          inherit src;
        };
        cargoArtifacts = crane'.buildDepsOnly common;
        buildPackage = pname: crane'.buildPackage (common // {
          inherit pname src cargoArtifacts;
          inherit (crane'.crateNameFromCargoToml {
            src = "${src}/${pname}";
          }) version;
          cargoExtraArgs = "--locked -p ${pname}";
          doCheck = false; # tests are run as a flake check
        });
        # this feels like something that should already exist in lib
        # and i just dont know the name of...
        mapToAttrs = f: l: listToAttrs (map (n: { name = n; value = f n; }) l);
        members = mapToAttrs buildPackage [
          "maw"
        ];
      in {
        checks = {
          clippy = crane'.cargoClippy (common // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- -D warnings";
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
