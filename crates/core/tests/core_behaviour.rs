use std::sync::{Arc, Barrier};

use releasedock_core::{
    asset_matcher::{Architecture, AssetMatcher, OperatingSystem},
    config::Language,
    install_plan::{InstallManagementKind, InstallPlan, InstallSelectionGuard},
    integrity::{IntegrityPlan, IntegrityStatus},
    manifest::{
        InstallPathKind, InstalledApp, LifecycleAction, LifecycleEvent, Manifest, ManifestStore,
        SystemPackageManager,
    },
    release::{Release, ReleaseAsset},
    release_policy::{ReleaseDirection, ReleasePolicy},
    repo::RepoRef,
};

#[cfg(target_os = "windows")]
use releasedock_core::{
    installer::adopt_system_installer_app_with, windows_install_registry::WindowsInstallDiscovery,
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
fn prefers_windows_portable_archive_over_installer() {
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

    assert_eq!(matched.asset.name, "demo-windows-x64.zip");
    assert!(matched.score > 0);
}

#[test]
fn prefers_windows_bare_executable_over_installer() {
    let release = Release::fixture(
        "v0.2.5",
        vec![
            ReleaseAsset::fixture("ReleaseDock_0.2.5_x64_en-US.msi"),
            ReleaseAsset::fixture("ReleaseDock-windows-x64.exe"),
            ReleaseAsset::fixture("ReleaseDock_0.2.5_x64-setup.exe"),
        ],
    );

    let matched = AssetMatcher::new(OperatingSystem::Windows, Architecture::X64)
        .select_best(&release)
        .unwrap();

    assert_eq!(matched.asset.name, "ReleaseDock-windows-x64.exe");
    assert_eq!(
        matched.install_type,
        releasedock_core::asset_matcher::InstallType::Executable
    );
}

#[test]
fn prefers_windows_setup_exe_over_msi() {
    let release = Release::fixture(
        "v0.2.5",
        vec![
            ReleaseAsset::fixture("ReleaseDock_0.2.5_x64_en-US.msi"),
            ReleaseAsset::fixture("ReleaseDock_0.2.5_x64-setup.exe"),
        ],
    );

    let matched = AssetMatcher::new(OperatingSystem::Windows, Architecture::X64)
        .select_best(&release)
        .unwrap();

    assert_eq!(matched.asset.name, "ReleaseDock_0.2.5_x64-setup.exe");
    assert_eq!(
        matched.install_type,
        releasedock_core::asset_matcher::InstallType::WindowsInstaller
    );
}

#[test]
fn recognizes_linux_executable_assets_without_extensions() {
    let release = Release::fixture(
        "v1.2.3",
        vec![ReleaseAsset::fixture("releasedock-linux-x64")],
    );

    let matched = AssetMatcher::new(OperatingSystem::Linux, Architecture::X64)
        .select_best(&release)
        .unwrap();

    assert_eq!(matched.asset.name, "releasedock-linux-x64");
    assert_eq!(
        matched.install_type,
        releasedock_core::asset_matcher::InstallType::Executable
    );
}

#[test]
fn rejects_linux_assets_without_extension_or_arch_keyword() {
    let release = Release::fixture("v1.2.3", vec![ReleaseAsset::fixture("releasedock-linux")]);

    let result = AssetMatcher::new(OperatingSystem::Linux, Architecture::X64).select_best(&release);

    assert!(result.is_err());
}

#[test]
fn rejects_windows_checksum_files_as_installable_assets() {
    let release = Release::fixture(
        "v1.2.3",
        vec![ReleaseAsset::fixture("checksums-windows.txt")],
    );

    let result =
        AssetMatcher::new(OperatingSystem::Windows, Architecture::X64).select_best(&release);

    assert!(result.is_err());
}

#[test]
fn rejects_linux_auxiliary_assets_without_extensions() {
    let release = Release::fixture("v1.2.3", vec![ReleaseAsset::fixture("checksums-linux-x64")]);

    let result = AssetMatcher::new(OperatingSystem::Linux, Architecture::X64).select_best(&release);

    assert!(result.is_err());
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
fn prefers_linux_archive_over_system_package() {
    let release = Release::fixture(
        "v2.0.0",
        vec![
            ReleaseAsset::fixture("demo-linux-amd64.tar.gz"),
            ReleaseAsset::fixture("demo-linux-amd64.deb"),
        ],
    );

    let matched = AssetMatcher::new(OperatingSystem::Linux, Architecture::X64)
        .select_best(&release)
        .unwrap();

    assert_eq!(matched.asset.name, "demo-linux-amd64.tar.gz");
}

#[test]
fn recognizes_linux_pacman_packages() {
    let release = Release::fixture(
        "v2.0.0",
        vec![ReleaseAsset::fixture("demo-linux-x86_64.pkg.tar.zst")],
    );

    let matched = AssetMatcher::new(OperatingSystem::Linux, Architecture::X64)
        .select_best(&release)
        .unwrap();

    assert_eq!(matched.asset.name, "demo-linux-x86_64.pkg.tar.zst");
    assert_eq!(
        matched.install_type,
        releasedock_core::asset_matcher::InstallType::LinuxPackage
    );
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
    assert_eq!(manifest.schema_version, 4);
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
    assert_eq!(manifest.schema_version, 4);
    assert!(manifest.apps[0].uninstall_supported);
    assert_eq!(
        manifest.apps[0].install_type,
        releasedock_core::asset_matcher::InstallType::Executable
    );
    assert_eq!(
        manifest.apps[0].install_path_kind,
        releasedock_core::manifest::InstallPathKind::ManagedPath
    );
}

#[test]
fn upgrades_legacy_windows_setup_manifest_entries_to_system_installers() {
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
              "asset_name": "project-windows-x64-setup.exe",
              "install_path": "/tmp/project/project-windows-x64-setup.exe"
            }
          ]
        }"#,
    )
    .unwrap();

    let manifest = store.load().unwrap();
    assert_eq!(
        manifest.apps[0].install_type,
        releasedock_core::asset_matcher::InstallType::WindowsInstaller
    );
    assert_eq!(
        manifest.apps[0].install_path_kind,
        releasedock_core::manifest::InstallPathKind::SystemInstaller
    );
    assert!(!manifest.apps[0].uninstall_supported);
}

