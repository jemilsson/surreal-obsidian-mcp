{
  description = "Surreal Obsidian MCP - MCP server for Obsidian vaults with SurrealDB";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    let
      # NixOS module for the service
      nixosModule = { config, lib, pkgs, ... }:
        with lib;
        let
          cfg = config.services.surreal-obsidian-mcp;

          # Config template (API keys will be injected at runtime from apiKeyFile)
          configTemplate = builtins.toJSON {
            vault = {
              path = cfg.vaultPath;
            };
            database = {
              path = cfg.databasePath;
            };
            embedding = {
              provider = cfg.embedding.provider;
              model = cfg.embedding.model;
              dimensions = cfg.embedding.dimensions;
            } // optionalAttrs (cfg.embedding.apiBase != null) {
              api_base = cfg.embedding.apiBase;
            };
            reranking = {
              enabled = cfg.reranking.enable;
            } // optionalAttrs cfg.reranking.enable {
              provider = cfg.reranking.provider;
              model = cfg.reranking.model;
              top_n = cfg.reranking.topN;
            } // optionalAttrs (cfg.reranking.modelCache != null) {
              model_cache = cfg.reranking.modelCache;
            };
            sync = {
              watch_for_changes = cfg.sync.watchForChanges;
              initial_indexing = cfg.sync.initialIndexing;
              batch_size = cfg.sync.batchSize;
            };
            graph = {
              extract_links = cfg.graph.extractLinks;
              extract_backlinks = cfg.graph.extractBacklinks;
              extract_tags = cfg.graph.extractTags;
              extract_mentions = cfg.graph.extractMentions;
            };
          };

          runtimeConfigPath = "/run/surreal-obsidian-mcp/config.json";
        in
        {
          options.services.surreal-obsidian-mcp = {
            enable = mkEnableOption "Surreal Obsidian MCP server";

            package = mkOption {
              type = types.package;
              default = self.packages.${pkgs.system}.default;
              description = "The surreal-obsidian-mcp package to use";
            };

            vaultPath = mkOption {
              type = types.path;
              description = "Path to the Obsidian vault";
              example = "/home/user/Documents/ObsidianVault";
            };

            databasePath = mkOption {
              type = types.str;
              default = "/var/lib/surreal-obsidian-mcp/obsidian.db";
              description = "Path to the SurrealDB database file";
            };

            embedding = {
              provider = mkOption {
                type = types.enum [ "open-ai" "open-ai-compatible" "ollama" ];
                default = "ollama";
                description = "Embedding provider to use";
              };

              model = mkOption {
                type = types.str;
                default = "nomic-embed-text";
                description = "Embedding model name";
              };

              dimensions = mkOption {
                type = types.int;
                default = 768;
                description = "Embedding dimensions";
              };

              apiKeyFile = mkOption {
                type = types.nullOr types.path;
                default = null;
                description = ''
                  Path to file containing the API key for embedding provider.
                  Set to null for providers that don't need API keys (e.g., Ollama).
                  Example: /run/secrets/openai-api-key

                  For secret management, consider using:
                  - agenix: https://github.com/ryantm/agenix
                  - sops-nix: https://github.com/Mic92/sops-nix
                '';
                example = "/run/secrets/openai-api-key";
              };

              apiBase = mkOption {
                type = types.nullOr types.str;
                default = "http://localhost:11434";
                description = "API base URL for embedding provider";
              };
            };

            reranking = {
              enable = mkEnableOption "reranking support";

              provider = mkOption {
                type = types.enum [ "embedded" ];
                default = "embedded";
                description = "Reranking provider";
              };

              model = mkOption {
                type = types.str;
                default = "BAAI/bge-reranker-base";
                description = "Reranking model name";
              };

              modelCache = mkOption {
                type = types.nullOr types.str;
                default = "/var/lib/surreal-obsidian-mcp/model_cache";
                description = "Path to cache downloaded models";
              };

              topN = mkOption {
                type = types.int;
                default = 20;
                description = "Number of top results to return after reranking";
              };
            };

            sync = {
              watchForChanges = mkOption {
                type = types.bool;
                default = true;
                description = "Watch vault for file changes";
              };

              initialIndexing = mkOption {
                type = types.bool;
                default = true;
                description = "Perform initial indexing on startup";
              };

              batchSize = mkOption {
                type = types.int;
                default = 100;
                description = "Batch size for indexing operations";
              };
            };

            graph = {
              extractLinks = mkOption {
                type = types.bool;
                default = true;
                description = "Extract wiki-links from notes";
              };

              extractBacklinks = mkOption {
                type = types.bool;
                default = true;
                description = "Track backlinks between notes";
              };

              extractTags = mkOption {
                type = types.bool;
                default = true;
                description = "Extract tags from notes";
              };

              extractMentions = mkOption {
                type = types.bool;
                default = true;
                description = "Extract mentions from notes";
              };
            };

            user = mkOption {
              type = types.str;
              default = "surreal-obsidian-mcp";
              description = "User account under which the service runs";
            };

            group = mkOption {
              type = types.str;
              default = "surreal-obsidian-mcp";
              description = "Group under which the service runs";
            };
          };

          config = mkIf cfg.enable {
            users.users.${cfg.user} = {
              isSystemUser = true;
              group = cfg.group;
              description = "Surreal Obsidian MCP service user";
            };

            users.groups.${cfg.group} = {};

            systemd.services.surreal-obsidian-mcp = {
              description = "Surreal Obsidian MCP Server";
              wantedBy = [ "multi-user.target" ];
              after = [ "network.target" ];

              serviceConfig = {
                Type = "simple";
                User = cfg.user;
                Group = cfg.group;
                ExecStart = "${cfg.package}/bin/surreal-obsidian-mcp --config ${runtimeConfigPath}";
                Restart = "on-failure";
                RestartSec = "5s";
                RuntimeDirectory = "surreal-obsidian-mcp";

                # Hardening
                NoNewPrivileges = true;
                PrivateTmp = true;
                ProtectSystem = "strict";
                ProtectHome = true;
                ReadWritePaths = [
                  (dirOf cfg.databasePath)
                  cfg.vaultPath
                ] ++ optional (cfg.reranking.enable && cfg.reranking.modelCache != null) cfg.reranking.modelCache;

                # Security
                ProtectKernelTunables = true;
                ProtectKernelModules = true;
                ProtectControlGroups = true;
                RestrictAddressFamilies = [ "AF_UNIX" "AF_INET" "AF_INET6" ];
                RestrictNamespaces = true;
                LockPersonality = true;
                MemoryDenyWriteExecute = false; # Needed for JIT in some ML libraries
                RestrictRealtime = true;
                RestrictSUIDSGID = true;
                RemoveIPC = true;
                PrivateMounts = true;
              };

              preStart = ''
                # Ensure database directory exists
                mkdir -p $(dirname ${cfg.databasePath})

                # Ensure model cache exists if reranking is enabled
                ${optionalString (cfg.reranking.enable && cfg.reranking.modelCache != null) ''
                  mkdir -p ${cfg.reranking.modelCache}
                ''}

                # Create runtime config with API key injection
                echo '${configTemplate}' > ${runtimeConfigPath}

                ${optionalString (cfg.embedding.apiKeyFile != null) ''
                  # Inject API key from file
                  API_KEY=$(cat ${toString cfg.embedding.apiKeyFile})
                  ${pkgs.jq}/bin/jq '.embedding.api_key = $key' \
                    --arg key "$API_KEY" \
                    ${runtimeConfigPath} > ${runtimeConfigPath}.tmp
                  mv ${runtimeConfigPath}.tmp ${runtimeConfigPath}
                ''}

                # Ensure config is readable by service user
                chmod 600 ${runtimeConfigPath}
              '';
            };

            # Create state directory
            systemd.tmpfiles.rules = [
              "d /var/lib/surreal-obsidian-mcp 0750 ${cfg.user} ${cfg.group} -"
            ];
          };
        };
    in
    {
      # Export the NixOS module
      nixosModules.default = nixosModule;
    } // flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            cargo-watch
            cargo-edit
            cargo-outdated
            cargo-audit

            # Build essentials (needed for Rust linker and build scripts)
            gcc
            pkg-config
            openssl

            # Development tools
            git
            jq
          ];

          shellHook = ''
            echo "🦀 Rust development environment loaded"
            echo "📦 Cargo: $(cargo --version)"
            echo "🔧 Rustc: $(rustc --version)"
            echo ""
            echo "Available commands:"
            echo "  cargo build          - Build the project"
            echo "  cargo run            - Run the MCP server"
            echo "  cargo test           - Run tests"
            echo "  cargo watch -x run   - Auto-rebuild on changes"
          '';

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "surreal-obsidian-mcp";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          # Skip tests during Nix build due to SurrealDB serialization issues in sandbox
          doCheck = false;

          meta = with pkgs.lib; {
            description = "MCP server for indexing Obsidian vaults into SurrealDB";
            license = licenses.agpl3Plus;
            homepage = "https://github.com/jemilsson/surreal-obsidian-mcp";
            maintainers = [ ];
          };
        };
      }
    );
}
