# Attachments and inline images

List and upload story attachments:

```bash
tapd-cli attachment list --entry-id STORY_ID
tapd-cli --dry-run attachment upload --entry-id STORY_ID --file ./report.pdf
tapd-cli attachment upload --entry-id STORY_ID --file ./report.pdf
tapd-cli attachment download-url --id ATTACHMENT_ID
```

The CLI verifies an attachment by listing it after upload and matching its ID, object ID, type, and filename.

Upload a rich-text image independently:

```bash
tapd-cli image upload --file ./chart.png
tapd-cli image download-url --path '/tfl/pictures/.../chart.png'
```

To embed local images while creating or updating a story, use explicit placeholders in Markdown or HTML:

```markdown
![Chart]({{tapd-image:chart}})
```

Then map each placeholder to a local file:

```bash
tapd-cli story create \
  --name 'Business title' \
  --description-file ./description.md \
  --image chart=./chart.png
```

Repeat `--image NAME=PATH` for multiple images. The placeholder position determines the image position. Supported formats are PNG, GIF, JPG/JPEG, and BMP. The CLI validates all mappings before upload, verifies each uploaded path through TAPD, replaces the placeholders, and only then writes the story.

Do not silently upload a local path that appears in ordinary Markdown. Require the explicit `--image` mapping so the outbound file transfer is visible in the command.
