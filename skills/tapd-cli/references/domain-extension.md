# Organization-domain extensions

Keep organization-specific rules outside the reusable HTTP client. The public workspace contains `tapd-domain-demo` as a compact example.

Recommended dependency direction:

```text
organization domain crate
        ↓
    tapd-client
```

The domain crate should own:

- its configuration schema and validation;
- stable workspace/category/work-item identifiers that are configurable rather than hard-coded;
- business input types;
- conversion from business input to generic TAPD parameters;
- multi-call orchestration and post-write verification;
- domain-specific tests that ensure generated remote content contains only intended business information.

The generic `tapd-client` should continue to own HTTP transport, TAPD envelope decoding, lossless ID handling, upload primitives, URL construction, and workflow graph helpers.

Use a feature-gated CLI adapter when distributing the extension in the same binary. Keep the adapter thin: parse arguments, resolve the selected profile, deserialize the organization's configuration table, construct the domain service, and serialize its result.

Do not place real company identifiers, credentials, internal workspace IDs, private status names, or proprietary workflow rules in a public example.
