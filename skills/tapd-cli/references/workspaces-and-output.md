# Workspaces, output, and errors

Verify the target workspace before a sensitive or ambiguous write:

```bash
tapd-cli --profile PROFILE workspace get
```

Other workspace operations:

```bash
tapd-cli workspace get WORKSPACE_ID
tapd-cli workspace list --company-id COMPANY_ID
tapd-cli workspace members
tapd-cli workspace members --user USER --fields id,name,email
```

The default output is compact JSON. Use `--output json-pretty` for human inspection. JSON responses use:

```json
{"ok":true,"data":{}}
```

or:

```json
{"ok":false,"error":{"code":"command_failed","message":"..."}}
```

Treat a nonzero process exit or `ok=false` as failure. Do not infer success from partial stdout.

TAPD IDs must remain strings. Do not parse long IDs through a floating-point number type.
