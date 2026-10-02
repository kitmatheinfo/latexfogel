# IMPORTANT:
#
# When using the flake as input, include `?submodules=1` at the end of the flake
# URL. This also applies when building the flake, meaning you have to use the
# following command:
#
# $ nix build '.?submodules=1'

{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, crane }:
    let forAllSystems = nixpkgs.lib.genAttrs nixpkgs.lib.systems.flakeExposed;
    in rec {
      packages = exported-packages;
      # We need a unique name for this that does not clash with anything in the
      # devShells declaration
      exported-packages = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          craneLib = crane.mkLib pkgs;
          commonArgs = {
            src = craneLib.cleanCargoSource ./.;
            strictDeps = true;
            nativeBuildInputs = [ pkgs.cmake pkgs.pkg-config ];
            buildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
              pkgs.libiconv
            ];
          };
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
          # Unversioned attributes select the newest published version in the
          # pinned Nixpkgs. Keep older versions recorded as dependencies there.
          latestTypstPackages = builtins.attrValues (pkgs.lib.filterAttrs
            (name: package: pkgs.lib.isDerivation package && name == package.pname)
            pkgs.typstPackages);
          bundledTypstPackages = pkgs.lib.closePropagation latestTypstPackages;
          texliveCombined = pkgs.texliveBasic.withPackages (ps: with ps; [
            babel-german
            bussproofs
            unicode-math
            fontspec
            latexmk
            preview
            lm
            lm-math
            standalone
            xcolor
            xetex
            braket
          ]);
        in
        rec {
          typst-packages = pkgs.linkFarm "typst-packages" (map (package: {
            name = "preview/${package.pname}/${package.version}";
            path = "${package}/lib/typst-packages/${package.pname}/${package.version}";
          }) bundledTypstPackages);
          latexfogel = craneLib.buildPackage (commonArgs // {
            inherit cargoArtifacts;
          });
          default = latexfogel;
          docker = pkgs.dockerTools.buildLayeredImage {
            name = "ghcr.io/kitmatheinfo/latexfogel";
            tag = latexfogel.version;

            contents = with pkgs; [
              cacert # or reqwest is very unhappy
              fontconfig # or tectonic fails
              bash # or magick can not spawn `gs`
              imagemagick # to convert, imagemagick_light has no adapter
              ghostscript_headless # to convert
              docker-client # to communicate with docker
              texliveCombined
            ];

            config = {
              Entrypoint = [ "${latexfogel}/bin/latexfogel" ];
              WorkingDir = "/";
              Env = [
                "FONTCONFIG_FILE=${pkgs.makeFontsConf { fontDirectories = [ texliveCombined.fonts pkgs.noto-fonts pkgs.noto-fonts-color-emoji ]; }}"
                "TYPST_PACKAGES=${typst-packages}"
                "HOME=/tmp"
              ];
            };
          };
        }
      );
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          docker-image = exported-packages."${system}".docker;
        in {
          docker = pkgs.mkShell rec {
            publish = pkgs.writeScriptBin "publish" ''
              chore/publish.sh "${docker-image}" "${docker-image.imageName}" "${docker-image.imageTag}" "$1"
            '';
            packages = [ publish ];
          };
        });
    };
}
