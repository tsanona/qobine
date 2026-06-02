{
  description = "Qobine config";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, flake-utils, crane }:
    flake-utils.lib.eachDefaultSystem (system:
      let 
        pkgs = nixpkgs.legacyPackages.${system};
        inherit (pkgs) lib;
        craneLib = crane.mkLib pkgs;

        # from crane sqlx example
        unfilteredRoot = ./.;
        src = lib.fileset.toSource {
          root = unfilteredRoot;
          fileset = lib.fileset.unions [
            (craneLib.fileset.commonCargoSources unfilteredRoot)
            ./qobuz-player-controls/migrations
            ./justfile
          ];
        };

        commonArgs = with pkgs; {
          inherit src;
          strictDeps = true;

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
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        qobine-connect = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;

            pname = "qobine-connect";
            cargoExtraArgs = "--bin qobuz-player-connect";

            preBuild = ''
              export DATABASE_URL=sqlite://$(pwd)/qobuz-player.db
              just init-database
            '';

            # include initialized database in package to ship into container.
            postInstall = ''
              mkdir -p $out/data
              cp qobuz-player.db $out/data/
            '';

            doCheck = false;
          }
        );
      in
      {
        devShell = craneLib.devShell {
          inputsFrom = [ qobine-connect ];

          shellHook = ''
            just create-env-file
            just init-database
          '';

        };

        packages.qobine-connect = qobine-connect;

        packages.container = pkgs.dockerTools.buildImage {
          name = "qobine";

          copyToRoot = pkgs.buildEnv {
            name = "image-root";
            paths = [ qobine-connect pkgs.cacert ];
          };

          config = {
            Cmd = [ "qobuz-player-connect" ];
            Env = [
              "RUST_LOG=info" 
              "DATABASE_URL=sqlite:///data/qobuz-player.db" 
              "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            ];
            Volumes = { "/data" = {}; };
          };
        };
      }
    );
}
