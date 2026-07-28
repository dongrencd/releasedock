use std::sync::{Arc, Barrier};

use releasedock_core::{
    manifest::{InstalledApp, LifecycleAction, ManifestStore},
    release_policy::{PolicyMutation, ReleaseChannel},
};

fn installed_app() -> InstalledApp {
    InstalledApp::new(
        "owner/project",
        "project",
        "v1.2.3",
        "project.AppImage",
        "/tmp/project.AppImage".into(),
    )
}

#[test]
fn policy_mutation_is_atomic_and_records_only_effective_changes() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));
    store.save_apps(&[installed_app()]).unwrap();

    let changed = store
        .mutate_release_policy("owner/project", PolicyMutation::PinCurrent)
        .unwrap();
    let unchanged = store
        .mutate_release_policy("owner/project", PolicyMutation::PinCurrent)
        .unwrap();

    assert!(changed.changed);
    assert_eq!(changed.policy.pinned_version.as_deref(), Some("v1.2.3"));
    assert!(!unchanged.changed);

    let manifest = store.load().unwrap();
    assert_eq!(manifest.apps[0].release_policy, changed.policy);
    assert_eq!(manifest.lifecycle_events.len(), 1);
    assert_eq!(
        manifest.lifecycle_events[0].action,
        LifecycleAction::PolicyChange
    );
}

#[test]
fn ignored_versions_are_deduplicated_and_can_be_removed() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));
    let mut app = installed_app();
    app.release_policy.ignored_versions = vec!["v2.0.0".into(), "v2.0.0".into()];
    store.save_apps(&[app]).unwrap();

    let normalized = store
        .mutate_release_policy(
            "owner/project",
            PolicyMutation::IgnoreVersion("v2.0.0".into()),
        )
        .unwrap();
    assert!(normalized.changed);
    assert_eq!(normalized.policy.ignored_versions, ["v2.0.0"]);

    let removed = store
        .mutate_release_policy(
            "owner/project",
            PolicyMutation::UnignoreVersion("v2.0.0".into()),
        )
        .unwrap();
    assert!(removed.changed);
    assert!(removed.policy.ignored_versions.is_empty());

    let no_op = store
        .mutate_release_policy(
            "owner/project",
            PolicyMutation::UnignoreVersion("v2.0.0".into()),
        )
        .unwrap();
    assert!(!no_op.changed);
    assert_eq!(store.load().unwrap().lifecycle_events.len(), 2);
}

#[test]
fn concurrent_policy_mutations_preserve_every_ignored_version() {
    let temp = tempfile::tempdir().unwrap();
    let manifest_path = temp.path().join("apps.json");
    ManifestStore::at_path(manifest_path.clone())
        .save_apps(&[installed_app()])
        .unwrap();

    let writers = 8;
    let barrier = Arc::new(Barrier::new(writers));
    let handles = (0..writers)
        .map(|index| {
            let barrier = Arc::clone(&barrier);
            let manifest_path = manifest_path.clone();
            std::thread::spawn(move || {
                barrier.wait();
                ManifestStore::at_path(manifest_path)
                    .mutate_release_policy(
                        "owner/project",
                        PolicyMutation::IgnoreVersion(format!("v{index}.0.0")),
                    )
                    .unwrap();
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.join().unwrap();
    }

    let manifest = ManifestStore::at_path(manifest_path).load().unwrap();
    let mut ignored = manifest.apps[0].release_policy.ignored_versions.clone();
    ignored.sort();
    assert_eq!(ignored.len(), writers);
    assert_eq!(
        manifest
            .lifecycle_events
            .iter()
            .filter(|event| event.action == LifecycleAction::PolicyChange)
            .count(),
        writers.min(5)
    );
}

#[test]
fn policy_mutation_rejects_unmanaged_repository() {
    let temp = tempfile::tempdir().unwrap();
    let store = ManifestStore::at_path(temp.path().join("apps.json"));

    let error = store
        .mutate_release_policy(
            "owner/missing",
            PolicyMutation::SetChannel(ReleaseChannel::Prerelease),
        )
        .unwrap_err();

    assert!(error.to_string().contains("owner/missing"));
    assert!(error.to_string().contains("not installed"));
}