#[test]
fn upgrades_legacy_linux_executable_manifest_entries() {
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
              "asset_name": "releasedock-linux-x64",
              "install_path": "/tmp/project/releasedock-linux-x64"
            }
          ]
        }"#,
    )
    .unwrap();

    let manifest = store.load().unwrap();
    assert_eq!(manifest.schema_version, 4);
    assert_eq!(
        manifest.apps[0].install_type,
        releasedock_core::asset_matcher::InstallType::Executable
    );
    assert_eq!(manifest.apps[0].launch_path.as_deref(), None);
    assert!(manifest.apps[0].uninstall_supported);
}

#[test]
fn records_installer_path_for_system_installer_apps() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));

    store
        .save_apps(&[InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v1.0.0",
            "project-windows-x64.exe",
            temp.path().join("Program Files/ReleaseDock"),
            releasedock_core::asset_matcher::InstallType::WindowsInstaller,
            InstallPathKind::SystemInstaller,
            false,
        )
        .with_installer_path(Some(
            temp.path()
                .join("Downloads/ReleaseDock_0.2.5_x64_en-US.msi"),
        ))])
        .unwrap();

    let manifest = store.load().unwrap();
    assert_eq!(
        manifest.apps[0]
            .installer_path
            .as_ref()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy(),
        "ReleaseDock_0.2.5_x64_en-US.msi"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn adopts_system_installer_apps_with_a_discovery_result() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));
    let repo = RepoRef::parse("owner/project").unwrap();

    store
        .save_apps(&[InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v1.0.0",
            "project-windows-x64.exe",
            temp.path()
                .join("Downloads/ReleaseDock_0.2.5_x64_en-US.msi"),
            releasedock_core::asset_matcher::InstallType::WindowsInstaller,
            InstallPathKind::SystemInstaller,
            false,
        )])
        .unwrap();

    let adopted = adopt_system_installer_app_with(&store, &repo, |_names, _versions| {
        let install_path = temp.path().join("Program Files/ReleaseDock");
        let launch_path = install_path.join("ReleaseDock.exe");
        Ok(Some(WindowsInstallDiscovery {
            install_path,
            launch_path: Some(launch_path),
        }))
    })
    .unwrap();

    let install_path = temp.path().join("Program Files/ReleaseDock");
    let launch_path = install_path.join("ReleaseDock.exe");
    let installer_path = temp
        .path()
        .join("Downloads/ReleaseDock_0.2.5_x64_en-US.msi");

    assert_eq!(adopted.install_path, install_path);
    assert_eq!(adopted.launch_path.as_deref(), Some(launch_path.as_path()));
    assert_eq!(
        adopted.installer_path.as_deref(),
        Some(installer_path.as_path())
    );

    let manifest = store.load().unwrap();
    assert_eq!(manifest.apps[0].install_path, adopted.install_path);
    assert_eq!(manifest.apps[0].launch_path, adopted.launch_path);
    assert_eq!(manifest.apps[0].installer_path, adopted.installer_path);
}

