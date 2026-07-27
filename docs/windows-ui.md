# Windows UI

## Design Goal

The Windows first screen is a compact workbench, not a welcome page. Keep the most important decisions visible, move explanatory copy into buttons, fields, and hover text, and make it easy to judge which GitHub Release apps need attention before opening release notes or starting a task. The default UI language is English, and the settings page offers an English / Simplified Chinese switch. The bottom status strip and task feedback follow the selected UI language too. The inspector should show management mode and package manager labels for installed apps when those records exist.

## Layout

```text
┌────────────────────────────────────────────────────────────────────────────┐
│  status                     token  refresh                                 │
├───────────┬───────────────────────────────────────────────┬────────────────┤
│  rail     │  add   search  filter  select  clear  remove   │  details       │
│           │  ☐ LosslessCut    3.64    3.65    state   action│  release note  │
│           │  ☐ micro          2.0.14  2.0.14   state   action│  more info     │
├───────────┴───────────────────────────────────────────────┴────────────────┤
│  progress  █████████░░                                               refresh│
└────────────────────────────────────────────────────────────────────────────┘
```

## Pages

- Update manager: the default view. Keep the filter bar compact with All, Updates, Action needed, and Failed. Show current and no-release states in the list, but do not surface them as separate filters.
- Add GitHub repository: accept an `owner/repo` string or GitHub URL and add it to the tracked list.
- Filter tracked software: filter the local list only; do not search GitHub globally.
- Bulk actions: support row selection, compact select-all/clear controls, and bulk removal of uninstalled tracked items.
- Settings: GitHub token, install root, proxy, and language. Changes are saved automatically after editing, and the install root field shows the resolved default directory until the user overrides it.
- GitHub token: optional for public repositories; recommended for private repositories and frequent refreshes.
- Sidebar footer: show the product name and subtitle only. Do not use the lower-left area as a repository shortcut.
- Copy strategy: show data and state directly, keep button and field explanations in hover text, render release notes with simple Markdown structure, skip generated HTML comment anchors, and display asset and install path directly. The release note view should be larger than the metadata blocks and should separate headings, lists, tables, quotes, inline code, links, and body text.
- Hover placement: icon button help text should open to the right by default and flip left near the right edge, so the tooltip does not cover the relevant panel.
- Install locations: the detail panel should expose open-folder actions, and settings should expose open-root and restore-default actions.
- Managed installs should also expose an open-app action when a launch target can be resolved. AppImage entries can launch directly; archive entries should try an inferred executable first and fall back to opening the install location if no launch target is available. In the inspector, the secondary open-app button should only appear for update-available managed installs so the primary action does not repeat itself.
- Tracked repositories with no installable asset for the current platform should show `Open release` as the primary action instead of `Install`, so the user can inspect the release page without triggering an install preview.
- The inspector should label the asset row as `No installable asset for this platform` when the repository has no matched asset for the current OS.
- The inspector should not keep an empty secondary-action row after hiding duplicate actions, and the asset file plus install path rows should span the full inspector width so long names and paths stay readable.
- System installers: Windows `.exe` / `.msi` and Linux `.deb` / `.rpm` files require explicit confirmation and must not be treated like one-click installs. Successful installs should clean up the temporary download cache copy.
- System tray: closing the main window minimizes the app to the system tray instead of quitting. The tray menu provides "Check updates", "Open window", and "Quit" actions. Left-clicking the tray icon restores the window. The tray tooltip shows the pending update count ("ReleaseDock · 3 updates available") when there are new releases, or just "ReleaseDock" when everything is current.
- Windows desktop release builds should run as GUI subsystem binaries, so launching the app normally must not leave a console window behind. Windows shell-open actions should use the system shell rather than `cmd`.
- Background check: a background timer re-checks all tracked and installed repositories for new releases at a configurable interval. The settings page exposes a toggle ("Background check") and interval field ("Check interval" in minutes). The feature is enabled by default with a 30-minute interval. When new updates are found, a native system notification informs the user, and the top bar shows an "N updates available" pill until the next manual refresh or dashboard update clears it.

## Visuals

- Background: `#F5F7FA`
- Panels: `#FFFFFF`
- Primary: `#0F766E`
- Confirmation: `#B45309`
- Failure: `#B91C1C`
- Font: `Segoe UI`
- Version, path, and asset labels: `Cascadia Mono`

The layout should feel like a desktop operations tool, using a table and a detail inspector instead of a marketing-style card grid.
The right-hand inspector needs enough width for long asset names and install paths. Task progress belongs in a bottom status bar so downloads, installs, and uninstalls stay visible without displacing release notes, and indeterminate work should keep moving instead of freezing on a static bar. Dashboard refresh should stream item updates so the list can paint progressively instead of waiting for the full request to finish, and that refresh state should not overwrite install or uninstall progress. Zero-percent work should still show a visible sliver of progress, and failures should stay readable in the same strip. Repos without a published release should show a neutral state, not a failure. Release note tables should scroll horizontally instead of collapsing into unreadable text. The settings form should stay compact, with the install root presented as a standard row rather than a highlighted card.
The right-hand inspector should keep release note summary and version information visible without making the first screen too dense. The release note block should be visually dominant and capable of handling headings, paragraphs, lists, and code blocks. Install preview requests should fail within the shared GitHub timeout budget so the bottom status strip can leave the processing state on its own instead of hanging forever.
