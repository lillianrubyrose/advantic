{
  description = "Gleam compiler development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, fenix, ... }:
    let
      systems = [ "x86_64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      devShells = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          fenixPkgs = fenix.packages.${system};
          toolchain = fenixPkgs.stable.withComponents [
            "cargo"
            "clippy"
            "rust-analyzer"
            "rust-src"
            "rustc"
            "rustfmt"
          ];
          buildInputs = with pkgs; [ SDL2 ];
        in {
          default = pkgs.mkShell {
            packages = [
              toolchain
              fenixPkgs.targets.wasm32-unknown-unknown.latest.rust-std
              pkgs.samply
            ];
            buildInputs = [] ++ buildInputs;
            shellHook = ''
              export LD_LIBRARY_PATH=$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath buildInputs}
            '';
          };
        });
    };
}
