# Windows UI

## Design Goal

The Windows first screen is a compact workbench, not a welcome page. Keep the most important decisions visible, move explanatory copy into buttons, fields, and hover text, and make it easy to judge which GitHub Release apps need attention before opening release notes or starting a task. The default UI language is English, and the settings page offers an English / Simplified Chinese switch. The bottom status strip and task feedback follow the selected UI language too. The inspector should show management mode and package manager labels for installed apps when those records exist, and it should also surface recent lifecycle history for each repo when that history is available.

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

- Update manager: the default view. Keep the filter and bulk-action tools compact on one toolbar row where space allows, with All, Updates, Action needed, and Failed. Show current and no-release states in the list, but do not surface them as separate filters.
- Top bar: keep the page title left-aligned and place configuration status plus page actions in a single right-aligned cluster. The update manager may show the token pill, update-count pill, and Check updates button together; settings should keep the top-right area minimal and surface configuration state in the sidebar instead of pinning a lone token pill beside the title.
- Add GitHub repository: keep the add-repo row in the list workbench header, next to the managed-apps summary. Accept an `owner/repo` string or GitHub URL and add it to the tracked list.
- Empty dashboard: when there are no managed apps yet, collapse the workbench to a single wide panel and prioritize the add-repo input plus the no-apps hint. Do not reserve a right-hand inspector column for the empty state.
- Filter tracked software: filter the local list only; do not search GitHub globally.
- Bulk actions: support row selection, compact select-all/clear controls, and bulk removal of uninstalled tracked items.
- Settings: GitHub token, install root, proxy, and language. Changes are saved automatically after editing, and the install root field shows the resolved default directory until the user overrides it. On wide windows, use a left editing column and a right summary/actions rail; collapse to one column on narrow windows.
- GitHub token: optional for public repositories; recommended for private repositories and frequent refreshes.
- Sidebar footer: show the product name and subtitle only. Do not use the lower-left area as a repository shortcut.
- Copy strategy: show data and state directly, keep button and field explanations in hover text, render release notes with simple Markdown structure, skip generated HTML comment anchors, and display asset and install path directly. The release note view should be larger than the metadata blocks and should separate headings, lists, tables, quotes, inline code, links, and body text.
- Hover placement: icon button help text should open to the right by default and flip left near the right edge, so the tooltip does not cover the relevant panel.
- Install locations: the detail panel should expose open-folder actions, and settings should expose open-root and restore-default actions. Managed installs should open the containing folder, while system installers should open the installer file itself.
- Managed installs should also expose an open-app action when a launch target can be resolved. AppImage entries can launch directly; archive entries should try an inferred executable first and fall back to opening the install location if no launch target is available. Bare Linux `Executable` entries are treated as managed local installs without a GUI launch target, so they fall back to opening the install location instead of offering a launch action. In the inspector, the secondary open-app button should only appear for update-available managed installs so the primary action does not repeat itself, and the secondary actions should use a compact two-column layout instead of a long vertical stack.
- Tracked repositories with no installable asset for the current platform should show `Open release` as the primary action instead of `Install`, so the user can inspect the release page without triggering an install preview.
- Install preview: once the preview opens, keep only `Cancel` and `Confirm install` in the action area. Show management kind and package manager as compact meta rows instead of nested cards. Treat the preview card and the matching row action as a temporary amber confirmation state.
- Right-side actions: keep the inspector commands visually layered, with a filled primary action, lighter secondary actions, and restrained danger actions. The install preview confirmation button should stay in the amber confirmation palette instead of reusing the regular green primary style.
- The inspector should label the asset row as `No installable asset for this platform` when the repository has no matched asset for the current OS.
- The inspector should not keep an empty secondary-action row after hiding duplicate actions, and the asset file plus install path rows should span the full inspector width so long names and paths stay readable.
- The list row action column is clickable. For `needsChoice` items it launches the same install-preview flow as the inspector primary button, so the user can start installation from the table instead of hunting for the right-side control.
- For `needsChoice` items, the inspector should bring the action controls above release metadata and release notes so the install decision is visible before explanatory content.
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

The layout should feel like a desktop operations tool, using a table and a detail inspector instead of a marketing-style card grid. The add-repo control, list summary, search, filters, and bulk actions should read as one workbench rather than separate cards.
The right-hand inspector needs enough width for long asset names and install paths. Task progress belongs in a bottom status bar so downloads, installs, and uninstalls stay visible without displacing release notes, and indeterminate work should keep moving instead of freezing on a static bar. Determinate download and install progress should fill from left to right at the reported percentage without a sliding animation, and the percentage label should stay fully visible. Dashboard refresh should stream item updates so the list can paint progressively instead of waiting for the full request to finish, and that refresh state should not overwrite install or uninstall progress. Zero-percent work should still show a visible sliver of progress, and failures should stay readable in the same strip. Repos without a published release should show a neutral state, not a failure. Release note tables should scroll horizontally instead of collapsing into unreadable text. The settings form should stay compact, with the install root presented as a standard row rather than a highlighted card, and the first screen should avoid repeated page titles or tall tool blocks that push the list too far down.
The right-hand inspector should keep release note summary and version information visible without making the first screen too dense. The release note block should be visually dominant and capable of handling headings, paragraphs, lists, and code blocks. Install preview requests should fail within the shared GitHub timeout budget so the bottom status strip can leave the processing state on its own instead of hanging forever. If an install fails, keep the preview open and relabel the confirmation action as retry install so the user can immediately try again. The bottom status strip keeps the task description, percentage chip, and progress bar on separate lanes so the percentage stays readable while the fill still runs edge to edge.
