use std::{
    fs,
    io::{ErrorKind, Read, Write},
    net::TcpListener,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use assert_cmd::Command;
use releasedock_core::{
    integrity::sha256_file,
    manifest::{InstalledApp, ManifestStore},
    release_policy::{PolicyMutation, ReleaseChannel},
};
use serde_json::{Value, json};

const TEST_API_BASE_URL: &str = "RELEASEDOCK_TEST_GITHUB_API_BASE_URL";

struct Response {
    target: String,
    headers: Vec<(String, String)>,
    body: String,
    before_write: Option<Box<dyn FnOnce() + Send>>,
}

fn start_server(
    build_responses: impl FnOnce(&str) -> Vec<Response>,
) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let responses = build_responses(&base_url);

    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        for mut response in responses {
            let deadline = Instant::now() + Duration::from_secs(10);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "timed out waiting for CLI request"
                        );
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("failed to accept CLI request: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut received = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let read = stream.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                received.extend_from_slice(&buffer[..read]);
                if received.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8(received).unwrap();
            let target = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap()
                .to_string();
            assert_eq!(target, response.target);
            requests.push(target);

            if let Some(before_write) = response.before_write.take() {
                before_write();
            }

            let extra_headers = response
                .headers
                .iter()
                .map(|(name, value)| format!("{name}: {value}\r\n"))
                .collect::<String>();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n{}",
                response.body.len(),
                response.body
            )
            .unwrap();
        }
        requests
    });

    (base_url, server)
}

fn command(config: &Path, base_url: &str) -> Command {
    let mut command = Command::cargo_bin("releasedock").unwrap();
    command
        .env("GHRM_CONFIG_PATH", config)
        .env(TEST_API_BASE_URL, base_url)
        .env_remove("GITHUB_TOKEN")
        .env_remove("HTTPS_PROXY")
        .env_remove("https_proxy")
        .env_remove("HTTP_PROXY")
        .env_remove("http_proxy")
        .env_remove("ALL_PROXY")
        .env_remove("all_proxy");
    command
}

fn write_config(temp: &tempfile::TempDir) -> std::path::PathBuf {
    let path = temp.path().join("config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&json!({
            "installRoot": temp.path().join("install-root")
        }))
        .unwrap(),
    )
    .unwrap();
    path
}

fn release_body(base_url: &str, checksum: bool) -> String {
    let mut assets = vec![json!({
        "name": "demo-linux-x86_64.AppImage",
        "browser_download_url": format!("{base_url}/downloads/demo.AppImage"),
        "size": 19
    })];
    if checksum {
        assets.push(json!({
            "name": "SHA256SUMS",
            "browser_download_url": format!("{base_url}/downloads/SHA256SUMS"),
            "size": 100
        }));
    }
    serde_json::to_string(&json!({
        "tag_name": "v2.0.0",
        "draft": false,
        "prerelease": false,
        "assets": assets
    }))
    .unwrap()
}

#[test]
fn releases_all_follows_two_live_link_pages() {
    let temp = tempfile::tempdir().unwrap();
    let config = write_config(&temp);
    let (base_url, server) = start_server(|base_url| {
        vec![
            Response {
                target: "/repos/owner/project/releases?page=1&per_page=100".into(),
                headers: vec![(
                    "Link".into(),
                    format!(
                        "<{base_url}/repos/owner/project/releases?page=2&per_page=100>; rel=\"next\""
                    ),
                )],
                body: serde_json::to_string(&vec![json!({
                    "tag_name": "v2.0.0",
                    "draft": false,
                    "prerelease": false,
                    "assets": []
                })])
                .unwrap(),
                before_write: None,
            },
            Response {
                target: "/repos/owner/project/releases?page=2&per_page=100".into(),
                headers: Vec::new(),
                body: serde_json::to_string(&vec![json!({
                    "tag_name": "v1.0.0",
                    "draft": false,
                    "prerelease": false,
                    "assets": []
                })])
                .unwrap(),
                before_write: None,
            },
        ]
    });

    let output = command(&config, &base_url)
        .args(["releases", "owner/project", "--all", "--json"])
        .output()
        .unwrap();
    let requests = server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let releases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        releases
            .iter()
            .map(|release| release["tag_name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["v2.0.0", "v1.0.0"]
    );
    assert_eq!(requests.len(), 2);
}

#[test]
fn releases_all_stops_when_a_linked_page_is_empty() {
    let temp = tempfile::tempdir().unwrap();
    let config = write_config(&temp);
    let (base_url, server) = start_server(|base_url| {
        vec![Response {
            target: "/repos/owner/project/releases?page=1&per_page=100".into(),
            headers: vec![(
                "Link".into(),
                format!(
                    "<{base_url}/repos/owner/project/releases?page=2&per_page=100>; rel=\"next\""
                ),
            )],
            body: "[]".into(),
            before_write: None,
        }]
    });

    let output = command(&config, &base_url)
        .args(["releases", "owner/project", "--all", "--json"])
        .output()
        .unwrap();
    let requests = server.join().unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!([])
    );
    assert_eq!(requests.len(), 1);
}

