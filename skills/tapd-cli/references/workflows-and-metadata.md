# Workflows and metadata

Discover work item types, categories, templates, custom fields, and story field metadata before constructing unfamiliar writes:

```bash
tapd-cli metadata workitem-types
tapd-cli metadata categories
tapd-cli metadata templates --workitem-type-id TYPE_ID
tapd-cli metadata custom-fields --entity-type stories
tapd-cli metadata story-fields
```

Inspect workflow state and transition data:

```bash
tapd-cli workflow status-map --workitem-type-id TYPE_ID
tapd-cli workflow transitions --workitem-type-id TYPE_ID
tapd-cli workflow first-step --workitem-type-id TYPE_ID
tapd-cli workflow last-steps --workitem-type-id TYPE_ID
```

Use the safe transition command instead of directly guessing a status update:

```bash
tapd-cli --dry-run story transition STORY_ID --to 'Target status'
tapd-cli story transition STORY_ID --to 'Target status'
```

The command reads the story's actual work item type and current state, calculates a legal path, checks required transition fields, and verifies every applied step. Supply required values with repeated `--field FIELD=VALUE` when the dry run reports them.
