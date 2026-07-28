use releasedock_core::{
    integrity::{
        IntegrityPlan, IntegrityStatus, IntegrityVerifier, MAX_CHECKSUM_ASSET_SIZE,
        checksum_for_asset, sha256_file, verify_file_sha256,
    },
    release::{Release, ReleaseAsset, ReleaseClient},
};

const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[test]
fn parses_target_checksum_asset_in_supported_formats() {
    let target = "demo-linux-x64.tar.gz";

    assert_eq!(
        checksum_for_asset("DEMO-LINUX-X64.TAR.GZ.SHA256", SHA_A.as_bytes(), target,)
            .unwrap()
            .as_deref(),
        Some(SHA_A)
    );
    assert_eq!(
        checksum_for_asset(
            "demo-linux-x64.tar.gz.sha256",
            format!("{SHA_A} *{target}\n").as_bytes(),
            target,
        )
        .unwrap()
        .as_deref(),
        Some(SHA_A)
    );
}

#[test]
fn parses_supported_checksum_manifests_by_exact_target_basename() {
    for asset_name in ["SHA256SUMS", "sha256sums.TXT", "Checksums.txt"] {
        let contents = format!("{SHA_A}  other.tar.gz\n{SHA_A} *nested/demo-linux-x64.tar.gz\n");

        assert_eq!(
            checksum_for_asset(asset_name, contents.as_bytes(), "demo-linux-x64.tar.gz")
                .unwrap()
                .as_deref(),
            Some(SHA_A),
            "candidate {asset_name}"
        );
    }
}

#[test]
fn ignores_unsupported_or_non_matching_checksum_content() {
    assert_eq!(
        checksum_for_asset("MD5SUMS", SHA_A.as_bytes(), "demo.tar.gz").unwrap(),
        None
    );
    assert_eq!(
        checksum_for_asset(
            "SHA256SUMS",
            format!("{SHA_A} *Demo.tar.gz\n").as_bytes(),
            "demo.tar.gz",
        )
        .unwrap(),
        None
    );
}

#[test]
fn parses_gnu_mode_character_without_consuming_a_literal_star() {
    assert_eq!(
        checksum_for_asset(
            "SHA256SUMS",
            format!("{SHA_A} *demo.tar.gz\n").as_bytes(),
            "demo.tar.gz",
        )
        .unwrap()
        .as_deref(),
        Some(SHA_A)
    );

    assert_eq!(
        checksum_for_asset(
            "SHA256SUMS",
            format!("{SHA_A}  *demo.tar.gz\n").as_bytes(),
            "demo.tar.gz",
        )
        .unwrap(),
        None
    );
    assert_eq!(
        checksum_for_asset(
            "SHA256SUMS",
            format!("{SHA_A}  *demo.tar.gz\n").as_bytes(),
            "*demo.tar.gz",
        )
        .unwrap()
        .as_deref(),
        Some(SHA_A)
    );
}

#[test]
fn rejects_invalid_digest_that_names_the_target() {
    let error = checksum_for_asset("SHA256SUMS", b"not-a-sha256 *demo.tar.gz\n", "demo.tar.gz")
        .unwrap_err();

    assert!(error.to_string().contains("invalid SHA-256"), "{error:#}");
    assert!(error.to_string().contains("demo.tar.gz"), "{error:#}");
}

#[test]
fn target_specific_checksum_fails_closed_without_a_valid_target_record() {
    for contents in [
        format!("{SHA_A} *other.tar.gz\n"),
        format!("{SHA_A} # demo.tar.gz\n"),
        "# checksum generated during release\n".to_string(),
        String::new(),
    ] {
        let error = checksum_for_asset("demo.tar.gz.sha256", contents.as_bytes(), "demo.tar.gz")
            .unwrap_err();

        assert!(error.to_string().contains("invalid SHA-256"), "{error:#}");
        assert!(error.to_string().contains("demo.tar.gz"), "{error:#}");
    }
}

#[test]
fn checksum_candidates_allow_duplicates_but_reject_conflicts() {
    let duplicate = IntegrityVerifier::from_checksum_contents(
        "demo.tar.gz",
        &[
            ("demo.tar.gz.sha256", SHA_A.as_bytes()),
            ("SHA256SUMS", format!("{SHA_A} *demo.tar.gz\n").as_bytes()),
        ],
    )
    .unwrap();
    assert_eq!(duplicate.expected_sha256.as_deref(), Some(SHA_A));
    assert_eq!(
        duplicate.checksum_asset_name.as_deref(),
        Some("demo.tar.gz.sha256")
    );
    assert_eq!(duplicate.status, IntegrityStatus::RecordedOnly);

    let error = IntegrityVerifier::from_checksum_contents(
        "demo.tar.gz",
        &[
            ("demo.tar.gz.sha256", SHA_A.as_bytes()),
            (
                "checksums.txt",
                format!("{SHA_B} *demo.tar.gz\n").as_bytes(),
            ),
        ],
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("conflicting SHA-256"),
        "{error:#}"
    );
}

#[test]
fn no_matching_checksum_returns_recorded_only_plan() {
    let plan = IntegrityVerifier::from_checksum_contents(
        "demo.tar.gz",
        &[("SHA256SUMS", format!("{SHA_A} *other.tar.gz\n").as_bytes())],
    )
    .unwrap();

    assert_eq!(plan, IntegrityPlan::default());
}

#[test]
fn hashes_and_verifies_files_with_clear_mismatch_errors() {
    let temp = tempfile::tempdir().unwrap();
    let artifact = temp.path().join("artifact.bin");
    std::fs::write(&artifact, b"hello world").unwrap();
    let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

    assert_eq!(sha256_file(&artifact).unwrap(), expected);
    assert_eq!(verify_file_sha256(&artifact, expected).unwrap(), expected);

    let error = verify_file_sha256(&artifact, SHA_A).unwrap_err();
    assert!(error.to_string().contains("SHA-256 mismatch"), "{error:#}");
    assert!(error.to_string().contains(SHA_A), "{error:#}");
    assert!(error.to_string().contains(expected), "{error:#}");
}

#[tokio::test]
async fn discovery_skips_oversized_checksum_assets_before_download() {
    let target = ReleaseAsset::fixture("demo.tar.gz");
    let mut checksum = ReleaseAsset::fixture("SHA256SUMS");
    checksum.browser_download_url = "http://127.0.0.1:1/must-not-download".to_string();
    checksum.size = MAX_CHECKSUM_ASSET_SIZE + 1;
    let release = Release::fixture("v1.0.0", vec![target.clone(), checksum]);
    let client = ReleaseClient::new(None, None).unwrap();

    let plan = IntegrityVerifier::discover(&client, &release, &target)
        .await
        .unwrap();

    assert_eq!(plan, IntegrityPlan::default());
}
