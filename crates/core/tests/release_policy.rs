use releasedock_core::{
    release::Release,
    release_policy::{
        ReleaseChannel, ReleaseDirection, ReleasePolicy, ReleaseSelectionError, ReleaseSelector,
    },
};

fn release(tag: &str, prerelease: bool, draft: bool) -> Release {
    let mut release = Release::fixture(tag, Vec::new());
    release.prerelease = prerelease;
    release.draft = draft;
    release
}

#[test]
fn policy_deserializes_camel_case_fields_and_defaults() {
    let default_policy: ReleasePolicy = serde_json::from_str("{}").unwrap();
    assert_eq!(default_policy.channel, ReleaseChannel::Stable);
    assert_eq!(default_policy.pinned_version, None);
    assert!(default_policy.ignored_versions.is_empty());

    let policy: ReleasePolicy = serde_json::from_str(
        r#"{
          "channel": "prerelease",
          "pinnedVersion": "v2.0.0-beta.1",
          "ignoredVersions": ["v1.9.0"]
        }"#,
    )
    .unwrap();
    assert_eq!(policy.channel, ReleaseChannel::Prerelease);
    assert_eq!(policy.pinned_version.as_deref(), Some("v2.0.0-beta.1"));
    assert_eq!(policy.ignored_versions, ["v1.9.0"]);
}

#[test]
fn stable_channel_filters_drafts_prereleases_and_ignored_versions() {
    let releases = vec![
        release("v4.0.0", false, true),
        release("v3.0.0-beta.1", true, false),
        release("v2.0.0", false, false),
        release("v1.0.0", false, false),
    ];
    let policy = ReleasePolicy {
        ignored_versions: vec!["v2.0.0".to_string()],
        ..ReleasePolicy::default()
    };

    let selection = ReleaseSelector::select(&releases, &policy, Some("v2.0.0"), None).unwrap();

    assert_eq!(selection.release.tag_name, "v1.0.0");
    assert_eq!(selection.direction, ReleaseDirection::Downgrade);
}

#[test]
fn prerelease_channel_keeps_github_order_for_stable_and_prerelease_versions() {
    let releases = vec![
        release("v3.0.0-beta.1", true, false),
        release("v2.0.0", false, false),
    ];
    let policy = ReleasePolicy {
        channel: ReleaseChannel::Prerelease,
        ..ReleasePolicy::default()
    };

    let selection = ReleaseSelector::select(&releases, &policy, Some("v2.0.0"), None).unwrap();

    assert_eq!(selection.release.tag_name, "v3.0.0-beta.1");
    assert_eq!(selection.direction, ReleaseDirection::Upgrade);
}

#[test]
fn pinned_version_takes_precedence_over_ignored_versions() {
    let releases = vec![
        release("v3.0.0", false, false),
        release("v2.0.0", false, false),
    ];
    let policy = ReleasePolicy {
        pinned_version: Some("v2.0.0".to_string()),
        ignored_versions: vec!["v2.0.0".to_string()],
        ..ReleasePolicy::default()
    };

    let selection = ReleaseSelector::select(&releases, &policy, Some("v3.0.0"), None).unwrap();

    assert_eq!(selection.release.tag_name, "v2.0.0");
    assert_eq!(selection.direction, ReleaseDirection::Downgrade);
}

#[test]
fn missing_pinned_version_returns_specific_error() {
    let releases = vec![release("v1.0.0", false, false)];
    let policy = ReleasePolicy {
        pinned_version: Some("v2.0.0".to_string()),
        ..ReleasePolicy::default()
    };

    let error = ReleaseSelector::select(&releases, &policy, None, None).unwrap_err();

    assert_eq!(
        error,
        ReleaseSelectionError::PinnedVersionNotFound("v2.0.0".to_string())
    );
    assert!(error.to_string().contains("v2.0.0"));
}

#[test]
fn manual_tag_only_overrides_the_current_selection() {
    let releases = vec![
        release("v3.0.0-beta.1", true, false),
        release("v2.0.0", false, false),
        release("v1.0.0", false, false),
    ];
    let policy = ReleasePolicy {
        pinned_version: Some("v2.0.0".to_string()),
        ignored_versions: vec!["v3.0.0-beta.1".to_string()],
        ..ReleasePolicy::default()
    };

    let manual =
        ReleaseSelector::select(&releases, &policy, Some("v1.0.0"), Some("v3.0.0-beta.1")).unwrap();
    let automatic = ReleaseSelector::select(&releases, &policy, Some("v1.0.0"), None).unwrap();

    assert_eq!(manual.release.tag_name, "v3.0.0-beta.1");
    assert_eq!(automatic.release.tag_name, "v2.0.0");
    assert_eq!(policy.pinned_version.as_deref(), Some("v2.0.0"));
}

#[test]
fn direction_uses_release_order_instead_of_comparing_version_strings() {
    let releases = vec![
        release("release-z", false, false),
        release("release-a", false, false),
        release("release-m", false, false),
    ];
    let policy = ReleasePolicy::default();

    let upgrade =
        ReleaseSelector::select(&releases, &policy, Some("release-m"), Some("release-z")).unwrap();
    let reinstall =
        ReleaseSelector::select(&releases, &policy, Some("release-a"), Some("release-a")).unwrap();
    let unknown =
        ReleaseSelector::select(&releases, &policy, Some("missing"), Some("release-a")).unwrap();

    assert_eq!(upgrade.direction, ReleaseDirection::Upgrade);
    assert_eq!(reinstall.direction, ReleaseDirection::Reinstall);
    assert_eq!(unknown.direction, ReleaseDirection::Unknown);
}

#[test]
fn manual_tag_cannot_select_a_draft() {
    let releases = vec![release("v2.0.0", false, true)];

    let error = ReleaseSelector::select(&releases, &ReleasePolicy::default(), None, Some("v2.0.0"))
        .unwrap_err();

    assert_eq!(
        error,
        ReleaseSelectionError::ManualVersionNotFound("v2.0.0".to_string())
    );
}