#[cfg(not(target_os = "windows"))]
#[test]
fn adopt_rejects_non_system_installer_entries_before_platform_check() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));
    let repo = RepoRef::parse("owner/project").unwrap();

    store
        .save_apps(&[InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v1.0.0",
            "project-linux-x86_64.tar.gz",
            temp.path().join("project"),
            releasedock_core::asset_matcher::InstallType::Archive,
            InstallPathKind::ManagedPath,
            true,
        )])
        .unwrap();

    let error = releasedock_core::installer::adopt_system_installer_app(&store, &repo).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("only system installer entries can be adopted"),
        "{error:#}"
    );
}

#[cfg(not(target_os = "windows"))]
#[test]
fn adopt_reports_windows_only_after_confirming_system_installer_entry() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));
    let repo = RepoRef::parse("owner/project").unwrap();

    store
        .save_apps(&[InstalledApp::with_install_metadata(
            "owner/project",
            "project",
            "v1.0.0",
            "project-windows-x64.exe",
            temp.path().join("Downloads/project-windows-x64.exe"),
            releasedock_core::asset_matcher::InstallType::WindowsInstaller,
            InstallPathKind::SystemInstaller,
            false,
        )])
        .unwrap();

    let error = releasedock_core::installer::adopt_system_installer_app(&store, &repo).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("system installer adoption is only available on Windows"),
        "{error:#}"
    );
}

#[test]
fn appends_and_reads_recent_lifecycle_events() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));
    let event = LifecycleEvent::succeeded(
        "owner/project",
        "project",
        LifecycleAction::Install,
        "Installed project v1.0.0",
        Some("v1.0.0".to_string()),
        Some("project-linux-x86_64.tar.gz".to_string()),
        Some(temp.path().join("project")),
        Some(InstallPathKind::ManagedPath),
    );

    store.append_lifecycle_event(event.clone()).unwrap();

    let manifest = store.load().unwrap();
    assert_eq!(manifest.schema_version, 4);
    assert_eq!(
        manifest.latest_lifecycle_event("owner/project"),
        Some(&event)
    );
}

