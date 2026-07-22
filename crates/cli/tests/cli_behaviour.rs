use assert_cmd::Command;
use predicates::str::contains;
use std::fs;

use ghrm_core::{
    asset_matcher::InstallType,
    manifest::{InstallPathKind, InstalledApp, ManifestStore},
};

#[test]
fn list_reports_empty_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let manifest = temp.path().join("apps.json");

    Command::cargo_bin("ghrm")
        .unwrap()
        .args(["list", "--manifest", manifest.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("No managed apps"));
}

#[test]
fn install_outputs_json_plan_from_fixture() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/release_windows.json"
    );

    Command::cargo_bin("ghrm")
        .unwrap()
        .args([
            "install",
            "owner/project",
            "--release-fixture",
            fixture,
            "--os",
            "windows",
            "--arch",
            "x64",
            "--json",
        ])
        .assert()
        .success()
        .stdout(contains("\"asset_name\":\"demo-windows-x64.exe\""))
        .stdout(contains("\"requires_user_confirmation\":true"));
}

#[test]
fn install_requires_yes_in_non_interactive_mode() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/release_windows.json"
    );
    let temp = tempfile::tempdir().unwrap();
    let manifest = temp.path().join("apps.json");

    Command::cargo_bin("ghrm")
        .unwrap()
        .env("GHRM_CONFIG_PATH", temp.path().join("config.json"))
        .args([
            "install",
            "owner/project",
            "--release-fixture",
            fixture,
            "--manifest",
            manifest.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(contains("请使用 --yes"));
}

#[test]
fn install_installs_appimage_from_fixture() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/release_windows.json"
    );
    let temp = tempfile::tempdir().unwrap();
    let manifest = temp.path().join("apps.json");
    let artifact = temp.path().join("demo-linux-x86_64.AppImage");
    fs::write(&artifact, b"fake appimage payload").unwrap();

    Command::cargo_bin("ghrm")
        .unwrap()
        .args([
            "install",
            "owner/project",
            "--release-fixture",
            fixture,
            "--artifact-fixture",
            artifact.to_str().unwrap(),
            "--manifest",
            manifest.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success()
        .stdout(contains("Installed owner/project v1.2.3"));

    let manifest_json = fs::read_to_string(&manifest).unwrap();
    assert!(manifest_json.contains("owner/project"));

    Command::cargo_bin("ghrm")
        .unwrap()
        .args([
            "uninstall",
            "owner/project",
            "--manifest",
            manifest.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("Uninstalled owner/project"));

    let manifest_json = fs::read_to_string(&manifest).unwrap();
    assert!(manifest_json.contains("\"apps\": []"));
}

#[test]
fn config_commands_round_trip_through_temp_path() {
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("config.json");

    Command::cargo_bin("ghrm")
        .unwrap()
        .env("GHRM_CONFIG_PATH", &config_path)
        .args(["config", "set", "github-token", "ghp_testtoken"])
        .assert()
        .success();

    let config_json = fs::read_to_string(&config_path).unwrap();
    assert!(config_json.contains("ghp_testtoken"));

    Command::cargo_bin("ghrm")
        .unwrap()
        .env("GHRM_CONFIG_PATH", &config_path)
        .args(["config", "get"])
        .assert()
        .success()
        .stdout(contains("ghp_testtoken"));

    Command::cargo_bin("ghrm")
        .unwrap()
        .env("GHRM_CONFIG_PATH", &config_path)
        .args(["config", "clear", "github-token"])
        .assert()
        .success();

    let config_json = fs::read_to_string(&config_path).unwrap();
    assert!(!config_json.contains("ghp_testtoken"));
}

#[test]
fn uninstall_rejects_system_installer_records() {
    let temp = tempfile::tempdir().unwrap();
    let manifest = temp.path().join("apps.json");
    let store = ManifestStore::at_path(manifest.clone());
    store
        .save_apps(&[InstalledApp::with_install_metadata(
            "owner/system",
            "system",
            "v1.0.0",
            "system-windows-x64.exe",
            temp.path().join("system/system-windows-x64.exe"),
            InstallType::WindowsInstaller,
            InstallPathKind::SystemInstaller,
            false,
        )])
        .unwrap();

    Command::cargo_bin("ghrm")
        .unwrap()
        .args([
            "uninstall",
            "owner/system",
            "--manifest",
            manifest.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(contains("system installer"));

    let manifest_json = fs::read_to_string(&manifest).unwrap();
    assert!(manifest_json.contains("owner/system"));
}

#[test]
fn check_reports_current_and_update_statuses_from_fixture() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/release_windows.json"
    );
    let temp = tempfile::tempdir().unwrap();
    let manifest_path = temp.path().join("apps.json");
    let store = ManifestStore::at_path(manifest_path.clone());
    store
        .save_apps(&[
            InstalledApp::new(
                "owner/current",
                "current",
                "v1.2.3",
                "demo-linux-x86_64.AppImage",
                temp.path().join("current"),
            ),
            InstalledApp::new(
                "owner/update",
                "update",
                "v1.0.0",
                "demo-linux-x86_64.AppImage",
                temp.path().join("update"),
            ),
        ])
        .unwrap();

    Command::cargo_bin("ghrm")
        .unwrap()
        .args([
            "check",
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--release-fixture",
            fixture,
            "--json",
        ])
        .assert()
        .success()
        .stdout(contains("\"status\":\"current\""))
        .stdout(contains("\"status\":\"updateAvailable\""));
}

#[test]
fn check_reports_missing_assets_from_fixture() {
    let temp = tempfile::tempdir().unwrap();
    let manifest_path = temp.path().join("apps.json");
    let fixture_path = temp.path().join("release.json");
    let store = ManifestStore::at_path(manifest_path.clone());
    store
        .save_apps(&[InstalledApp::new(
            "owner/missing",
            "missing",
            "v1.0.0",
            "missing-linux-x86_64.AppImage",
            temp.path().join("missing"),
        )])
        .unwrap();

    fs::write(
        &fixture_path,
        r#"{
  "tag_name": "v1.1.0",
  "name": "No asset release",
  "body": "Release note",
  "html_url": "https://github.com/owner/missing/releases/tag/v1.1.0",
  "published_at": "2026-07-21T10:20:30Z",
  "prerelease": false,
  "assets": []
}"#,
    )
    .unwrap();

    Command::cargo_bin("ghrm")
        .unwrap()
        .args([
            "check",
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--release-fixture",
            fixture_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("缺少资产"))
        .stdout(contains("no matching asset"));
}

#[test]
fn info_outputs_release_note_from_fixture() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/release_windows.json"
    );

    Command::cargo_bin("ghrm")
        .unwrap()
        .args(["info", "owner/project", "--release-fixture", fixture])
        .assert()
        .success()
        .stdout(contains("Stable release"))
        .stdout(contains("Fix crash and improve startup."))
        .stdout(contains("demo-windows-x64.exe"));
}
