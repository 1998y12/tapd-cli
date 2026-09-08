# Authentication and configuration

Configuration contains non-secret values. Credentials are resolved in this order:

1. The selected profile's non-empty `token_env` environment variable.
2. The system credential store entry with service `tapd-cli` and account equal to the profile name.

Never ask the user to put a token in a command argument, output it, or save it in TOML.

Useful commands:

```bash
tapd-cli config path
tapd-cli config show
tapd-cli --profile PROFILE config validate

tapd-cli --profile PROFILE auth status
tapd-cli --profile PROFILE auth login
tapd-cli --profile PROFILE auth test
tapd-cli --profile PROFILE auth whoami
tapd-cli --profile PROFILE auth logout
```

`auth login` reads the token without terminal echo, verifies it, and then stores it. `auth login --from-env` migrates the configured environment variable after verification.

Configuration selection order:

1. `--config PATH`
2. `TAPD_CLI_CONFIG`
3. `./.tapd-cli.toml`
4. `$XDG_CONFIG_HOME/tapd-cli/config.toml`
5. `~/.config/tapd-cli/config.toml`

Profile selection order:

1. `--profile`
2. `TAPD_PROFILE`
3. `default_profile`
4. The only configured profile

Workspace overrides follow `--workspace-id`, `TAPD_WORKSPACE_ID`, then the selected profile's `workspace_id`.
