{
  description = "GPTDash development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
          ];
        };

        # Combined dev scripts
        check = pkgs.writeShellScriptBin "check" ''
          set -e
          echo "Running cargo check..."
          cargo check
          echo ""
          echo "Running cargo clippy..."
          cargo clippy -- -D warnings
          echo ""
          echo "Running biome check..."
          biome check .
          echo ""
          echo "Running biome lint..."
          biome lint .
          echo ""
          echo "✓ All checks passed!"
        '';

        format = pkgs.writeShellScriptBin "format" ''
          set -e
          echo "Running cargo fmt..."
          cargo fmt
          echo ""
          echo "Running biome format..."
          biome format --write .
          echo ""
          echo "✓ All formatting complete!"
        '';
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.nodejs
            pkgs.biome
            pkgs.k6

            # Build dependencies
            pkgs.pkg-config
            pkgs.openssl

            # Dev scripts
            check
            format
          ]
          ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.libiconv
          ];

          shellHook = ''
            echo "GPTDash dev environment loaded"
            echo "  check  - run all checks (cargo check/clippy, biome check/lint)"
            echo "  format - format all code (cargo fmt, biome format)"
          '';
        };
      }
    );
}
