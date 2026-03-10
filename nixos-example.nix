# Example NixOS configuration for Surreal Obsidian MCP
#
# Add this to your NixOS configuration to run the MCP server as a system service
{
  # Import the flake in your flake.nix inputs:
  # inputs.surreal-obsidian-mcp.url = "github:jemilsson/surreal-obsidian-mcp";

  # Then in your configuration:
  imports = [
    inputs.surreal-obsidian-mcp.nixosModules.default
  ];

  services.surreal-obsidian-mcp = {
    enable = true;

    # Path to your Obsidian vault
    vaultPath = "/home/user/Documents/ObsidianVault";

    # Database location (default: /var/lib/surreal-obsidian-mcp/obsidian.db)
    databasePath = "/var/lib/surreal-obsidian-mcp/obsidian.db";

    # Embedding configuration - using local Ollama (recommended)
    embedding = {
      provider = "ollama";
      model = "nomic-embed-text";
      dimensions = 768;
      apiBase = "http://localhost:11434";
      # apiKeyFile not needed for Ollama (leave as null)
    };

    # Optional: Enable reranking for better search quality
    reranking = {
      enable = true;
      provider = "embedded";
      model = "BAAI/bge-reranker-base";
      modelCache = "/var/lib/surreal-obsidian-mcp/model_cache";
      topN = 20;
    };

    # Sync configuration
    sync = {
      watchForChanges = true;   # Auto-reindex on file changes
      initialIndexing = true;    # Index on startup
      batchSize = 100;
    };

    # Graph extraction settings
    graph = {
      extractLinks = true;       # Extract [[wiki-links]]
      extractBacklinks = true;   # Track what links to each note
      extractTags = true;        # Extract #tags
      extractMentions = true;    # Extract @mentions
    };

    # Service user/group (defaults shown)
    # user = "surreal-obsidian-mcp";
    # group = "surreal-obsidian-mcp";
  };

  # If using Ollama, you may want to enable it as a service too:
  # services.ollama = {
  #   enable = true;
  #   acceleration = "cuda"; # or "rocm" for AMD GPUs
  # };
}

# Alternative configurations:

## Using OpenAI
# services.surreal-obsidian-mcp = {
#   enable = true;
#   vaultPath = "/home/user/Documents/ObsidianVault";
#
#   embedding = {
#     provider = "open-ai";
#     model = "text-embedding-3-small";
#     dimensions = 1536;
#     apiKeyFile = "/run/secrets/openai-api-key";  # Use agenix or sops-nix
#     apiBase = "https://api.openai.com/v1";
#   };
#
#   reranking.enable = false;  # Cloud-only setup
# };

## Using Venice.ai
# services.surreal-obsidian-mcp = {
#   enable = true;
#   vaultPath = "/home/user/Documents/ObsidianVault";
#
#   embedding = {
#     provider = "open-ai-compatible";
#     model = "text-embedding-bge-m3";
#     dimensions = 1024;
#     apiKeyFile = "/run/secrets/venice-api-key";
#     apiBase = "https://api.venice.ai/api/v1";
#   };
#
#   reranking = {
#     enable = true;
#     provider = "embedded";
#     model = "BAAI/bge-reranker-base";
#   };
# };

# Secret management best practices:
#
# 1. Use secret management for API keys (required):
#    - agenix: https://github.com/ryantm/agenix
#    - sops-nix: https://github.com/Mic92/sops-nix
#
# 2. Example with agenix:
#    age.secrets.openai-api-key.file = ./secrets/openai-api-key.age;
#    services.surreal-obsidian-mcp.embedding.apiKeyFile = config.age.secrets.openai-api-key.path;
#
# 3. Example with sops-nix:
#    sops.secrets."openai-api-key" = {};
#    services.surreal-obsidian-mcp.embedding.apiKeyFile = config.sops.secrets."openai-api-key".path;
#
# 4. Restrict vault access by adding the service user to a group:
#    users.users.surreal-obsidian-mcp.extraGroups = [ "obsidian-users" ];
#
# 5. The service runs with security hardening enabled by default
