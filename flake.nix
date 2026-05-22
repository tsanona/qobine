{
  description = "Qobine config";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        nativeBuildInputs = [ pkgs.pkg-config ];
        buildInputs = with pkgs; [
          glib
          gtk4
          alsa-lib
          openssl
          libsoup_3
          webkitgtk_6_0
          libadwaita
        ];
      };
    };
}
