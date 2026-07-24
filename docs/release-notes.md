# Release Notes

## Goal

Before updating software from a GitHub Release, the user should be able to see what changed in that release. The first version keeps the original release note content, but the GUI renders the common Markdown structure so headings, paragraphs, lists, and code blocks stay readable.

## Data Source

The GitHub Releases API returns:

- `name`: release title
- `tag_name`: version tag
- `body`: raw release note text
- `html_url`: GitHub release page
- `published_at`: publish time
- `assets`: release asset list

## CLI Behavior

`info owner/repo` shows:

- repository URL
- release title and tag
- publish time
- release page URL
- raw release note text
- asset list

If the release note is empty, show:

```text
This release does not include a release note.
```

## GUI Behavior

When the user selects an app in the update manager, the right-hand inspector shows:

- release title
- current version and latest version
- publish time
- rendered release note content
- install preview with the selected asset and confirmation prompt
- asset file
- install path or installer file, depending on install type
- open release page action
- copy release note action

The note view should be larger than the metadata sections and scroll internally when content is long. Headings should stand out from body text, and code blocks should remain readable inside the inspector.