#[test]
fn releases_all_rejects_an_unbounded_next_link_chain() {
    let temp = tempfile::tempdir().unwrap();
    let config = write_config(&temp);
    let (base_url, server) = start_server(|base_url| {
        (1..=20)
            .map(|page| Response {
                target: format!(
                    "/repos/owner/project/releases?page={page}&per_page=100"
                ),
                headers: vec![(
                    "Link".into(),
                    format!(
                        "<{base_url}/repos/owner/project/releases?page={}&per_page=100>; rel=\"next\"",
                        page + 1
                    ),
                )],
                body: serde_json::to_string(&vec![json!({
                    "tag_name": format!("v{page}.0.0"),
                    "draft": false,
                    "prerelease": false,
                    "assets": []
                })])
                .unwrap(),
                before_write: None,
            })
            .collect()
    });

    let output = command(&config, &base_url)
        .args(["releases", "owner/project", "--all", "--json"])
        .output()
        .unwrap();
    let requests = server.join().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("maximum 20 pages"));
    assert_eq!(requests.len(), 20);
}

#[test]
fn releases_all_rejects_a_repeated_page_tag_set() {
    let temp = tempfile::tempdir().unwrap();
    let config = write_config(&temp);
    let repeated = serde_json::to_string(&vec![json!({
        "tag_name": "v2.0.0",
        "draft": false,
        "prerelease": false,
        "assets": []
    })])
    .unwrap();
    let (base_url, server) = start_server(|base_url| {
        vec![
            Response {
                target: "/repos/owner/project/releases?page=1&per_page=100".into(),
                headers: vec![(
                    "Link".into(),
                    format!(
                        "<{base_url}/repos/owner/project/releases?page=2&per_page=100>; rel=\"next\""
                    ),
                )],
                body: repeated.clone(),
                before_write: None,
            },
            Response {
                target: "/repos/owner/project/releases?page=2&per_page=100".into(),
                headers: Vec::new(),
                body: repeated,
                before_write: None,
            },
        ]
    });

    let output = command(&config, &base_url)
        .args(["releases", "owner/project", "--all", "--json"])
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("repeated release page"));
}

#[test]
fn live_install_json_discovers_checksum_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let config = write_config(&temp);
    let digest = "a".repeat(64);
    let checksum_body = format!("{digest}  demo-linux-x86_64.AppImage\n");
    let (base_url, server) = start_server(|base_url| {
        vec![
            Response {
                target: "/repos/owner/project/releases/latest".into(),
                headers: Vec::new(),
                body: release_body(base_url, true),
                before_write: None,
            },
            Response {
                target: "/downloads/SHA256SUMS".into(),
                headers: Vec::new(),
                body: checksum_body,
                before_write: None,
            },
        ]
    });

    let output = command(&config, &base_url)
        .args([
            "install",
            "owner/project",
            "--os",
            "linux",
            "--arch",
            "x64",
            "--json",
        ])
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["integrity"]["expectedSha256"], digest);
    assert_eq!(plan["integrity"]["checksumAssetName"], "SHA256SUMS");
    assert_eq!(plan["requires_user_confirmation"], false);
}

