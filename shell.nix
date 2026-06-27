{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
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
    wireplumber
    networkmanager
    iproute2
    brightnessctl
    bluez
    wtype
    grim
    slurp
  ];
}
