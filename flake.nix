{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    naersk.url = "github:nix-community/naersk";
    naersk.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, naersk }:
    let forAllSystems = nixpkgs.lib.genAttrs nixpkgs.lib.systems.flakeExposed;
    in
    rec {
      packages = flake-packages;
      flake-packages = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          naersk' = pkgs.callPackage naersk { };
        in
        rec {
          latexfogel = naersk'.buildPackage
            {
              root = ./.;
              nativeBuildInputs = with pkgs; [ pkg-config openssl graphite2 icu freetype fontconfig ];
            };
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
              (texlive.combine {
                inherit (texlive)
                  babel-german
                  bussproofs
                  latexmk
                  preview
                  scheme-basic
                  standalone
                  xcolor
                  xetex
                  braket
                  ;
              })
            ];

            config = {
              Entrypoint = [ "/bin/latexfogel" ];
              WorkingDir = "/";
              Env = [
                "FONTCONFIG_FILE=${pkgs.fontconfig.out}/etc/fonts/fonts.conf"
                "FONTCONFIG_PATH=${pkgs.fontconfig.out}/etc/fonts/"
              ];
            };
          };
        }
      );
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        rec {
          docker = pkgs.mkShell rec {
            publish = pkgs.writeScriptBin "publish" ''
              chore/publish.sh "${flake-packages."${system}".docker}" "${flake-packages."${system}".docker.imageName}" "${flake-packages."${system}".docker.imageTag}" "$1"
            '';
            packages = [ publish ];
          };
        });
    };
}
