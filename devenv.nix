{ pkgs, lib, ... }:
let
  # Coherent development SDK/runtime; provenance is in nix/dotnet-10.0.404.json.
  # Nix loader/locale preparation support is still unverified (see nix/README.md).
  supportedDotnet =
    (pkgs.dotnetCorePackages.buildDotnetSdk ./nix/dotnet-10.0.404.nix).sdk_10_0_4xx;
in
{
  packages = [ (import ./nix/toolchain.nix { inherit pkgs; }) supportedDotnet
    pkgs.rust-bin.stable."1.100.0".default
  ] ++ (with pkgs; [
    git gh curl jq unzip zip ripgrep shellcheck shfmt cargo-audit
    pkg-config openssl python3 stdenv.cc
  ]) ++ lib.optionals pkgs.stdenv.isLinux (with pkgs; [
    xvfb-run xorg.xauth openbox dbus glibcLocales patchelf imagemagick
  ]);
  env = {
    FN_DEVENV = "1";
    SAFE_CHAIN_MINIMUM_PACKAGE_AGE_HOURS = "48";
    DOTNET_CLI_TELEMETRY_OPTOUT = "1";
    DOTNET_SKIP_FIRST_TIME_EXPERIENCE = "1";
    FSHARP_NAVIGATOR_DOTNET = "${supportedDotnet}/bin/dotnet";
  };
  enterShell = ''
    export PATH="$DEVENV_ROOT/cli/bin:$PATH"
  '';
  cachix.enable = false;
  devenv.warnOnNewVersion = false;
}
