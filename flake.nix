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

        mkCommonArgs = { binName, features ? "" }:
          with pkgs; {
            inherit src;
            strictDeps = true;

            pname = "qobine";

            nativeBuildInputs = [ 
              pkg-config 
              sqlx-cli 
              just
              # # --feature mqtt
              # cmake
            ] ++ lib.optional (builtins.elem features ["mqtt" "all"]) cmake;

            buildInputs = [
              # qobuz-player + qobuz-player-connect + qobuz-player-rfid + qobuz-player-web (base libs)
              alsa-lib
              openssl
              # # qobuz-player-gtk (+ base libs)
              # libadwaita
              # webkitgtk_6_0
            ] ++ lib.optionals (builtins.elem binName ["qobuz-player-gtk" "all"] ) [ libadwaita webkitgtk_6_0 ];
          };
        
        mkDbUrlEnvVar = { path }: "DATABASE_URL=sqlite:///${path}/qobuz-player.db";

        # binName = "all" because buildDepsOnly tries to build full workspace (even with cargo extra args set) and thus needs all libs.
        cargoArtifacts = craneLib.buildDepsOnly (mkCommonArgs { binName = "all"; features ="all"; });

        mkQobineBin = { binName, features ? "" }:
          craneLib.buildPackage (
            (mkCommonArgs { inherit binName features; })
            // {
              inherit cargoArtifacts;

              cargoExtraArgs = "--bin ${binName}" + lib.optionalString (features != "") " --features '${features}'";

              pname = binName;

              preBuild = ''
                export ${mkDbUrlEnvVar { path = "$(pwd)"; }}
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
          inputsFrom = [ (mkCommonArgs { binName = "all"; features = "all"; }) ];

          shellHook = ''
            export ${mkDbUrlEnvVar { path = "tmp"; }}
            just init-database
          '';

        };

        packages.container = 
          let
            binName = "qobuz-player-connect";
            redirectHost = "0.0.0.0";
            redirectPort = "42859";
          in pkgs.dockerTools.buildImage {

          name = binName;

          copyToRoot = pkgs.buildEnv {
            name = "image-root";
            paths = [
              (mkQobineBin { inherit binName; features = "mqtt"; })
              pkgs.cacert
              pkgs.tini
            ];
          };

          config = {
            Cmd = [ "tini" "--" binName ];
            Env = [
              "RUST_LOG=info" 
              (mkDbUrlEnvVar { path = "data"; }) #DATABASE_URL
              "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
              "QOBINE_REDIRECT_PORT=${redirectPort}"
              "QOBINE_REDIRECT_HOST=${redirectHost}"
            ];
            Volumes = { "/data" = {}; };
            ExposedPorts = {
                "${redirectPort}/tcp" = {};
              };
          };
        };
      }
    );
}
