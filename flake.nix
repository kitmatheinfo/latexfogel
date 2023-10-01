{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.05";

    naersk.url = "github:nix-community/naersk";
    naersk.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, naersk }:
    let forAllSystems = nixpkgs.lib.genAttrs nixpkgs.lib.systems.flakeExposed;
    in
    {
      packages = forAllSystems (system:
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
            name = "kitmatheinfo/latexfogel";
            tag = latexfogel.version;

            contents = [
              latexfogel
              pkgs.cacert                 # or reqwest is very unhappy
              pkgs.fontconfig             # or tectonic fails
              pkgs.bash                   # or magick can not spawn `gs`
              pkgs.imagemagick            # to convert, imagemagick_light has no adapter
              pkgs.ghostscript_headless   # to convert
            ];

            config = {
              Cmd = [ "/bin/latexfogel" ];
              WorkingDir = "/";
              Env = [
                "FONTCONFIG_FILE=${pkgs.fontconfig.out}/etc/fonts/fonts.conf"
                "FONTCONFIG_PATH=${pkgs.fontconfig.out}/etc/fonts/"
              ];
            };
          };
        }
      );
    };
}
