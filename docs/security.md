# Security

## Threat Model

A GitHub Release asset is not automatically trustworthy. The project may be compromised, a release asset may be replaced, and an installer may execute arbitrary code.

## Initial Policy

- Windows `.exe` / `.msi` files are never executed silently.
- Installers require a second confirmation step. The CLI exposes `--yes`, and the desktop app keeps an explicit confirm button.
- The desktop app shows an install preview before running the install.
- Linux `.deb` / `.rpm` installs through the system installer only keep traceable state. The local cache directory is not treated as the actual installed result.
- Private tokens are only used for GitHub API requests, stay on the local machine, and are not sent back to the repository.
- Tokens must not be written to logs.

## Future Hardening

- SHA256 verification
- Signature display
- Release author and tag display before download
- Raw release note display before update
- Rollback on failed updates