#[test]
fn concurrent_manifest_writers_preserve_all_events() {
    let temp = tempfile::tempdir().unwrap();
    let manifest_path = temp.path().join("apps.json");
    let writers = 12;
    let barrier = Arc::new(Barrier::new(writers));
    let handles = (0..writers)
        .map(|index| {
            let barrier = barrier.clone();
            let manifest_path = manifest_path.clone();
            std::thread::spawn(move || {
                let store = ManifestStore::at_path(manifest_path);
                barrier.wait();
                store.append_lifecycle_event(LifecycleEvent::succeeded(
                    format!("owner/project-{index}"),
                    format!("project-{index}"),
                    LifecycleAction::Install,
                    "installed",
                    Some("v1.0.0".to_string()),
                    None,
                    None,
                    None,
                ))
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.join().unwrap().unwrap();
    }

    let stored = ManifestStore::at_path(manifest_path).load().unwrap();
    assert_eq!(stored.lifecycle_events.len(), writers);
}

#[test]
fn returns_recent_lifecycle_events_in_reverse_chronological_order() {
    let mut manifest = Manifest::empty();
    for index in 0..6 {
        manifest.append_lifecycle_event(LifecycleEvent::succeeded(
            "owner/project",
            "project",
            LifecycleAction::Update,
            format!("update {index}"),
            Some(format!("v1.0.{index}")),
            Some(format!("project-{index}.tar.gz")),
            None,
            None,
        ));
    }

    let recent = manifest.recent_lifecycle_events("owner/project", 3);
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].summary, "update 5");
    assert_eq!(recent[1].summary, "update 4");
    assert_eq!(recent[2].summary, "update 3");
}

#[test]
fn save_apps_preserves_existing_lifecycle_events() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));
    store
        .save(&Manifest {
            schema_version: 4,
            apps: vec![],
            lifecycle_events: vec![LifecycleEvent::succeeded(
                "owner/project",
                "project",
                LifecycleAction::Install,
                "Installed project v1.0.0",
                Some("v1.0.0".to_string()),
                Some("project-linux-x86_64.tar.gz".to_string()),
                None,
                None,
            )],
        })
        .unwrap();

    store
        .save_apps(&[InstalledApp::new(
            "owner/project",
            "project",
            "v1.1.0",
            "project-linux-x86_64.tar.gz",
            temp.path().join("project"),
        )])
        .unwrap();

    let manifest = store.load().unwrap();
    assert_eq!(manifest.apps.len(), 1);
    assert_eq!(manifest.apps[0].installed_version, "v1.1.0");
    assert_eq!(manifest.lifecycle_events.len(), 1);
    assert_eq!(manifest.lifecycle_events[0].repo_id, "owner/project");
}

#[test]
fn keeps_only_recent_lifecycle_events_per_repo() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));
    let mut manifest = Manifest::empty();

    for index in 0..6 {
        manifest.append_lifecycle_event(LifecycleEvent::succeeded(
            "owner/project",
            "project",
            LifecycleAction::Update,
            format!("update {index}"),
            Some(format!("v1.0.{index}")),
            Some(format!("project-{index}.tar.gz")),
            None,
            None,
        ));
    }

    store.save(&manifest).unwrap();
    let manifest = store.load().unwrap();

    assert_eq!(manifest.lifecycle_events.len(), 5);
    assert_eq!(
        manifest.lifecycle_events.first().unwrap().summary,
        "update 1"
    );
    assert_eq!(
        manifest.lifecycle_events.last().unwrap().summary,
        "update 5"
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
            releasedock_core::asset_matcher::InstallType::WindowsInstaller,
            InstallPathKind::SystemInstaller,
            false,
        )])
        .unwrap();

    let error =
        releasedock_core::installer::uninstall_repo(&store, "owner/project", Language::En, None)
            .unwrap_err();
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

    let plan = InstallPlan::from_match(&repo, &release, &matched, Language::En);

    assert_eq!(plan.repo_id, "owner/project");
    assert_eq!(plan.version, "v1.0.0");
    assert_eq!(plan.asset_name, "project-windows-x64.exe");
    assert!(!plan.requires_user_confirmation);
    assert_eq!(plan.management_kind, InstallManagementKind::ManagedLocal);
    assert_eq!(
        plan.install_type,
        releasedock_core::asset_matcher::InstallType::Executable
    );
    assert_eq!(plan.integrity, IntegrityPlan::default());
    assert_eq!(plan.integrity.status, IntegrityStatus::RecordedOnly);
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

    let plan = InstallPlan::from_match(&repo, &release, &matched, Language::En);

    assert!(plan.requires_user_confirmation);
    assert!(
        plan.notes
            .iter()
            .any(|note| note.contains("Linux system packages"))
    );
    assert_eq!(plan.management_kind, InstallManagementKind::SystemPackage);
    assert_eq!(
        plan.system_package_manager,
        Some(SystemPackageManager::Debian)
    );
}

