use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use releasedock_core::{
    asset_matcher::InstallType,
    integrity::IntegrityStatus,
    manifest::{InstalledApp, LifecycleAction, ManifestStore, RollbackSnapshot},
    release_policy::{ReleaseChannel, ReleasePolicy},
};

#[test]
fn migrates_v3_manifest_to_v4_integrity_and_policy_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));
    std::fs::write(
        store.path(),
        r#"{
          "schema_version": 3,
          "apps": [{
            "id": "owner/project",
            "name": "project",
            "repo_url": "https://github.com/owner/project",
            "installed_version": "v1.0.0",
            "installed_at": "2026-07-21T10:20:30Z",
            "asset_name": "project-linux-x64.tar.gz",
            "install_path": "/tmp/project",
            "install_type": "Archive",
            "install_path_kind": "managedPath",
            "uninstall_supported": true
          }]
        }"#,
    )
    .unwrap();

    let manifest = store.load().unwrap();
    let app = &manifest.apps[0];

    assert_eq!(manifest.schema_version, 4);
    assert_eq!(app.release_policy, ReleasePolicy::default());
    assert_eq!(app.release_policy.channel, ReleaseChannel::Stable);
    assert_eq!(app.artifact_sha256, None);
    assert_eq!(app.integrity_status, None);
    assert_eq!(app.checksum_asset_name, None);
    assert_eq!(app.rollback, None);
    assert_eq!(app.managed_root, None);
}

#[test]
fn round_trips_v4_integrity_policy_and_managed_file_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));
    let mut app = InstalledApp::new(
        "owner/project",
        "project",
        "v2.0.0",
        "project.AppImage",
        PathBuf::from("/opt/releasedock/project.AppImage"),
    );
    app.install_type = InstallType::AppImage;
    app.launch_path = Some(PathBuf::from("/opt/releasedock/project.AppImage"));
    app.release_policy = ReleasePolicy {
        channel: ReleaseChannel::Prerelease,
        pinned_version: Some("v2.0.0".to_string()),
        ignored_versions: vec!["v1.9.0".to_string()],
    };
    app.artifact_sha256 = Some("a".repeat(64));
    app.integrity_status = Some(IntegrityStatus::VerifiedChecksum);
    app.checksum_asset_name = Some("project.AppImage.sha256".to_string());
    app.managed_root = Some(PathBuf::from("/opt/releasedock"));
    app.rollback = Some(RollbackSnapshot {
        version: "v1.9.0".to_string(),
        asset_name: "project.AppImage".to_string(),
        install_path: PathBuf::from("/opt/releasedock/project.AppImage"),
        launch_path: Some(PathBuf::from("/opt/releasedock/project.AppImage")),
        install_type: InstallType::AppImage,
        artifact_sha256: Some("b".repeat(64)),
        integrity_status: Some(IntegrityStatus::VerifiedChecksum),
        checksum_asset_name: Some("SHA256SUMS".to_string()),
        snapshot_path: PathBuf::from("/var/lib/releasedock/snapshots/project.AppImage"),
        installed_at: Utc.with_ymd_and_hms(2026, 7, 20, 9, 30, 0).unwrap(),
    });

    store.save_apps(&[app.clone()]).unwrap();
    let loaded = store.load().unwrap();

    assert_eq!(loaded.schema_version, 4);
    assert_eq!(loaded.apps, vec![app]);
}

#[test]
fn rollback_snapshot_can_describe_a_managed_directory() {
    let snapshot = RollbackSnapshot {
        version: "v1.0.0".to_string(),
        asset_name: "project.tar.gz".to_string(),
        install_path: PathBuf::from("/opt/releasedock/project"),
        launch_path: Some(PathBuf::from("/opt/releasedock/project/bin/project")),
        install_type: InstallType::Archive,
        artifact_sha256: None,
        integrity_status: Some(IntegrityStatus::RecordedOnly),
        checksum_asset_name: None,
        snapshot_path: PathBuf::from("/var/lib/releasedock/snapshots/project"),
        installed_at: Utc.with_ymd_and_hms(2026, 7, 20, 9, 30, 0).unwrap(),
    };

    let json = serde_json::to_string(&snapshot).unwrap();
    let restored: RollbackSnapshot = serde_json::from_str(&json).unwrap();

    assert_eq!(restored, snapshot);
    assert_eq!(restored.install_type, InstallType::Archive);
    assert!(restored.launch_path.unwrap().ends_with("bin/project"));
}

#[test]
fn lifecycle_actions_use_camel_case_v4_values() {
    for (action, expected) in [
        (LifecycleAction::Downgrade, "\"downgrade\""),
        (LifecycleAction::Rollback, "\"rollback\""),
        (LifecycleAction::PolicyChange, "\"policyChange\""),
    ] {
        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(json, expected);
        assert_eq!(
            serde_json::from_str::<LifecycleAction>(&json).unwrap(),
            action
        );
    }
}
