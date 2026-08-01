# Security

## Threat Model

A GitHub Release asset is not automatically trustworthy. The project may be compromised, a release asset may be replaced, and an installer may execute arbitrary code.

## Initial Policy

- Windows `.exe` / `.msi` files are never executed silently.
- Official tagged Windows release assets are Authenticode-signed through CI when the repository's Windows signing secrets are configured. Unsigned tagged builds are still published with an explicit CI warning and may show unknown-publisher or SmartScreen warnings.
- Tagged releases include `SHA256SUMS` for published artifacts. Install previews and completed records surface checksum status when upstream checksum metadata is available.
- Installers require a second confirmation step. The CLI exposes `--yes`, and the desktop app keeps an explicit confirm button.
- The desktop app shows an install preview before running the install.
- Linux `.deb` / `.rpm` installs through the system installer only keep traceable state. The local cache directory is not treated as the actual installed result.
- Windows system-installer uninstall opens the OS uninstall tool from the normal Uninstall confirmation. ReleaseDock does not directly delete the discovered system install directory.
- Windows system-installer discovery and automatic adoption only read ARP/Uninstall registry metadata. They do not assume the installer package path is the real app directory, and they do not call `UninstallString` during adoption.
- Managed-local updates create rollback snapshots, and rollback is available only for records with a trusted managed snapshot.
- Private tokens are only used for GitHub API requests, stay on the local machine, and are not sent back to the repository.
- Tokens must not be written to logs.

## Future Hardening

- Signature display
- Release author and tag display before download
- Raw release note display before update
