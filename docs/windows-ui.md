# Windows UI

## Design Goal

The Windows first screen is a compact workbench, not a welcome page. Keep the most important decisions visible, move explanatory copy into buttons, fields, and hover text, and make it easy to judge which GitHub Release apps need attention before opening release notes or starting a task. The default UI language is English, and the settings page offers an English / Simplified Chinese switch.

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

- Update manager: the default view. Show only items that need attention, failed items, and items that need asset selection.
- Add GitHub repository: accept an `owner/repo` string or GitHub URL and add it to the tracked list.
- Filter tracked software: filter the local list only; do not search GitHub globally.
- Bulk actions: support row selection, select-all for the current result set, and bulk removal of uninstalled tracked items.
- Current project: seed the current repository on first launch so the project release is visible immediately.
- Settings: GitHub token, install root, proxy, and language.
- GitHub token: optional for public repositories; recommended for private repositories and frequent refreshes.
- Copy strategy: show data and state directly, keep button and field explanations in hover text, show the release note summary by default, and display asset, install path, and uninstall capability directly.
- Hover placement: icon button help text should open to the right by default and flip left near the right edge, so the tooltip does not cover the relevant panel.
- Install locations: the detail panel should expose open-folder actions, Windows system installers should expose a system-uninstall action, and settings should expose open-root and restore-default actions.
- System installers: Windows `.exe` / `.msi` and Linux `.deb` / `.rpm` files require explicit confirmation and must not be treated like one-click installs.

## Visuals

- Background: `#F5F7FA`
- Panels: `#FFFFFF`
- Primary: `#0F766E`
- Confirmation: `#B45309`
- Failure: `#B91C1C`
- Font: `Segoe UI`
- Version, path, and asset labels: `Cascadia Mono`

The layout should feel like a desktop operations tool, using a table and a detail inspector instead of a marketing-style card grid.
The right-hand inspector needs enough width for long asset names and install paths. Task progress belongs in the inspector, not in a bottom status bar.
The right-hand inspector should keep release note summary, version information, asset, path, and uninstall capability visible without making the first screen too dense.
