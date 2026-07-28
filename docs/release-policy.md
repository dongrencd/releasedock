# Release Catalog and Policy

## GitHub Release Catalog

The core `release` module supports three shared GitHub API operations:

- fetch the latest published release;
- fetch one release page with a caller-selected page size capped at 100;
- fetch one release by its exact tag.

Pagination follows GitHub's `Link` response header. A page reports another page only when the header contains `rel="next"`. Catalog traversal stops when a page is empty, rejects a repeated complete page tag set, and fails explicitly if a next link would exceed 20 pages or 2,000 collected releases. Release tags are encoded as one URL path segment, so tags containing slashes or spaces are queried without changing the endpoint path. API calls keep the existing 20-second request timeout.

The `Release` model retains both `draft` and `prerelease`. Missing fields deserialize as `false` for compatibility with saved fixtures.

## Selection Policy

The `release_policy` module owns release selection independently of installation and asset matching:

- draft releases are never selected;
- the stable channel excludes prereleases;
- the prerelease channel allows both stable and prerelease entries;
- a pinned tag takes priority over ignored tags during automatic selection;
- without a pin, ignored tags are skipped and the first eligible entry in GitHub API order is selected;
- a manual tag overrides the policy for one selection without mutating it, but still cannot select a draft.

Upgrade direction uses the relative position of the current and target tags in the GitHub API response. It does not compare tag strings or require semantic-version formatting. A missing current tag yields an unknown direction.

## Artifact Integrity

The core `integrity` module discovers optional SHA-256 metadata next to the selected release asset. It recognizes these checksum assets case-insensitively:

- `<target-asset-name>.sha256`;
- `SHA256SUMS`;
- `SHA256SUMS.txt`;
- `checksums.txt`.

Checksum assets whose GitHub metadata reports more than 2 MiB are skipped. Recognized assets are also downloaded with a 2 MiB streaming limit, so a missing or incorrect `Content-Length` cannot bypass the boundary. This limit applies to successful and error response bodies. A target-specific checksum file may contain a bare 64-character hexadecimal digest or the standard GNU `digest [*]filename` format. GNU records are parsed as exactly one separator space followed by the text (` `) or binary (`*`) mode character; a later star remains part of the filename. Shared manifests use an exact, case-sensitive match against the target asset basename. Unsupported files and manifests without a matching entry produce a recorded-only integrity plan.

Malformed digests attached to the target are rejected. Because `<target>.sha256` explicitly claims the target, an empty file, malformed/comment-only content, or a record for another filename also fails closed. Identical values from multiple checksum assets are accepted, while conflicting values fail discovery. Discovery only records an expected digest; it does not make an artifact verified. The downloaded artifact must pass the core streaming SHA-256 file verifier before it can be recorded as `verifiedChecksum`. A mismatch reports both the expected and calculated values.

`InstallPlan` now carries an `IntegrityPlan`. Existing callers receive a `recordedOnly` plan without an expected digest, and can attach discovered metadata with `with_integrity`. The base core constructor does not add a warning note when upstream checksum metadata is absent; interactive callers such as the CLI add their own confirmation warning. The plan also carries a compatibility-defaulted release direction so installation lifecycle history can distinguish a downgrade from an ordinary update. Optional compatibility-defaulted fields carry the expected installed version and release policy for update selection, plus the target policy for a new install.

Every downloaded artifact is hashed before extraction or installer execution. When an expected digest is present, installation continues only after an exact SHA-256 verification and records `verifiedChecksum` plus the checksum asset name. Without an expected digest, the calculated digest is still persisted as `recordedOnly`. A mismatch keeps the download cache for diagnosis, records a failed lifecycle event, and does not modify the active managed install or installed-app record. The same verification ordering applies to system packages and external installers.

## Manifest Schema v4

Manifest schema v4 extends each installed application with:

- its `ReleasePolicy`, defaulting to the stable channel with no pin or ignored versions;
- the recorded artifact SHA-256, integrity status, and checksum asset name;
- an optional rollback snapshot containing the prior version, asset and install paths, launch path, install type, integrity metadata, snapshot path, and installation time.

