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

    naersk.url = "github:nix-community/naersk";
    naersk.inputs.nixpkgs.follows = "nixpkgs";

    typst-packages.url = "github:typst/packages";
    typst-packages.flake = false;
  };

  outputs = { self, nixpkgs, naersk, typst-packages }:
    let forAllSystems = nixpkgs.lib.genAttrs nixpkgs.lib.systems.flakeExposed;
    in rec {
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          naersk' = pkgs.callPackage naersk { };
          texliveCombined = (pkgs.texlive.combine {
            inherit (pkgs.texlive)
              babel-german
              bussproofs
              unicode-math
              fontspec
              latexmk
              preview
              lm
              lm-math
              scheme-basic
              standalone
              xcolor
              xetex
              braket
              ;
          });
        in
        rec {
          latexfogel = naersk'.buildPackage { src = ./.; };
          default = latexfogel;
          docker = pkgs.dockerTools.buildLayeredImage {
            name = "ghcr.io/kitmatheinfo/latexfogel";
            tag = latexfogel.version;

            contents = with pkgs; [
              latexfogel
              cacert # or reqwest is very unhappy
              fontconfig # or tectonic fails
              bash # or magick can not spawn `gs`
              imagemagick # to convert, imagemagick_light has no adapter
              ghostscript_headless # to convert
              docker-client # to communicate with docker
              texliveCombined
            ];

            config = {
              Entrypoint = [ "/bin/latexfogel" ];
              WorkingDir = "/";
              Env = [
                "FONTCONFIG_FILE=${pkgs.makeFontsConf { fontDirectories = [ texliveCombined.fonts pkgs.noto-fonts pkgs.noto-fonts-color-emoji ]; }}"
                "TYPST_PACKAGES=${typst-packages}/packages"
                "HOME=/tmp"
              ];
            };
          };
        }
      );
      devShells = forAllSystems (system:
        let pkgs = import nixpkgs { inherit system; };
        in rec {
          docker = pkgs.mkShell rec {
            publish = pkgs.writeScriptBin "publish" ''
              chore/publish.sh "${packages."${system}".docker}" "${packages."${system}".docker.imageName}" "${packages."${system}".docker.imageTag}" "$1"
            '';
            packages = [ publish ];
          };
        });
    };
}
