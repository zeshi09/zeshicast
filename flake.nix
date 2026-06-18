{
  description = "zeshicast — Raycast-like launcher + notification daemon for Wayland/niri";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor = system: import nixpkgs { inherit system; };

      zeshicastFor = pkgs:
        pkgs.rustPlatform.buildRustPackage {
          pname = "zeshicast";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          buildFeatures = [ "gui" "layer-shell" ];
          nativeBuildInputs = with pkgs; [ pkg-config wrapGAppsHook4 ];
          buildInputs = with pkgs; [
            gtk4
            gtk4-layer-shell
            glib
            graphene
            gdk-pixbuf
            pango
            cairo
            wayland
          ];
          # Runtime tools the launcher shells out to. wl-clipboard is the one we
          # rely on (clipboard copy / image paste-back); the rest (niri, wpctl,
          # nmcli, …) come from the user's graphical session.
          preFixup = ''
            gappsWrapperArgs+=(--prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.wl-clipboard ]})
          '';
          meta = {
            description =
              "Raycast-like launcher + freedesktop notification daemon for Wayland/niri";
            mainProgram = "zeshicast-gtk";
          };
        };
    in
    {
      devShells = forAllSystems (system:
        let pkgs = pkgsFor system;
        in {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              pkg-config
              gtk4
              gtk4-layer-shell
              glib
              graphene
              gdk-pixbuf
              pango
              cairo
              wayland
              wayland-protocols
              wl-clipboard
              xclip
            ];
          };
        });

      packages = forAllSystems (system:
        let pkgs = pkgsFor system;
        in {
          default = zeshicastFor pkgs;
          zeshicast = zeshicastFor pkgs;
        });

      # `nix run github:blackzeshi/zeshicast` launches the GTK launcher.
      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/zeshicast-gtk";
        };
      });

      # NixOS module: installs the package and runs the resident daemon (warm
      # launcher index + freedesktop notification server + clipboard capture) as
      # a systemd *user* service tied to the graphical session.
      nixosModules.default = { config, lib, pkgs, ... }:
        let cfg = config.services.zeshicast;
        in {
          options.services.zeshicast = {
            enable = lib.mkEnableOption
              "zeshicast launcher daemon and notification server";
            package = lib.mkOption {
              type = lib.types.package;
              default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
              defaultText = lib.literalExpression "zeshicast.packages.\${system}.default";
              description = "The zeshicast package to use.";
            };
          };

          config = lib.mkIf cfg.enable {
            environment.systemPackages = [ cfg.package ];

            systemd.user.services.zeshicast = {
              description = "zeshicast launcher daemon + notification server";
              documentation = [ "https://github.com/zeshi09/zeshicast" ];
              partOf = [ "graphical-session.target" ];
              after = [ "graphical-session.target" ];
              wantedBy = [ "graphical-session.target" ];
              serviceConfig = {
                ExecStart = "${lib.getExe cfg.package} --daemon";
                ExecStop = "${lib.getExe cfg.package} --quit";
                Restart = "on-failure";
                RestartSec = 2;
              };
            };
          };
        };
    };
}
