{ config, lib, pkgs, ... }:

let
  cfg = config.services.webby-runner;

  instanceOpts = { name, ... }: {
    options = {
      server = lib.mkOption {
        type = lib.types.str;
        example = "http://webby.example.com";
        description = "Webby server URL (http(s):// auto-rewritten to ws(s)://).";
      };

      name = lib.mkOption {
        type = lib.types.str;
        default = name;
        description = "Runner name; doubles as identity across reconnects. Defaults to the instance name.";
      };

      shell = lib.mkOption {
        type = lib.types.str;
        default = "bash";
        description = "Shell to spawn (ignored if `command` is set).";
      };

      command = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        example = "podman run --rm -it -v /var/lib/webby-runner/work:/work debian:stable bash";
        description = "Full command line to spawn instead of a shell; executed via `sh -c`.";
      };

      user = lib.mkOption {
        type = lib.types.str;
        default = "webby-runner";
        description = "System user to run the runner as.";
      };

      group = lib.mkOption {
        type = lib.types.str;
        default = "webby-runner";
        description = "System group for the runner user.";
      };

      workingDirectory = lib.mkOption {
        type = lib.types.path;
        default = "/var/lib/webby-runner";
        description = "Working directory for the runner (used as home for the service user).";
      };

      environment = lib.mkOption {
        type = lib.types.attrsOf lib.types.str;
        default = { };
        example = { CARGO_TARGET_DIR = "/var/lib/webby-runner/target"; };
        description = "Extra env vars for the spawned session (inherited by the shell).";
      };

      environmentFile = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Optional systemd EnvironmentFile for secrets.";
      };
    };
  };
in
{
  options.services.webby-runner = {
    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.webby or (throw ''
        services.webby-runner.package is not set and pkgs.webby is not available.
        Either import the webby overlay or set services.webby-runner.package explicitly.
      '');
      description = "The webby package to use.";
    };

    manageUsers = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Whether this module should create the service users/groups referenced
        by instances. Set to false when your instances point at existing
        accounts (e.g. your own uid) you manage elsewhere.
      '';
    };

    instances = lib.mkOption {
      type = lib.types.attrsOf (lib.types.submodule instanceOpts);
      default = { };
      description = ''
        Named runner instances. Each becomes a systemd service `webby-runner-<name>.service`.

        Example:

          services.webby-runner.instances = {
            raw = { server = "http://localhost:8080"; };
            sbx = {
              server = "http://localhost:8080";
              command = "podman run --rm -it -v /var/lib/webby-runner/sbx:/work debian:stable bash";
            };
          };
      '';
    };
  };

  config = lib.mkIf (cfg.instances != { }) (
    let
      instanceNames = lib.attrNames cfg.instances;
      # Collect distinct users/groups across instances (usually just one).
      userSet = lib.unique (map (n: cfg.instances.${n}.user) instanceNames);
      groupSet = lib.unique (map (n: cfg.instances.${n}.group) instanceNames);
    in
    {
      users.users = lib.mkIf cfg.manageUsers (lib.listToAttrs (map (u: {
        name = u;
        value = {
          isSystemUser = true;
          group = u;
          home = "/var/lib/${u}";
          createHome = false;
        };
      }) userSet));

      users.groups = lib.mkIf cfg.manageUsers (lib.listToAttrs (map (g: {
        name = g;
        value = { };
      }) groupSet));

      systemd.tmpfiles.rules = map (n:
        let i = cfg.instances.${n};
        in "d ${i.workingDirectory} 0750 ${i.user} ${i.group} - -"
      ) instanceNames;

      systemd.services = lib.listToAttrs (map (n:
        let i = cfg.instances.${n};
        in {
          name = "webby-runner-${n}";
          value = {
            description = "Webby runner (${n})";
            wantedBy = [ "multi-user.target" ];
            after = [ "network-online.target" ];
            wants = [ "network-online.target" ];

            environment = {
              TERM = "xterm-256color";
              COLORTERM = "truecolor";
            } // i.environment;

            serviceConfig = {
              Type = "exec";
              ExecStart =
                let
                  cmdFlag = if i.command != null then "--command ${lib.escapeShellArg i.command}" else "";
                  shellFlag = if i.command == null then "--shell ${lib.escapeShellArg i.shell}" else "";
                in
                "${cfg.package}/bin/webby runner ${lib.escapeShellArg i.server} --name ${lib.escapeShellArg i.name} ${shellFlag} ${cmdFlag}";
              User = i.user;
              Group = i.group;
              WorkingDirectory = i.workingDirectory;

              # Respawn shell exits (^D from browser) — same as the packaged unit.
              Restart = "always";
              RestartSec = "2s";

              EnvironmentFile = lib.mkIf (i.environmentFile != null) i.environmentFile;

              NoNewPrivileges = true;
              PrivateTmp = true;
              ProtectKernelTunables = true;
              ProtectKernelModules = true;
              ProtectControlGroups = true;
              LockPersonality = true;

              StandardOutput = "journal";
              StandardError = "journal";
            };
          };
        }
      ) instanceNames);
    }
  );
}
