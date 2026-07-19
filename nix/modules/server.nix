{ config, lib, pkgs, ... }:

let
  cfg = config.services.webby-server;
in
{
  options.services.webby-server = {
    enable = lib.mkEnableOption "the Webby server";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.webby or (throw ''
        services.webby-server.package is not set and pkgs.webby is not available.
        Either import the webby overlay (nixpkgs.overlays = [ inputs.webby.overlays.default ];)
        or set services.webby-server.package explicitly.
      '');
      description = "The webby package to use.";
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 8080;
      description = "TCP port to listen on.";
    };

    address = lib.mkOption {
      type = lib.types.str;
      default = "0.0.0.0";
      description = "Address to bind. Use 127.0.0.1 if you're fronting this with a reverse proxy.";
    };

    dataDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/webby";
      description = "Directory holding the SQLite database and any runtime state.";
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "webby";
      description = "System user to run the server as.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "webby";
      description = "System group for the server user.";
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open the server port in the firewall.";
    };

    environment = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      example = {
        RUST_LOG = "info";
        VAPID_PUBLIC_KEY = "…";
      };
      description = "Extra environment variables. Sensitive values should come via environmentFile.";
    };

    environmentFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = ''
        Path to an EnvironmentFile with secrets (VAPID_PRIVATE_KEY, etc.).
        Loaded by systemd; not world-readable.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    users.users.${cfg.user} = {
      isSystemUser = true;
      group = cfg.group;
      home = cfg.dataDir;
      createHome = false;
    };
    users.groups.${cfg.group} = { };

    systemd.tmpfiles.rules = [
      "d ${cfg.dataDir} 0750 ${cfg.user} ${cfg.group} - -"
    ];

    networking.firewall.allowedTCPPorts = lib.mkIf cfg.openFirewall [ cfg.port ];

    systemd.services.webby-server = {
      description = "Webby SSR server";
      wantedBy = [ "multi-user.target" ];
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];

      environment = {
        DATABASE_URL = "sqlite://${cfg.dataDir}/webby.db";
        LEPTOS_SITE_ADDR = "${cfg.address}:${toString cfg.port}";
        LEPTOS_SITE_ROOT = "${cfg.package}/share/webby/site";
        RUST_LOG = "info";
      } // cfg.environment;

      serviceConfig = {
        Type = "exec";
        ExecStart = "${cfg.package}/bin/webby serve";
        User = cfg.user;
        Group = cfg.group;
        WorkingDirectory = cfg.dataDir;
        Restart = "on-failure";
        RestartSec = "5s";

        EnvironmentFile = lib.mkIf (cfg.environmentFile != null) cfg.environmentFile;

        # Hardening
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictSUIDSGID = true;
        LockPersonality = true;
        ReadWritePaths = [ cfg.dataDir ];

        StandardOutput = "journal";
        StandardError = "journal";
      };
    };
  };
}
