# Stories

List stories; the default page contains 30 records and the maximum page size is 200:

```bash
tapd-cli story list
tapd-cli story list --limit 100 --page 2 --order 'modified desc'
tapd-cli story list --status STATUS --owner USER
tapd-cli story list --filter FIELD=VALUE
```

Use `story count` with the same filters when total size matters:

```bash
tapd-cli story count --status STATUS
```

Read one story:

```bash
tapd-cli story get STORY_ID
```

Create from Markdown:

```bash
tapd-cli --dry-run story create \
  --name 'Business title' \
  --description-file ./description.md

tapd-cli story create \
  --name 'Business title' \
  --description-file ./description.md
```

Common create options include `--owner`, repeated `--cc`, `--developer`, `--priority`, `--iteration-id`, `--parent-id`, `--category-id`, `--workitem-type-id`, `--template-id`, `--label`, `--begin`, `--due`, and repeated `--custom-field FIELD=VALUE`.

Update only the fields explicitly supplied:

```bash
tapd-cli --dry-run story update STORY_ID --owner USER
tapd-cli story update STORY_ID --description-file ./replacement.md
```

Supplying a description replaces the full description. Read and preserve existing content first when the user asks for a partial edit.
