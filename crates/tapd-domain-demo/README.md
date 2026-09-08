# tapd-domain-demo

This crate is an organization-neutral example of building a domain workflow on top of `tapd-client`. It is intentionally small enough to copy and replace.

## What the example owns

- A domain configuration schema (`DemoRequestConfig`)
- Validation for required template placeholders
- A business input type (`DemoRequestInput`)
- A deterministic plan that maps business fields to TAPD story parameters
- A service that executes the plan through `TapdClient`
- A thin, feature-gated adapter in `tapd-cli`

## What stays in tapd-client

- Authentication headers and HTTP transport
- TAPD response-envelope decoding
- Lossless ID handling
- Upload and read-back primitives
- Workflow graph helpers
- TAPD URL construction

## Adapting it

1. Copy the crate under a new organization-neutral package name.
2. Replace the demo configuration and input types with real business concepts.
3. Keep stable workspace, category, work-item, template, and status identifiers configurable.
4. Generate only intended business content; do not add machine provenance unless it is a real requirement.
5. Add tests for parameter planning before implementing remote calls.
6. Add a thin CLI feature and command adapter without moving HTTP logic into the binary crate.
7. Keep credentials and private organization values out of source control.

The related CLI adapter is [`../tapd-cli/src/commands/demo.rs`](../tapd-cli/src/commands/demo.rs), and the public configuration shape is shown in [`../../examples/tapd-cli.example.toml`](../../examples/tapd-cli.example.toml).