The snapshot model describes both a managed file and a managed directory. Managed-local installation now uses the repository active directory at `apps/<owner>-<repo>` as one transaction boundary. New content is prepared in a sibling staging directory. During an update, the previous active directory is moved to `rollbacks/<owner>-<repo>/<unique>`, then the staged directory is promoted without symlinks. The installed-app record, target policy for a new install, and successful lifecycle event are written by one atomic manifest save. A promotion or manifest-save failure removes the new active content and restores the previous active directory and manifest state. A first installation has no rollback snapshot.

After a successful update, only the immediately previous active version remains as the manifest-referenced rollback snapshot. Before manifest persistence, an older snapshot is moved to an explicitly named stale tombstone so a save failure can restore its original path. After the manifest commit, deleting that tombstone is non-fatal garbage collection: an undeletable stale path is not referenced by the manifest, does not turn the committed install into a failed lifecycle event, and is retried before the next related managed operation. Snapshot metadata retains the previous version, asset and install paths, launch target, install type, integrity fields, and installation time. Updating or rolling back preserves the current repository release policy.

`rollback_repo` supports only managed-path records with a valid snapshot. The guarded variant also compares the previewed active version and the expected snapshot state under the manifest lock before any move. Snapshot state includes absence as well as the snapshot version and path, so a snapshot created during confirmation is rejected as a stale plan. When both previewed and current state have no snapshot, the guarded path returns the normal missing-snapshot error without moving files. It checks that active and snapshot paths exist and that file-type snapshot metadata resolves inside the active directory. File snapshots with a recorded digest are checked before restoration. Archive snapshots represent extracted directories, so their original archive digest is metadata only and is not misrepresented as a hash of the directory tree. Rollback exchanges active and snapshot contents: the restored version becomes active and the formerly active version becomes the sole snapshot, allowing the same operation to switch back. The app update and successful rollback event share one manifest save; move or save failures reverse the filesystem exchange.

Managed uninstall moves the repository active directory and rollback snapshot to tombstones before one manifest save. Any move failure leaves or restores the active paths without changing the installed-app record, and a save failure restores both tombstones. Physical tombstone deletion and empty-directory pruning happen after the commit as non-fatal garbage collection; undeletable tombstones are unreferenced and retried by a later related managed operation. System-package and external-installer behavior remains outside managed rollback because ReleaseDock cannot atomically reverse changes made by the operating-system installer.

Lifecycle history supports `downgrade`, `rollback`, and `policyChange` in addition to the existing actions. Loading schema v1 through v3 normalizes it to v4. The new policy, integrity, and rollback fields use compatibility defaults, while the existing legacy install-type, path-kind, launch-path, and uninstall normalization remains unchanged.

## CLI Release Lifecycle

`releasedock releases <owner/repo>` lists the first 100 non-draft releases, including when a fixture contains more entries. Stable releases are shown by default; `--include-prerelease` also shows prereleases, `--all` follows the bounded GitHub pagination rules above, and `--json` returns the filtered `Release` array. Text output includes the tag, stable or prerelease channel, and publication time.

`releasedock install` accepts `--version <tag>` for a one-time exact selection and `--prerelease` for automatic selection from both stable and prerelease releases. An exact version does not create a pin. A new installation uses the stable policy unless `--prerelease` is present, in which case the prerelease policy is committed with the app and install event in the same manifest save. Every newly generated install plan records `ExpectedAbsent`; update plans record `ExpectedInstalled` with the selected version and policy. The core checks that state before artifact access and retains its final manifest baseline comparison. A missing guard remains accepted only for compatibility with older serialized plans. Updating an existing app preserves its prior policy. Stable automatic installation keeps using GitHub's latest-release endpoint; prerelease selection follows the paginated catalog.

Installed repositories expose atomic policy commands:

- `releasedock pin <owner/repo> [tag]` pins the supplied tag, or the currently installed tag when omitted;
- `releasedock unpin <owner/repo>` removes the pin;
- `releasedock ignore <owner/repo> <tag>` and `unignore` update the ignored-tag set without duplicates;
- `releasedock channel <owner/repo> <stable|prerelease>` changes the automatic release channel.

