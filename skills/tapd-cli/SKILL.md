---
name: tapd-cli
description: Manage TAPD workspaces, stories, comments, attachments, inline images, workflow transitions, and metadata through the tapd-cli command. Use when a user asks to inspect or change TAPD data. Do not use for unrelated project-management systems.
license: MIT
---

# TAPD CLI

Use `tapd-cli` as the execution engine for TAPD operations. Do not reproduce supported operations with ad-hoc HTTP requests.

## Core workflow

1. Confirm the executable is available with `tapd-cli --version` when availability is unknown.
2. Select the intended profile with `--profile`; do not guess a workspace for a write.
3. Use `tapd-cli auth status` to inspect local credential availability without exposing or requesting the token.
4. Prefer the default JSON output and inspect the top-level `ok` field before using `data`.
5. For a complex write, use `--dry-run` first when it can validate the intended payload without changing TAPD.
6. Perform a write only when the user requested the corresponding change. Do not add provenance, automation, test-run, or generated-by markers unless the user explicitly asks for that business content.
7. Report created or changed object IDs and URLs returned by the CLI.

## Route to references

- Authentication, profiles, configuration, and credential precedence: read [references/auth-and-config.md](references/auth-and-config.md).
- Workspace lookup and output/error handling: read [references/workspaces-and-output.md](references/workspaces-and-output.md).
- Story listing, filtering, creation, updates, and counts: read [references/stories.md](references/stories.md).
- Comments: read [references/comments.md](references/comments.md).
- Attachments and rich-text images: read [references/attachments-and-images.md](references/attachments-and-images.md).
- Workflow transitions and metadata discovery: read [references/workflows-and-metadata.md](references/workflows-and-metadata.md).
- Building an organization-specific domain crate: read [references/domain-extension.md](references/domain-extension.md).

Read only the references needed for the current request.
