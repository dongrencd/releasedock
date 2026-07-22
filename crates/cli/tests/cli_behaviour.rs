use assert_cmd::Command;
use predicates::str::contains;

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
