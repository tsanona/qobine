{
  description = "Qobine config";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let pkgs = import nixpkgs { inherit system; };
      in with pkgs;
      {
        devShell = mkShell {
          nativeBuildInputs = [ 
            pkg-config 
            sqlx-cli 
            just
            # --feature mqtt
            cmake
          ];
          buildInputs = [
            # qobuz-player + qobuz-player-connect + qobuz-player-rfid + qobuz-player-web (base libs)
            alsa-lib
            openssl
            # qobuz-player-gtk (+ base libs)
            libadwaita
            webkitgtk_6_0
          ];
          ALSA_PLUGIN_DIR="${pipewire}/lib/alsa-lib";
        };
      }
    );
}
