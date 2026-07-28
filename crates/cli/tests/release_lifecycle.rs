use std::{fs, path::Path};

use assert_cmd::Command;
use predicates::str::contains;
use releasedock_core::{
    integrity::sha256_file,
    manifest::{InstalledApp, LifecycleAction, ManifestStore},
    release_policy::{ReleaseChannel, ReleasePolicy},
};
use serde_json::{Value, json};

fn write_catalog(path: &Path) {
    fs::write(
        path,
        serde_json::to_vec_pretty(&json!([
            {
                "tag_name": "v4.0.0-draft",
                "name": "Draft",
                "published_at": "2026-07-24T10:20:30Z",
                "draft": true,
                "prerelease": false,
                "assets": []
            },
            {
                "tag_name": "v3.0.0-beta.1",
                "name": "Beta",
                "published_at": "2026-07-23T10:20:30Z",
                "draft": false,
                "prerelease": true,
                "assets": [{
                    "name": "demo-linux-x86_64.AppImage",
                    "browser_download_url": "https://example.invalid/v3/demo.AppImage",
                    "size": 16
                }]
            },
            {
                "tag_name": "v2.0.0",
                "name": "Stable two",
                "published_at": "2026-07-22T10:20:30Z",
                "draft": false,
                "prerelease": false,
                "assets": [{
                    "name": "demo-linux-x86_64.AppImage",
                    "browser_download_url": "https://example.invalid/v2/demo.AppImage",
                    "size": 16
                }]
            },
            {
                "tag_name": "v1.0.0",
                "name": "Stable one",
                "published_at": "2026-07-21T10:20:30Z",
                "draft": false,
                "prerelease": false,
                "assets": [{
                    "name": "demo-linux-x86_64.AppImage",
                    "browser_download_url": "https://example.invalid/v1/demo.AppImage",
                    "size": 16
                }]
            }
        ]))
        .unwrap(),
    )
    .unwrap();
}

fn configure_temp_root(temp: &tempfile::TempDir) -> std::path::PathBuf {
    let config = temp.path().join("config.json");
    fs::write(
        &config,
        serde_json::to_vec_pretty(&json!({
            "installRoot": temp.path().join("install-root")
        }))
        .unwrap(),
    )
    .unwrap();
    config
}

fn command(config: &Path) -> Command {
    let mut command = Command::cargo_bin("releasedock").unwrap();
    command.env("GHRM_CONFIG_PATH", config);
    command
}

