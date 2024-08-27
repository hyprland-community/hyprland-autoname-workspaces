{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, naersk }:
    let systems = [ "x86_64-linux" "aarch64-linux" ];
    in flake-utils.lib.eachSystem systems (system:
      let
        pkgs = import nixpkgs { inherit system; };
        naersk-lib = pkgs.callPackage naersk { };
        libs = with pkgs; [ libxkbcommon libinput libpulseaudio systemd ];
      in
      {
        defaultPackage = naersk-lib.buildPackage {
          src = ./.;
          meta.mainProgram = "hyprland-autoname-workspaces";
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = libs;
        };
        devShell = with pkgs; mkShell {
          buildInputs = [ cargo rustc rustfmt pre-commit rustPackages.clippy pkg-config ] ++ libs;
          RUST_SRC_PATH = rustPlatform.rustLibSrc;
        };
      }
    );
}
