# Package one zeshicast extension declaratively (Q5.6).
#
#   git-tools = zeshicast.lib.${system}.mkZeshicastExtension {
#     id = "you.git-tools";
#     src = pkgs.fetchFromGitHub { owner = "you"; repo = "git-tools"; rev = "…"; hash = "…"; };
#     capabilities = [ "shell" ];
#   };
#   # Home Manager:
#   xdg.configFile."zeshicast/extensions/you.git-tools".source = git-tools;
#
# The build fails when the extension does not pass `zeshicast --extension-lint`
# or its manifest asks for a capability not listed in `capabilities`, so the
# list here is the review point, like a registry entry's.
{ lib, stdenvNoCC, zeshicast }:

{ id
, src
, capabilities
, version ? "unstable"
}:

assert lib.assertMsg (builtins.match "[A-Za-z0-9][A-Za-z0-9._-]*" id != null)
  "mkZeshicastExtension: `${id}` is not a valid extension id";

stdenvNoCC.mkDerivation {
  pname = "zeshicast-extension-${id}";
  inherit version src;

  dontConfigure = true;
  dontBuild = true;
  dontFixup = true;

  installPhase = ''
    runHook preInstall
    cp -r . $out
    runHook postInstall
  '';

  doInstallCheck = true;
  nativeInstallCheckInputs = [ zeshicast ];
  installCheckPhase = ''
    runHook preInstallCheck
    grep -Eq '^[[:space:]]*id[[:space:]]*=[[:space:]]*"${lib.escapeRegex id}"' $out/extension.toml \
      || { echo "extension.toml does not declare id = \"${id}\"" >&2; exit 1; }
    zeshicast --extension-lint $out --allow ${lib.escapeShellArg (lib.concatStringsSep "," capabilities)}
    runHook postInstallCheck
  '';

  meta.description = "zeshicast extension ${id}";
}