fn install_fixture(config: &Path, manifest: &Path, catalog: &Path, artifact: &Path, version: &str) {
    command(config)
        .args([
            "install",
            "owner/project",
            "--version",
            version,
            "--release-fixture",
            catalog.to_str().unwrap(),
            "--artifact-fixture",
            artifact.to_str().unwrap(),
            "--manifest",
            manifest.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success();
}

#[test]
fn releases_filters_drafts_and_prereleases_and_outputs_json_arrays() {
    let temp = tempfile::tempdir().unwrap();
    let config = configure_temp_root(&temp);
    let catalog = temp.path().join("releases.json");
    write_catalog(&catalog);

    let default = command(&config)
        .args([
            "releases",
            "owner/project",
            "--release-fixture",
            catalog.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(default.status.success());
    let releases: Vec<Value> = serde_json::from_slice(&default.stdout).unwrap();
    assert_eq!(
        releases
            .iter()
            .map(|release| release["tag_name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["v2.0.0", "v1.0.0"]
    );

    let all_channels = command(&config)
        .args([
            "releases",
            "owner/project",
            "--release-fixture",
            catalog.to_str().unwrap(),
            "--include-prerelease",
        ])
        .output()
        .unwrap();
    assert!(all_channels.status.success());
    let text = String::from_utf8(all_channels.stdout).unwrap();
    assert!(text.contains("v3.0.0-beta.1 prerelease 2026-07-23T10:20:30+00:00"));
    assert!(text.contains("v2.0.0 stable 2026-07-22T10:20:30+00:00"));
    assert!(!text.contains("v4.0.0-draft"));
}

#[test]
fn install_version_selects_an_exact_fixture_tag_and_marks_unverified_plan() {
    let temp = tempfile::tempdir().unwrap();
    let config = configure_temp_root(&temp);
    let catalog = temp.path().join("releases.json");
    write_catalog(&catalog);

    command(&config)
        .args([
            "install",
            "owner/project",
            "--version",
            "v1.0.0",
            "--release-fixture",
            catalog.to_str().unwrap(),
            "--os",
            "linux",
            "--arch",
            "x64",
            "--json",
        ])
        .assert()
        .success()
        .stdout(contains("\"version\":\"v1.0.0\""))
        .stdout(contains("\"status\":\"recordedOnly\""))
        .stdout(contains("\"requires_user_confirmation\":true"))
        .stdout(contains("No upstream SHA-256 checksum"));

    command(&config)
        .args([
            "install",
            "owner/project",
            "--version",
            "v9.0.0",
            "--release-fixture",
            catalog.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .failure()
        .stderr(contains("release fixture does not contain tag `v9.0.0`"));
}

#[test]
fn yes_keeps_plan_warning_and_install_and_list_show_recorded_integrity() {
    let temp = tempfile::tempdir().unwrap();
    let config = configure_temp_root(&temp);
    let catalog = temp.path().join("releases.json");
    let manifest_path = temp.path().join("apps.json");
    let artifact = temp.path().join("demo-linux-x86_64.AppImage");
    write_catalog(&catalog);
    fs::write(&artifact, b"local test artifact").unwrap();
    let digest = sha256_file(&artifact).unwrap();
    let digest_prefix = &digest[..12];

    command(&config)
        .args([
            "install",
            "owner/project",
            "--version",
            "v1.0.0",
            "--release-fixture",
            catalog.to_str().unwrap(),
            "--artifact-fixture",
            artifact.to_str().unwrap(),
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success()
        .stdout(contains("Release direction: unknown"))
        .stdout(contains(
            "SHA-256: unverified; no upstream checksum was found",
        ))
        .stdout(contains(format!(
            "integrity=recordedOnly sha256={digest_prefix} source=none"
        )));

    command(&config)
        .args(["list", "--manifest", manifest_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains(format!(
            "integrity=recordedOnly sha256={digest_prefix} source=none"
        )));
}

#[test]
fn update_version_conflicts_with_all() {
    Command::cargo_bin("releasedock")
        .unwrap()
        .args(["update", "--all", "--version", "v2.0.0"])
        .assert()
        .failure()
        .stderr(contains("cannot be used with '--version <VERSION>'"));
}

#[test]
fn policy_commands_round_trip_and_no_ops_do_not_repeat_events() {
    let temp = tempfile::tempdir().unwrap();
    let config = configure_temp_root(&temp);
    let manifest_path = temp.path().join("apps.json");
    let store = ManifestStore::at_path(manifest_path.clone());
    store
        .save_apps(&[InstalledApp::new(
            "owner/project",
            "project",
            "v1.0.0",
            "demo.AppImage",
            temp.path().join("demo.AppImage"),
        )])
        .unwrap();

    for _ in 0..2 {
        command(&config)
            .args([
                "pin",
                "owner/project",
                "--manifest",
                manifest_path.to_str().unwrap(),
            ])
            .assert()
            .success()
            .stdout(contains("Pinned owner/project to v1.0.0"));
    }
    assert_eq!(store.load().unwrap().lifecycle_events.len(), 1);

    command(&config)
        .args([
            "ignore",
            "owner/project",
            "v2.0.0",
            "--manifest",
            manifest_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    command(&config)
        .args([
            "channel",
            "owner/project",
            "prerelease",
            "--manifest",
            manifest_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    let configured = store.load().unwrap();
    assert_eq!(
        configured.apps[0].release_policy,
        ReleasePolicy {
            channel: ReleaseChannel::Prerelease,
            pinned_version: Some("v1.0.0".into()),
            ignored_versions: vec!["v2.0.0".into()],
        }
    );

    command(&config)
        .args([
            "unignore",
            "owner/project",
            "v2.0.0",
            "--manifest",
            manifest_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    command(&config)
        .args([
            "unpin",
            "owner/project",
            "--manifest",
            manifest_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    let cleared = store.load().unwrap();
    assert_eq!(cleared.apps[0].release_policy.pinned_version, None);
    assert!(cleared.apps[0].release_policy.ignored_versions.is_empty());
    assert!(
        cleared
            .lifecycle_events
            .iter()
            .all(|event| event.action == LifecycleAction::PolicyChange)
    );
}

#[test]
fn check_uses_each_installed_apps_policy_and_reports_selector_errors() {
    let temp = tempfile::tempdir().unwrap();
    let config = configure_temp_root(&temp);
    let catalog = temp.path().join("releases.json");
    let manifest_path = temp.path().join("apps.json");
    write_catalog(&catalog);

    let mut pinned = InstalledApp::new(
        "owner/pinned",
        "pinned",
        "v1.0.0",
        "demo-linux-x86_64.AppImage",
        temp.path().join("pinned"),
    );
    pinned.release_policy.pinned_version = Some("v2.0.0".into());
    let mut prerelease = InstalledApp::new(
        "owner/prerelease",
        "prerelease",
        "v2.0.0",
        "demo-linux-x86_64.AppImage",
        temp.path().join("prerelease"),
    );
    prerelease.release_policy.channel = ReleaseChannel::Prerelease;
    let mut missing = InstalledApp::new(
        "owner/missing",
        "missing",
        "v1.0.0",
        "demo-linux-x86_64.AppImage",
        temp.path().join("missing"),
    );
    missing.release_policy.pinned_version = Some("v9.0.0".into());
    let mut downgrade = InstalledApp::new(
        "owner/downgrade",
        "downgrade",
        "v2.0.0",
        "demo-linux-x86_64.AppImage",
        temp.path().join("downgrade"),
    );
    downgrade.release_policy.pinned_version = Some("v1.0.0".into());
    ManifestStore::at_path(manifest_path.clone())
        .save_apps(&[pinned, prerelease, missing, downgrade])
        .unwrap();

    let output = command(&config)
        .args([
            "check",
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--release-fixture",
            catalog.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let apps = report["apps"].as_array().unwrap();

    assert_eq!(apps[0]["latest_version"], "v2.0.0");
    assert_eq!(apps[0]["status"], "updateAvailable");
    assert_eq!(apps[1]["latest_version"], "v3.0.0-beta.1");
    assert_eq!(apps[1]["status"], "updateAvailable");
    assert_eq!(apps[2]["status"], "fetchFailed");
    assert!(apps[2]["reason"].as_str().unwrap().contains("v9.0.0"));
    assert_eq!(apps[3]["latest_version"], "v1.0.0");
    assert_eq!(apps[3]["direction"], "downgrade");
    assert_eq!(apps[3]["status"], "downgradeAvailable");

    command(&config)
        .args([
            "check",
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--release-fixture",
            catalog.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains(
            "owner/downgrade [可降级] current v2.0.0 -> target v1.0.0",
        ));
}

#[test]
fn update_honors_pin_and_manual_version_override_and_skips_current_target() {
    let temp = tempfile::tempdir().unwrap();
    let config = configure_temp_root(&temp);
    let catalog = temp.path().join("releases.json");
    let manifest_path = temp.path().join("apps.json");
    let artifact = temp.path().join("demo-linux-x86_64.AppImage");
    write_catalog(&catalog);
    fs::write(&artifact, b"local test artifact").unwrap();
    install_fixture(&config, &manifest_path, &catalog, &artifact, "v1.0.0");

    command(&config)
        .args([
            "pin",
            "owner/project",
            "v2.0.0",
            "--manifest",
            manifest_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    command(&config)
        .args([
            "update",
            "owner/project",
            "--release-fixture",
            catalog.to_str().unwrap(),
            "--artifact-fixture",
            artifact.to_str().unwrap(),
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success()
        .stdout(contains("Updated owner/project to v2.0.0"));

    command(&config)
        .args([
            "update",
            "owner/project",
            "--version",
            "v3.0.0-beta.1",
            "--release-fixture",
            catalog.to_str().unwrap(),
            "--artifact-fixture",
            artifact.to_str().unwrap(),
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success()
        .stdout(contains("Updated owner/project to v3.0.0-beta.1"));

    command(&config)
        .args([
            "update",
            "owner/project",
            "--version",
            "v3.0.0-beta.1",
            "--release-fixture",
            catalog.to_str().unwrap(),
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success()
        .stdout(contains("Already at target version v3.0.0-beta.1"));
}

#[test]
fn update_all_reports_and_skips_apps_already_at_their_policy_target() {
    let temp = tempfile::tempdir().unwrap();
    let config = configure_temp_root(&temp);
    let catalog = temp.path().join("releases.json");
    let manifest_path = temp.path().join("apps.json");
    write_catalog(&catalog);
    ManifestStore::at_path(manifest_path.clone())
        .save_apps(&[InstalledApp::new(
            "owner/project",
            "project",
            "v2.0.0",
            "demo-linux-x86_64.AppImage",
            temp.path().join("missing-on-purpose"),
        )])
        .unwrap();

    // No artifact fixture is supplied. Success therefore proves the installer was skipped.
    command(&config)
        .args([
            "update",
            "--all",
            "--release-fixture",
            catalog.to_str().unwrap(),
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success()
        .stdout(contains(
            "Already at target version v2.0.0 for owner/project",
        ));
}

#[test]
fn update_skips_current_target_before_matching_release_assets() {
    let temp = tempfile::tempdir().unwrap();
    let config = configure_temp_root(&temp);
    let catalog = temp.path().join("no-assets.json");
    let manifest_path = temp.path().join("apps.json");
    fs::write(
        &catalog,
        serde_json::to_vec_pretty(&json!([{
            "tag_name": "v2.0.0",
            "draft": false,
            "prerelease": false,
            "assets": []
        }]))
        .unwrap(),
    )
    .unwrap();
    ManifestStore::at_path(manifest_path.clone())
        .save_apps(&[InstalledApp::new(
            "owner/project",
            "project",
            "v2.0.0",
            "old-linux-x86_64.AppImage",
            temp.path().join("missing-on-purpose"),
        )])
        .unwrap();

    command(&config)
        .args([
            "update",
            "owner/project",
            "--release-fixture",
            catalog.to_str().unwrap(),
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success()
        .stdout(contains(
            "Already at target version v2.0.0 for owner/project",
        ));
}

#[test]
fn update_all_skips_current_no_asset_target_without_blocking_other_updates() {
    let temp = tempfile::tempdir().unwrap();
    let config = configure_temp_root(&temp);
    let catalog = temp.path().join("policy-releases.json");
    let manifest_path = temp.path().join("apps.json");
    let artifact = temp.path().join("demo-linux-x86_64.AppImage");
    fs::write(&artifact, b"local test artifact").unwrap();
    fs::write(
        &catalog,
        serde_json::to_vec_pretty(&json!([
            {
                "tag_name": "v3.0.0-beta.1",
                "draft": false,
                "prerelease": true,
                "assets": []
            },
            {
                "tag_name": "v2.0.0",
                "draft": false,
                "prerelease": false,
                "assets": [{
                    "name": "demo-linux-x86_64.AppImage",
                    "browser_download_url": "https://example.invalid/v2/demo.AppImage",
                    "size": 16
                }]
            },
            {
                "tag_name": "v1.0.0",
                "draft": false,
                "prerelease": false,
                "assets": [{
                    "name": "demo-linux-x86_64.AppImage",
                    "browser_download_url": "https://example.invalid/v1/demo.AppImage",
                    "size": 16
                }]
            }
        ]))
        .unwrap(),
    )
    .unwrap();

    command(&config)
        .args([
            "install",
            "owner/update",
            "--version",
            "v1.0.0",
            "--release-fixture",
            catalog.to_str().unwrap(),
            "--artifact-fixture",
            artifact.to_str().unwrap(),
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success();
    let store = ManifestStore::at_path(manifest_path.clone());
    let mut apps = store.load().unwrap().apps;
    apps[0].release_policy.pinned_version = Some("v2.0.0".into());
    let mut skipped = InstalledApp::new(
        "owner/skip",
        "skip",
        "v3.0.0-beta.1",
        "old-linux-x86_64.AppImage",
        temp.path().join("skip-missing-on-purpose"),
    );
    skipped.release_policy.channel = ReleaseChannel::Prerelease;
    skipped.release_policy.pinned_version = Some("v3.0.0-beta.1".into());
    apps.push(skipped);
    store.save_apps(&apps).unwrap();

    command(&config)
        .args([
            "update",
            "--all",
            "--release-fixture",
            catalog.to_str().unwrap(),
            "--artifact-fixture",
            artifact.to_str().unwrap(),
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success()
        .stdout(contains(
            "Already at target version v3.0.0-beta.1 for owner/skip",
        ))
        .stdout(contains("Release direction: upgrade"))
        .stdout(contains(
            "SHA-256: unverified; no upstream checksum was found",
        ))
        .stdout(contains("Updated owner/update to v2.0.0"))
        .stdout(contains("integrity=recordedOnly"));
}

#[test]
fn prerelease_install_persists_channel_but_explicit_old_version_stays_stable() {
    let temp = tempfile::tempdir().unwrap();
    let config = configure_temp_root(&temp);
    let catalog = temp.path().join("releases.json");
    let beta_manifest = temp.path().join("beta-apps.json");
    let stable_manifest = temp.path().join("stable-apps.json");
    let stable_config = temp.path().join("stable-config.json");
    let artifact = temp.path().join("demo-linux-x86_64.AppImage");
    write_catalog(&catalog);
    fs::write(&artifact, b"local test artifact").unwrap();
    fs::write(
        &stable_config,
        serde_json::to_vec_pretty(&json!({
            "installRoot": temp.path().join("stable-install-root")
        }))
        .unwrap(),
    )
    .unwrap();

    command(&config)
        .args([
            "install",
            "owner/project",
            "--prerelease",
            "--release-fixture",
            catalog.to_str().unwrap(),
            "--artifact-fixture",
            artifact.to_str().unwrap(),
            "--manifest",
            beta_manifest.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success();
    let beta_stored = ManifestStore::at_path(beta_manifest).load().unwrap();
    assert_eq!(
        beta_stored.apps[0].release_policy.channel,
        ReleaseChannel::Prerelease
    );
    assert_eq!(
        beta_stored
            .lifecycle_events
            .iter()
            .filter(|event| event.action == LifecycleAction::Install)
            .count(),
        1
    );
    assert!(
        beta_stored
            .lifecycle_events
            .iter()
            .all(|event| event.action != LifecycleAction::PolicyChange)
    );

    install_fixture(
        &stable_config,
        &stable_manifest,
        &catalog,
        &artifact,
        "v1.0.0",
    );
    assert_eq!(
        ManifestStore::at_path(stable_manifest).load().unwrap().apps[0]
            .release_policy
            .channel,
        ReleaseChannel::Stable
    );
}

#[test]
fn rollback_requires_confirmation_reports_missing_snapshot_and_swaps_versions() {
    let temp = tempfile::tempdir().unwrap();
    let config = configure_temp_root(&temp);
    let catalog = temp.path().join("releases.json");
    let manifest_path = temp.path().join("apps.json");
    let artifact = temp.path().join("demo-linux-x86_64.AppImage");
    write_catalog(&catalog);
    fs::write(&artifact, b"local test artifact").unwrap();

    command(&config)
        .args([
            "rollback",
            "owner/project",
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success()
        .stdout(contains("No managed app matched owner/project"));
    install_fixture(&config, &manifest_path, &catalog, &artifact, "v1.0.0");

    command(&config)
        .args([
            "rollback",
            "owner/project",
            "--manifest",
            manifest_path.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(contains("use --yes"));
    command(&config)
        .args([
            "rollback",
            "owner/project",
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(contains("does not have a rollback snapshot"));

    install_fixture(&config, &manifest_path, &catalog, &artifact, "v2.0.0");
    command(&config)
        .args([
            "rollback",
            "owner/project",
            "--manifest",
            manifest_path.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(contains("Rollback owner/project: v2.0.0 -> v1.0.0"))
        .stderr(contains("use --yes"));
    command(&config)
        .args([
            "rollback",
            "owner/project",
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success()
        .stdout(contains("Rolled back owner/project from v2.0.0 to v1.0.0"));
}
