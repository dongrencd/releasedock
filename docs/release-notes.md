# Release Notes

## Goal

Before updating software from a GitHub Release, the user should be able to see what changed in that release. The GUI keeps the original release note content but renders the common Markdown structure so headings, paragraphs, lists, tables, quotes, links, inline code, checklist items, and images stay readable.

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
- the install action should read like an install step, not a generic view action
- downloaded cache files should be cleaned up after a successful install

The note view should be larger than the metadata sections and scroll internally when content is long. Headings should stand out from body text, tables should scroll horizontally, and code blocks and images should remain readable inside the inspector.

Task progress lives in the bottom status bar. It should keep moving for indeterminate work, show a visible sliver at `0%`, and clear itself after success or failure so the strip does not get stuck on a stale state.
Repositories without a published release should render as a neutral `No release` state in the GUI instead of surfacing a loading error.
Dashboard refresh progress should update the list incrementally, but it should not replace install or uninstall task progress while those tasks are active.

## Background Update Check

The desktop app checks for new releases in the background while it runs in the system tray:

- **Trigger**: closing the window hides it to the tray; a timer then re-checks repositories every N minutes (configurable, default 30).
- **Notification**: when new updates are found, a system notification is shown. The tray tooltip updates to "ReleaseDock · N updates available".
- **Top bar badge**: the main window also shows an "N updates available" pill in the top-right corner, so users who keep the window open still see the count at a glance.
- **Tray menu**: right-click offers "Check updates" (triggers an immediate refresh), "Open window" (restores the hidden window), and "Quit" (exits the app).
- **Settings**: the background check toggle and check interval are stored in the config and auto-saved like the other settings fields.

This background loop reuses the same GitHub release data path as the manual "Check updates" button, so results stay consistent. It does not auto-install anything — it only surfaces new releases so the user can decide.
