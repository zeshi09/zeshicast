{
  description = "zeshicast — Raycast-like launcher + notification daemon for Wayland/niri";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor = system: import nixpkgs { inherit system; };
      commonRuntimePackages = pkgs:
        with pkgs; [
          wl-clipboard
          xclip
          wireplumber
          networkmanager
          iproute2
          brightnessctl
          bluez
          wtype
          grim
          slurp
        ];

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

      zeshicastModuleOptions = { lib, pkgs }: {
        enable = lib.mkEnableOption
          "zeshicast launcher daemon and notification server";
        package = lib.mkOption {
          type = lib.types.package;
          default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
          defaultText = lib.literalExpression "zeshicast.packages.\${system}.default";
          description = "The zeshicast package to use.";
        };
        extraRuntimePackages = lib.mkOption {
          type = lib.types.listOf lib.types.package;
          default = [ ];
          example = lib.literalExpression ''
            with pkgs; [
              wireplumber
              networkmanager
              iproute2
              brightnessctl
              bluez
              wtype
              grim
              slurp
              xclip
            ]
          '';
          description = ''
            Extra command-line tools made available to zeshicast runtime
            integrations. The package wrapper already includes wl-clipboard;
            add tools here for audio, networking, brightness, Bluetooth,
            screenshot, typing, and fallback clipboard actions.
          '';
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
            ] ++ commonRuntimePackages pkgs;
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
          options.services.zeshicast = zeshicastModuleOptions { inherit lib pkgs; };

          config = lib.mkIf cfg.enable {
            environment.systemPackages = [ cfg.package ] ++ cfg.extraRuntimePackages;

            systemd.user.services.zeshicast = {
              description = "zeshicast launcher daemon + notification server";
              documentation = [ "https://github.com/zeshi09/zeshicast" ];
              partOf = [ "graphical-session.target" ];
              after = [ "graphical-session.target" ];
              wantedBy = [ "graphical-session.target" ];
              path = cfg.extraRuntimePackages;
              # Hardening зеркален packaging/zeshicast-gtk.service (P2-6).
              # Включать закомментированное ниже только после ручной проверки:
              # GTK4/malloc/rustls могут ломаться под W^X и строгим фильтром.
              serviceConfig = {
                ExecStart = "${lib.getExe cfg.package} --daemon";
                ExecStop = "${lib.getExe cfg.package} --quit";
                Restart = "on-failure";
                RestartSec = 2;
                NoNewPrivileges = true;
                PrivateTmp = true;
                ProtectSystem = "full";
                ProtectHome = "read-only";
                # %h корректен для user-юнитов; демону нужны запись конфига и кэша.
                ReadWritePaths = [
                  "%h/.config/zeshicast"
                  "%h/.cache/zeshicast"
                  "%h/.local/share/fonts/zeshicast"  # bundled font install (ui/fonts.rs)
                ];
                RestrictSUIDSGID = true;
                LockPersonality = true;
                RestrictRealtime = true;
                ProtectKernelTunables = true;
                ProtectKernelModules = true;
                ProtectControlGroups = true;
                ProtectClock = true;
                ProtectHostname = true;
                CapabilityBoundingSet = [ ];
                RestrictAddressFamilies = [
                  "AF_UNIX"
                  "AF_INET"
                  "AF_INET6"
                  "AF_NETLINK"
                ];
                #MemoryDenyWriteExecute = true;
                #SystemCallFilter = [ "@system-service" ];
                #SystemCallArchitectures = "native";
              };
            };
          };
        };

      # Home Manager module: installs the package and defines the same daemon as
      # a systemd user service using Home Manager's unit-file schema.
      homeManagerModules.default = { config, lib, pkgs, ... }:
        let cfg = config.services.zeshicast;
        in {
          options.services.zeshicast = zeshicastModuleOptions { inherit lib pkgs; };

          config = lib.mkIf cfg.enable {
            home.packages = [ cfg.package ] ++ cfg.extraRuntimePackages;

            systemd.user.services.zeshicast = {
              Unit = {
                Description = "zeshicast launcher daemon + notification server";
                Documentation = [ "https://github.com/zeshi09/zeshicast" ];
                PartOf = [ "graphical-session.target" ];
                After = [ "graphical-session.target" ];
              };
              # Hardening зеркален packaging/zeshicast-gtk.service (P2-6).
              # Включать закомментированное ниже только после ручной проверки:
              # GTK4/malloc/rustls могут ломаться под W^X и строгим фильтром.
              Service = {
                ExecStart = "${lib.getExe cfg.package} --daemon";
                ExecStop = "${lib.getExe cfg.package} --quit";
                Restart = "on-failure";
                RestartSec = 2;
                NoNewPrivileges = true;
                PrivateTmp = true;
                ProtectSystem = "full";
                ProtectHome = "read-only";
                # %h корректен для user-юнитов; демону нужны запись конфига и кэша.
                ReadWritePaths = [
                  "%h/.config/zeshicast"
                  "%h/.cache/zeshicast"
                  "%h/.local/share/fonts/zeshicast"  # bundled font install (ui/fonts.rs)
                ];
                RestrictSUIDSGID = true;
                LockPersonality = true;
                RestrictRealtime = true;
                ProtectKernelTunables = true;
                ProtectKernelModules = true;
                ProtectControlGroups = true;
                ProtectClock = true;
                ProtectHostname = true;
                CapabilityBoundingSet = [ ];
                RestrictAddressFamilies = [
                  "AF_UNIX"
                  "AF_INET"
                  "AF_INET6"
                  "AF_NETLINK"
                ];
                #MemoryDenyWriteExecute = true;
                #SystemCallFilter = [ "@system-service" ];
                #SystemCallArchitectures = "native";
              };
              Install.WantedBy = [ "graphical-session.target" ];
            };
          };
        };
    };
}
