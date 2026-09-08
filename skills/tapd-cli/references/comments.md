# Comments

List comments for an object:

```bash
tapd-cli comment list --entry-type stories --entry-id STORY_ID
```

Add a Markdown comment:

```bash
tapd-cli --dry-run comment add \
  --entry-type stories \
  --entry-id STORY_ID \
  --description-file ./comment.md

tapd-cli comment add \
  --entry-type stories \
  --entry-id STORY_ID \
  --description-file ./comment.md
```

Update an existing comment:

```bash
tapd-cli comment update COMMENT_ID --description 'Updated business content'
```

The comment author is the authenticated TAPD user. Do not attempt to impersonate another author, and do not add machine provenance or run identifiers unless the user explicitly requests that text.