#[test]
fn install_plan_can_attach_discovered_integrity_without_changing_callers() {
    let repo = RepoRef::parse("owner/project").unwrap();
    let release = Release::fixture(
        "v1.0.0",
        vec![ReleaseAsset::fixture("project-linux-amd64.tar.gz")],
    );
    let matched = AssetMatcher::new(OperatingSystem::Linux, Architecture::X64)
        .select_best(&release)
        .unwrap();
    let integrity = IntegrityPlan {
        expected_sha256: Some("a".repeat(64)),
        checksum_asset_name: Some("SHA256SUMS".to_string()),
        status: IntegrityStatus::RecordedOnly,
    };

    let plan = InstallPlan::from_match(&repo, &release, &matched, Language::En)
        .with_integrity(integrity.clone());

    assert_eq!(plan.integrity, integrity);
}

#[test]
fn install_plan_defaults_release_direction_and_supports_builder() {
    let repo = RepoRef::parse("owner/project").unwrap();
    let release = Release::fixture(
        "v2.0.0",
        vec![ReleaseAsset::fixture("project-linux-x64.AppImage")],
    );
    let matched = AssetMatcher::new(OperatingSystem::Linux, Architecture::X64)
        .select_best(&release)
        .unwrap();
    let plan = InstallPlan::from_match(&repo, &release, &matched, Language::En);

    assert_eq!(plan.release_direction, ReleaseDirection::Unknown);
    assert_eq!(
        plan.with_release_direction(ReleaseDirection::Downgrade)
            .release_direction,
        ReleaseDirection::Downgrade
    );

    let mut serialized = serde_json::to_value(InstallPlan::from_match(
        &repo,
        &release,
        &matched,
        Language::En,
    ))
    .unwrap();
    serialized
        .as_object_mut()
        .unwrap()
        .remove("release_direction");
    serialized
        .as_object_mut()
        .unwrap()
        .remove("selection_guard");
    serialized.as_object_mut().unwrap().remove("target_policy");
    let restored: InstallPlan = serde_json::from_value(serialized).unwrap();
    assert_eq!(restored.release_direction, ReleaseDirection::Unknown);
    assert_eq!(restored.selection_guard, None);
    assert_eq!(restored.target_policy, None);
}

#[test]
fn install_selection_guard_uses_an_explicit_camel_case_state() {
    let absent = serde_json::to_value(InstallSelectionGuard::ExpectedAbsent).unwrap();
    assert_eq!(absent, serde_json::json!({ "state": "expectedAbsent" }));

    let installed = serde_json::to_value(InstallSelectionGuard::ExpectedInstalled {
        installed_version: "v1.0.0".to_string(),
        release_policy: ReleasePolicy::default(),
    })
    .unwrap();
    assert_eq!(installed["state"], "expectedInstalled");
    assert_eq!(installed["installedVersion"], "v1.0.0");
    assert_eq!(installed["releasePolicy"]["channel"], "stable");
}

#[test]
fn linux_pacman_install_plan_records_package_manager() {
    let repo = RepoRef::parse("owner/project").unwrap();
    let release = Release::fixture(
        "v1.0.0",
        vec![ReleaseAsset::fixture("project-linux-amd64.pkg.tar.zst")],
    );
    let matched = AssetMatcher::new(OperatingSystem::Linux, Architecture::X64)
        .select_best(&release)
        .unwrap();

    let plan = InstallPlan::from_match(&repo, &release, &matched, Language::En);

    assert_eq!(plan.management_kind, InstallManagementKind::SystemPackage);
    assert_eq!(
        plan.system_package_manager,
        Some(SystemPackageManager::Pacman)
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
