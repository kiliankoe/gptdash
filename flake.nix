{
  description = "gptdash dev shell, package, and NixOS module";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;

        version = lib.substring 0 7 (self.sourceInfo.rev or "dev");

        frontend = pkgs.buildNpmPackage {
          pname = "gptdash-frontend";
          inherit version;
          src = ./frontend;
          npmDepsHash = "sha256-oEYy16VZ/pw4n850Kf0bAl4ZkzExy5y5Lc4GWQ0k2Kw=";
          NODE_OPTIONS = "--max-old-space-size=4096";

          # Create the version file like the Makefile does
          preBuild = ''
            echo "export const VERSION = \"${version}\";" > src/version.ts
          '';

          buildPhase = ''
            runHook preBuild
            npm run build
            runHook postBuild
          '';
          installPhase = ''
            runHook preInstall
            mkdir -p $out/dist
            cp -r dist/* $out/dist/
            runHook postInstall
          '';
        };

        gptdash = pkgs.buildGoModule {
          pname = "gptdash";
          inherit version;
          src = ./.;
          modRoot = "./backend";
          subPackages = [ "cmd/server" ];

          # Copy/embed built frontend into backend/static/dist before compilation
          preBuild = ''
            mkdir -p static/dist
            cp -r ${frontend}/dist/* static/dist/
          '';

          ldflags = [ "-X main.version=${version}" ];

          vendorHash = "sha256-hGx/bQNL6BXEGVYZTqivt0bpTai2/PdxEGdc6TKMZMA=";

          go = pkgs.go_1_24 or pkgs.go;

          env.CGO_ENABLED = "0";
          doCheck = false;

          postInstall = ''
            # Normalize binary name
            if [ -e "$out/bin/server" ]; then
              mv "$out/bin/server" "$out/bin/gptdash"
            fi
          '';
        };
      in
      {
        packages = {
          inherit gptdash;
          default = gptdash;
        };

        apps.default = {
          type = "app";
          program = lib.getExe gptdash;
        };

        devShells.default = pkgs.mkShell {
          packages = [
            (pkgs.nodejs_24 or pkgs.nodejs_latest or pkgs.nodejs)
            (pkgs.go_1_24 or pkgs.go)
            pkgs.git
            pkgs.makeWrapper
            pkgs.gnumake
          ];

          shellHook = ''
            echo "gptdash dev shell: go=$(go version | cut -d' ' -f3) node=$(node --version)"
            echo "- Run: make build  (produces ./gptdash)"
            echo "- Or: (cd frontend && npm run dev) + (cd backend && go run ./cmd/server)"
          '';
        };

        nixosModules.default =
          {
            config,
            lib,
            pkgs,
            ...
          }:
          let
            cfg = config.services.gptdash;
          in
          {
            options.services.gptdash = {
              enable = lib.mkEnableOption "GPTdash game server";

              package = lib.mkOption {
                type = lib.types.package;
                default = gptdash;
                description = "Package providing the gptdash binary.";
              };

              user = lib.mkOption {
                type = lib.types.str;
                default = "gptdash";
                description = "User to run the service as.";
              };

              group = lib.mkOption {
                type = lib.types.str;
                default = "gptdash";
                description = "Group to run the service as.";
              };

              stateDir = lib.mkOption {
                type = lib.types.str;
                default = "/var/lib/gptdash";
                description = "Working directory for state and exports.";
              };

              # Core settings (mapped to env/flags)
              port = lib.mkOption {
                type = lib.types.port;
                default = 8080;
                description = "TCP port to listen on.";
              };

              defaultProvider = lib.mkOption {
                type = lib.types.enum [
                  "openai"
                  "ollama"
                ];
                default = "openai";
                description = "Default AI provider.";
              };

              defaultModel = lib.mkOption {
                type = lib.types.str;
                default = "gpt-3.5-turbo";
                description = "Default AI model to use.";
              };

              systemPrompt = lib.mkOption {
                type = lib.types.str;
                default = "Du bist eine prägnante, sich kurzfassende KI. Antworte knapp in 1-2 Sätzen.";
                description = "System prompt for the AI provider.";
              };

              openAI = {
                apiKey = lib.mkOption {
                  type = lib.types.nullOr lib.types.str;
                  default = null;
                  description = "OpenAI API key (required for OpenAI provider).";
                };
                baseURL = lib.mkOption {
                  type = lib.types.nullOr lib.types.str;
                  default = null;
                  description = "Custom OpenAI API base URL.";
                };
              };

              ollamaHost = lib.mkOption {
                type = lib.types.str;
                default = "http://localhost:11434";
                description = "Ollama host URL.";
              };

              gmCredentials = {
                user = lib.mkOption {
                  type = lib.types.nullOr lib.types.str;
                  default = null;
                  description = "GM username for basic auth.";
                };
                pass = lib.mkOption {
                  type = lib.types.nullOr lib.types.str;
                  default = null;
                  description = "GM password for basic auth.";
                };
              };

              singleSession = lib.mkOption {
                type = lib.types.bool;
                default = true;
                description = "Allow only one active session at a time.";
              };

              export = {
                enabled = lib.mkOption {
                  type = lib.types.bool;
                  default = true;
                  description = "Enable exporting game results to a file.";
                };
                file = lib.mkOption {
                  type = lib.types.str;
                  default = "/var/lib/gptdash/gptdash-results.txt";
                  description = "Path to export game results file (should reside under stateDir).";
                };
              };

              environment = lib.mkOption {
                type = lib.types.attrsOf lib.types.str;
                default = { };
                description = "Extra environment variables to pass to the service.";
              };
            };

            config = lib.mkIf cfg.enable {
              users.users.${cfg.user} = {
                isSystemUser = true;
                group = cfg.group;
                home = cfg.stateDir;
              };
              users.groups.${cfg.group} = { };

              systemd.tmpfiles.rules = [
                "d ${cfg.stateDir} 0750 ${cfg.user} ${cfg.group} - -"
                "f ${cfg.export.file} 0640 ${cfg.user} ${cfg.group} - -"
              ];

              systemd.services.gptdash =
                let
                  baseEnv = {
                    DEFAULT_PROVIDER = cfg.defaultProvider;
                    DEFAULT_MODEL = cfg.defaultModel;
                    SYSTEM_PROMPT = cfg.systemPrompt;
                    OLLAMA_HOST = cfg.ollamaHost;
                    SINGLE_SESSION = lib.boolToString cfg.singleSession;
                    EXPORT_ENABLED = lib.boolToString cfg.export.enabled;
                    EXPORT_FILE = toString cfg.export.file;
                  };
                  optionalEnv = lib.mkMerge [
                    cfg.environment
                    (lib.optionalAttrs (cfg.openAI.apiKey != null) { OPENAI_API_KEY = cfg.openAI.apiKey; })
                    (lib.optionalAttrs (cfg.openAI.baseURL != null) { OPENAI_BASE_URL = cfg.openAI.baseURL; })
                    (lib.optionalAttrs (cfg.gmCredentials.user != null) { GM_USER = cfg.gmCredentials.user; })
                    (lib.optionalAttrs (cfg.gmCredentials.pass != null) { GM_PASS = cfg.gmCredentials.pass; })
                  ];
                in
                {
                  description = "GPTdash game server";
                  wantedBy = [ "multi-user.target" ];
                  after = [ "network-online.target" ];
                  wants = [ "network-online.target" ];

                  serviceConfig = {
                    User = cfg.user;
                    Group = cfg.group;
                    WorkingDirectory = cfg.stateDir;
                    ExecStart = ''${lib.getExe cfg.package} --port ${toString cfg.port}'';

                    # Hardening
                    Restart = "on-failure";
                    RestartSec = 2;
                    NoNewPrivileges = true;
                    PrivateTmp = true;
                    ProtectHome = true;
                    ProtectKernelTunables = true;
                    ProtectKernelModules = true;
                    ProtectKernelLogs = true;
                    ProtectControlGroups = true;
                    SystemCallFilter = "@system-service";
                  };

                  environment = baseEnv // optionalEnv;
                };

              # Optionally open the firewall port (off by default)
              # networking.firewall.allowedTCPPorts = [ cfg.port ];
            };
          };
      }
    );
}
