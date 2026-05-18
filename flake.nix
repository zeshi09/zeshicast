{
  description = "zeshicast development shell";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { nixpkgs, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
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
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
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
          };
        });
    };
}
