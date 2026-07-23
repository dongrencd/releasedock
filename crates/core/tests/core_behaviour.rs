use ghrm_core::{
    asset_matcher::{Architecture, AssetMatcher, OperatingSystem},
    install_plan::InstallPlan,
    manifest::{InstallPathKind, InstalledApp, ManifestStore},
    release::{Release, ReleaseAsset},
    repo::RepoRef,
};

#[test]
fn parses_owner_repo_and_github_urls() {
    assert_eq!(
        RepoRef::parse("owner/project").unwrap().id(),
        "owner/project"
    );
    assert_eq!(
        RepoRef::parse("https://github.com/owner/project/releases")
            .unwrap()
            .id(),
        "owner/project"
    );
}

#[test]
fn rejects_non_github_urls() {
    let err = RepoRef::parse("https://gitlab.com/owner/project").unwrap_err();
    assert!(err.to_string().contains("GitHub"));
}

#[test]
fn prefers_windows_x64_installer_over_zip() {
    let release = Release::fixture(
        "v1.2.3",
        vec![
            ReleaseAsset::fixture("demo-linux-x86_64.AppImage"),
            ReleaseAsset::fixture("demo-windows-x64.zip"),
            ReleaseAsset::fixture("demo-windows-x64.exe"),
        ],
    );

    let matched = AssetMatcher::new(OperatingSystem::Windows, Architecture::X64)
        .select_best(&release)
        .unwrap();

    assert_eq!(matched.asset.name, "demo-windows-x64.exe");
    assert!(matched.score > 0);
}

#[test]
fn prefers_linux_appimage_for_desktop_apps() {
    let release = Release::fixture(
        "v2.0.0",
        vec![
            ReleaseAsset::fixture("demo-linux-amd64.tar.gz"),
            ReleaseAsset::fixture("demo-x86_64.AppImage"),
            ReleaseAsset::fixture("demo-windows-x64.exe"),
        ],
    );

    let matched = AssetMatcher::new(OperatingSystem::Linux, Architecture::X64)
        .select_best(&release)
        .unwrap();

    assert_eq!(matched.asset.name, "demo-x86_64.AppImage");
}

#[test]
fn writes_and_reads_manifest_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));

    store
        .save_apps(&[InstalledApp::new(
            "owner/project",
            "project",
            "v1.0.0",
            "project-windows-x64.zip",
            temp.path().join("project"),
        )])
        .unwrap();

    let manifest = store.load().unwrap();
    assert_eq!(manifest.schema_version, 2);
    assert_eq!(manifest.apps[0].id, "owner/project");
    assert_eq!(manifest.apps[0].installed_version, "v1.0.0");
}

#[test]
fn upserts_and_removes_manifest_entries() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));

    store
        .upsert_app(InstalledApp::new(
            "owner/project",
            "project",
            "v1.0.0",
            "project-linux-x86_64.tar.gz",
            temp.path().join("project"),
        ))
        .unwrap();
    store
        .upsert_app(InstalledApp::new(
            "owner/project",
            "project",
            "v1.1.0",
            "project-linux-x86_64.tar.gz",
            temp.path().join("project"),
        ))
        .unwrap();

    let manifest = store.load().unwrap();
    assert_eq!(manifest.apps.len(), 1);
    assert_eq!(manifest.apps[0].installed_version, "v1.1.0");

    let removed = store.remove_app("owner/project").unwrap();
    assert!(removed.is_some());
    assert!(store.load().unwrap().apps.is_empty());
}

#[test]
fn upgrades_legacy_manifest_entries_to_current_install_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));
    std::fs::write(
        store.path(),
        r#"{
          "schema_version": 1,
          "apps": [
            {
              "id": "owner/project",
              "name": "project",
              "repo_url": "https://github.com/owner/project",
              "installed_version": "v1.0.0",
              "installed_at": "2026-07-21T10:20:30Z",
              "asset_name": "project-windows-x64.exe",
              "install_path": "/tmp/project/project-windows-x64.exe"
            }
          ]
        }"#,
    )
    .unwrap();

    let manifest = store.load().unwrap();
    assert_eq!(manifest.schema_version, 2);
    assert!(!manifest.apps[0].uninstall_supported);
    assert_eq!(
        manifest.apps[0].install_path_kind,
        ghrm_core::manifest::InstallPathKind::SystemInstaller
    );
}

#[test]
fn rejects_uninstall_for_system_installer_entries() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));
    store
        .save_apps(&[InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v1.0.0",
            "project-windows-x64.exe",
            temp.path().join("project/project-windows-x64.exe"),
            ghrm_core::asset_matcher::InstallType::WindowsInstaller,
            InstallPathKind::SystemInstaller,
            false,
        )])
        .unwrap();

    let error = ghrm_core::installer::uninstall_repo(&store, "owner/project").unwrap_err();
    assert!(error.to_string().contains("system installer"));
    assert_eq!(store.load().unwrap().apps.len(), 1);
}

#[test]
fn creates_install_plan_without_executing_installer() {
    let repo = RepoRef::parse("owner/project").unwrap();
    let release = Release::fixture(
        "v1.0.0",
        vec![ReleaseAsset::fixture("project-windows-x64.exe")],
    );
    let matched = AssetMatcher::new(OperatingSystem::Windows, Architecture::X64)
        .select_best(&release)
        .unwrap();

    let plan = InstallPlan::from_match(&repo, &release, &matched);

    assert_eq!(plan.repo_id, "owner/project");
    assert_eq!(plan.version, "v1.0.0");
    assert_eq!(plan.asset_name, "project-windows-x64.exe");
    assert!(plan.requires_user_confirmation);
}

#[test]
fn linux_package_install_plan_requires_confirmation() {
    let repo = RepoRef::parse("owner/project").unwrap();
    let release = Release::fixture(
        "v1.0.0",
        vec![ReleaseAsset::fixture("project-linux-amd64.deb")],
    );
    let matched = AssetMatcher::new(OperatingSystem::Linux, Architecture::X64)
        .select_best(&release)
        .unwrap();

    let plan = InstallPlan::from_match(&repo, &release, &matched);

    assert!(plan.requires_user_confirmation);
    assert!(
        plan.notes
            .iter()
            .any(|note| note.contains("Linux .deb/.rpm packages"))
    );
}

#[test]
fn parses_release_note_url_and_publish_time() {
    let release: Release = serde_json::from_str(
        r#"{
          "tag_name": "v1.2.3",
          "name": "Stable release",
          "body": "Fix crash and improve startup.",
          "html_url": "https://github.com/owner/project/releases/tag/v1.2.3",
          "published_at": "2026-07-21T10:20:30Z",
          "prerelease": false,
          "assets": []
        }"#,
    )
    .unwrap();

    assert_eq!(
        release.release_note(),
        Some("Fix crash and improve startup.")
    );
    assert_eq!(
        release.html_url.as_deref(),
        Some("https://github.com/owner/project/releases/tag/v1.2.3")
    );
    assert_eq!(
        release.published_at.unwrap().to_rfc3339(),
        "2026-07-21T10:20:30+00:00"
    );
}