Each effective policy change and its `policyChange` lifecycle event are written under the manifest lock in one atomic save. Repeating an already-applied command reports an unchanged result and does not add another lifecycle event.

`releasedock check` and `releasedock update` use the installed repository policy through `ReleaseSelector`. Pins, ignored releases, channels, and GitHub release order therefore produce the same eligible target in both commands. Check output names the current and selected target versions, exposes direction in JSON, and reports a pinned older target as `downgradeAvailable` instead of an update. `update` requires exactly one repository or `--all`; `--version <tag>` overrides the policy for one repository and conflicts with `--all`. Update plans capture the selected installed version and complete release policy. The core compares both under the lock immediately before artifact work, while the existing full manifest baseline comparison remains the final pre-commit guard. An app already at its eligible target is reported and skipped before asset matching or checksum discovery, so an unchanged release with no current-platform asset does not fail either single or bulk update. Other bulk entries continue through plan construction and installation. Direction is derived from catalog order, never from textual tag comparison.

`releasedock rollback <owner/repo>` prints the active-to-snapshot version transition before confirmation, then restores the snapshot through the guarded core transaction API. Non-interactive execution requires `--yes`. Repositories without a managed record are reported without changing the manifest, while records without a snapshot return the core rollback error.

`releasedock config get` retains the configuration JSON fields but emits `***redacted***` instead of a stored GitHub token. The value on disk is unchanged.

Live install and update plans discover checksum metadata after release and asset selection. A discovered digest and checksum asset name are attached to the plan and verified during installation. When upstream checksum metadata is absent, the CLI marks the plan as requiring confirmation and prints an unverified warning. Offline fixtures never fetch checksum assets, remain `recordedOnly`, and require the same confirmation. Checksum parsing errors or conflicting digests stop plan creation.

Plan previews always show release direction and integrity status. `--yes` skips only reading the confirmation response; it does not hide the preview or its warning. Single and bulk previews identify the checksum source that will be verified, or state that no upstream checksum was found. Successful install and update output, as well as text `list` output, shows the final `verifiedChecksum` or `recordedOnly` status, a shortened SHA-256 digest, and the checksum source when one exists.

## Desktop Release Lifecycle

The desktop inspector collects the first 100 non-draft releases for the selected repository, continuing across pages when drafts consume positions on an earlier page. Traversal rejects repeated pages and remains bounded to 20 pages and 2,000 releases. A user can preview an explicit tag, switch an installed repository between the stable and prerelease channels, pin or unpin the selected version, and ignore or restore the selected target. After a policy mutation, the inspector selects the eligible target returned by the refreshed dashboard rather than retaining an ignored or obsolete tag. These policy changes use the same atomic core manifest mutations and lifecycle events as the CLI. A tracked repository uses the selected channel as the target policy for its first installation. Dashboard selection uses the same bounded catalog traversal for tracked repositories and installed applications, so it can find a stable target or pinned version beyond a draft- or prerelease-only first page.

Install previews expose the selected target, upgrade or downgrade direction, integrity status, and checksum source. A discovered checksum is labeled as pending SHA-256 verification until installation verifies the downloaded bytes; absence of an expected checksum is labeled unverified and recorded-only. The preview intentionally omits repository and download URLs plus local install paths. Confirmation returns that complete path-free preview object. The Tauri backend first validates its `ExpectedAbsent` or `ExpectedInstalled` guard against the current manifest, then fetches the release again, repeats core release and asset selection, and discovers integrity metadata again. It compares repository, tag, asset, install type, management kind, system package manager, confirmation requirement, integrity, direction, guard, and target policy; explanatory notes may differ. Any other change is a stale preview, and execution uses only the rebuilt server plan. The frontend therefore cannot turn a stale preview or arbitrary path into an installation request. Request identity prevents a late preview response or rejection for one repository from replacing the selected repository's confirmation, error, or task status.

Rollback is available only for managed-path records with a manifest snapshot. The preview identifies the active version and snapshot version, while the snapshot path is retained only as an opaque identity check. During confirmation, the backend reloads the manifest, compares the complete preview identity, and constructs `RollbackGuard` from the current trusted record before calling the guarded core transaction. It never uses the frontend-provided path for filesystem access. System-package and external-installer records do not expose rollback.
