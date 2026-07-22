use ghrm_core::{
    asset_matcher::{Architecture, AssetMatcher, OperatingSystem},
    install_plan::InstallPlan,
    manifest::{InstalledApp, ManifestStore},
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
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.apps[0].id, "owner/project");
    assert_eq!(manifest.apps[0].installed_version, "v1.0.0");
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