#[test]
fn live_install_json_warns_when_checksum_metadata_is_missing() {
    let temp = tempfile::tempdir().unwrap();
    let config = write_config(&temp);
    let (base_url, server) = start_server(|base_url| {
        vec![Response {
            target: "/repos/owner/project/releases/latest".into(),
            headers: Vec::new(),
            body: release_body(base_url, false),
            before_write: None,
        }]
    });

    let output = command(&config, &base_url)
        .args([
            "install",
            "owner/project",
            "--os",
            "linux",
            "--arch",
            "x64",
            "--json",
        ])
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["integrity"]["expectedSha256"], Value::Null);
    assert_eq!(plan["requires_user_confirmation"], true);
    assert!(
        plan["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|note| note.as_str().unwrap().contains("No upstream SHA-256"))
    );
}

#[test]
fn live_install_success_reports_verified_checksum_source() {
    let temp = tempfile::tempdir().unwrap();
    let config = write_config(&temp);
    let manifest = temp.path().join("apps.json");
    let artifact = temp.path().join("demo-linux-x86_64.AppImage");
    fs::write(&artifact, b"local test artifact").unwrap();
    let digest = sha256_file(&artifact).unwrap();
    let digest_prefix = digest[..12].to_string();
    let checksum_body = format!("{digest}  demo-linux-x86_64.AppImage\n");
    let (base_url, server) = start_server(|base_url| {
        vec![
            Response {
                target: "/repos/owner/project/releases/latest".into(),
                headers: Vec::new(),
                body: release_body(base_url, true),
                before_write: None,
            },
            Response {
                target: "/downloads/SHA256SUMS".into(),
                headers: Vec::new(),
                body: checksum_body,
                before_write: None,
            },
        ]
    });

    let output = command(&config, &base_url)
        .args([
            "install",
            "owner/project",
            "--artifact-fixture",
            artifact.to_str().unwrap(),
            "--manifest",
            manifest.to_str().unwrap(),
            "--yes",
        ])
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("SHA-256: pending verification from SHA256SUMS"));
    assert!(stdout.contains(&format!(
        "integrity=verifiedChecksum sha256={digest_prefix} source=SHA256SUMS"
    )));
}

#[test]
fn live_update_rejects_policy_change_during_release_selection() {
    let temp = tempfile::tempdir().unwrap();
    let config = write_config(&temp);
    let manifest = temp.path().join("apps.json");
    let artifact = temp.path().join("demo-linux-x86_64.AppImage");
    fs::write(&artifact, b"local test artifact").unwrap();
    ManifestStore::at_path(manifest.clone())
        .save_apps(&[InstalledApp::new(
            "owner/project",
            "project",
            "v1.0.0",
            "demo-linux-x86_64.AppImage",
            temp.path().join("installed.AppImage"),
        )])
        .unwrap();

    let digest = sha256_file(&artifact).unwrap();
    let checksum_body = format!("{digest}  demo-linux-x86_64.AppImage\n");
    let manifest_for_server = manifest.clone();
    let (base_url, server) = start_server(|base_url| {
        vec![
            Response {
                target: "/repos/owner/project/releases?page=1&per_page=100".into(),
                headers: Vec::new(),
                body: format!("[{}]", release_body(base_url, true)),
                before_write: None,
            },
            Response {
                target: "/downloads/SHA256SUMS".into(),
                headers: Vec::new(),
                body: checksum_body,
                before_write: Some(Box::new(move || {
                    ManifestStore::at_path(manifest_for_server)
                        .mutate_release_policy(
                            "owner/project",
                            PolicyMutation::SetChannel(ReleaseChannel::Prerelease),
                        )
                        .unwrap();
                })),
            },
        ]
    });

    let output = command(&config, &base_url)
        .args([
            "update",
            "owner/project",
            "--artifact-fixture",
            artifact.to_str().unwrap(),
            "--manifest",
            manifest.to_str().unwrap(),
            "--yes",
        ])
        .output()
        .unwrap();
    server.join().unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("stale install plan"));
    let stored = ManifestStore::at_path(manifest).load().unwrap();
    assert_eq!(stored.apps[0].installed_version, "v1.0.0");
    assert_eq!(
        stored.apps[0].release_policy.channel,
        ReleaseChannel::Prerelease
    );
}
