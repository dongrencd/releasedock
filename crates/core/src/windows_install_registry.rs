use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsInstallDiscovery {
    pub install_path: PathBuf,
    pub launch_path: Option<PathBuf>,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn normalize_version(value: &str) -> String {
    normalize(value.trim_start_matches('v').trim())
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn expand_env_vars(raw: &str) -> String {
    let expanded = std::env::vars().fold(raw.to_string(), |acc, (key, value)| {
        acc.replace(&format!("%{key}%"), &value)
    });
    expanded
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn expand_and_trim(raw: &str) -> String {
    let expanded = expand_env_vars(raw);
    expanded.trim().trim_matches('"').to_string()
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn parse_command_path(raw: &str) -> Option<PathBuf> {
    let trimmed = expand_env_vars(raw);
    let trimmed = trimmed
        .split_once(',')
        .map_or(trimmed.as_str(), |(path, _)| path)
        .trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(quoted) = trimmed.strip_prefix('"') {
        let end = quoted.find('"')?;
        return Some(PathBuf::from(&quoted[..end]));
    }

    let token = trimmed.split_whitespace().next().unwrap_or_default();
    if token.is_empty() {
        None
    } else {
        Some(PathBuf::from(token))
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn is_installer_like(file_name: &str) -> bool {
    let lowered = file_name.to_ascii_lowercase();
    lowered.contains("setup")
        || lowered.contains("install")
        || lowered.contains("uninstall")
        || lowered.contains("updater")
        || lowered.contains("update")
        || lowered.contains("patch")
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn resolve_install_path_from_fields(
    install_location: Option<&str>,
    display_icon: Option<&str>,
    uninstall_string: Option<&str>,
) -> Option<PathBuf> {
    if let Some(location) = install_location {
        let location = expand_and_trim(location);
        if !location.is_empty() {
            let path = PathBuf::from(&location);
            if path.is_dir() {
                return Some(path);
            }
        }
    }

    if let Some(icon) = display_icon {
        if let Some(path) = parse_command_path(icon) {
            if path.is_file() {
                return Some(path.parent().map(Path::to_path_buf).unwrap_or(path));
            }
            if path.is_dir() {
                return Some(path);
            }
        }
    }

    if let Some(uninstall) = uninstall_string {
        if let Some(path) = parse_command_path(uninstall) {
            if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
                if file_name.eq_ignore_ascii_case("msiexec.exe") {
                    return None;
                }
            }
            if path.is_file() {
                return Some(path.parent().map(Path::to_path_buf).unwrap_or(path));
            }
            if path.is_dir() {
                return Some(path);
            }
        }
    }

    None
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn resolve_launch_path(
    display_icon: Option<&str>,
    install_path: &Path,
    candidate_names: &[&str],
) -> Option<PathBuf> {
    if let Some(icon) = display_icon {
        if let Some(path) = parse_command_path(icon) {
            if path.is_file() {
                return Some(path);
            }
        }
    }

    if install_path.is_file() {
        return Some(install_path.to_path_buf());
    }

    if !install_path.is_dir() {
        return None;
    }

    let normalized_candidates: Vec<String> = candidate_names
        .iter()
        .map(|name| normalize(name))
        .filter(|name| name.len() >= 3)
        .collect();

    let mut fallback = None;
    let Ok(read_dir) = std::fs::read_dir(install_path) else {
        return None;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !file_name.to_ascii_lowercase().ends_with(".exe") {
            continue;
        }
        if is_installer_like(file_name) {
            continue;
        }
        let stem = normalize(
            path.file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or_default(),
        );
        if normalized_candidates
            .iter()
            .any(|candidate| stem == *candidate)
        {
            return Some(path);
        }
        if fallback.is_none() {
            fallback = Some(path);
        }
    }

    fallback
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{
        WindowsInstallDiscovery, normalize, normalize_version, resolve_install_path_from_fields,
        resolve_launch_path,
    };
    use anyhow::Result;
    use std::path::PathBuf;
    use winreg::enums::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
    };
    use winreg::RegKey;

    #[derive(Debug, Clone)]
    struct RegistryEntry {
        display_name: String,
        display_version: Option<String>,
        install_location: Option<String>,
        display_icon: Option<String>,
        uninstall_string: Option<String>,
        system_component: bool,
    }

    pub fn discover_installation(
        candidate_names: &[&str],
        candidate_versions: &[&str],
    ) -> Result<Option<WindowsInstallDiscovery>> {
        let mut best: Option<(i32, WindowsInstallDiscovery)> = None;

        for entry in read_entries()? {
            if entry.system_component {
                continue;
            }

            let Some(name_score) = score_name(&entry.display_name, candidate_names) else {
                continue;
            };
            let version_score = score_version(entry.display_version.as_deref(), candidate_versions);
            let Some(install_path) = resolve_install_path(&entry) else {
                continue;
            };
            let launch_path = resolve_launch_path(
                entry.display_icon.as_deref(),
                &install_path,
                candidate_names,
            );
            let score = name_score + version_score;
            let discovery = WindowsInstallDiscovery {
                install_path,
                launch_path,
            };

            if best
                .as_ref()
                .is_none_or(|(best_score, _)| score > *best_score)
            {
                best = Some((score, discovery));
            }
        }

        Ok(best.map(|(_, discovery)| discovery))
    }

    fn read_entries() -> Result<Vec<RegistryEntry>> {
        let mut entries = Vec::new();
        for (hive, view) in [
            (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
            (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
            (HKEY_CURRENT_USER, KEY_WOW64_64KEY),
            (HKEY_CURRENT_USER, KEY_WOW64_32KEY),
        ] {
            let root = RegKey::predef(hive);
            let uninstall = match root.open_subkey_with_flags(
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
                KEY_READ | view,
            ) {
                Ok(key) => key,
                Err(_) => continue,
            };

            for key_name in uninstall.enum_keys().flatten() {
                let Ok(entry) = uninstall.open_subkey_with_flags(&key_name, KEY_READ | view) else {
                    continue;
                };
                let display_name = match entry.get_value::<String, _>("DisplayName") {
                    Ok(value) if !value.trim().is_empty() => value,
                    _ => continue,
                };
                let display_version = entry.get_value::<String, _>("DisplayVersion").ok();
                let install_location = entry.get_value::<String, _>("InstallLocation").ok();
                let display_icon = entry.get_value::<String, _>("DisplayIcon").ok();
                let uninstall_string = entry.get_value::<String, _>("UninstallString").ok();
                let system_component = entry
                    .get_value::<u32, _>("SystemComponent")
                    .map(|value| value != 0)
                    .unwrap_or(false);

                entries.push(RegistryEntry {
                    display_name,
                    display_version,
                    install_location,
                    display_icon,
                    uninstall_string,
                    system_component,
                });
            }
        }

        Ok(entries)
    }

    fn score_name(display_name: &str, candidate_names: &[&str]) -> Option<i32> {
        let normalized_name = normalize(display_name);
        if normalized_name.len() < 3 {
            return None;
        }

        let mut best: Option<i32> = None;
        for candidate in candidate_names {
            let normalized_candidate = normalize(candidate);
            if normalized_candidate.len() < 3 {
                continue;
            }

            if normalized_name == normalized_candidate {
                return Some(1_000);
            }

            if normalized_name.starts_with(&normalized_candidate) {
                best = Some(best.map_or(750, |current| current.max(750)));
            }
        }

        best
    }

    fn score_version(display_version: Option<&str>, candidate_versions: &[&str]) -> i32 {
        let Some(display_version) = display_version else {
            return 0;
        };
        let normalized_display = normalize_version(display_version);
        if normalized_display.is_empty() {
            return 0;
        }

        for candidate in candidate_versions {
            let normalized_candidate = normalize_version(candidate);
            if normalized_candidate.is_empty() {
                continue;
            }
            if normalized_display == normalized_candidate {
                return 200;
            }
        }
        0
    }

    fn resolve_install_path(entry: &RegistryEntry) -> Option<PathBuf> {
        resolve_install_path_from_fields(
            entry.install_location.as_deref(),
            entry.display_icon.as_deref(),
            entry.uninstall_string.as_deref(),
        )
    }
}

#[cfg(target_os = "windows")]
pub use platform::discover_installation;

#[cfg(not(target_os = "windows"))]
pub fn discover_installation(
    _candidate_names: &[&str],
    _candidate_versions: &[&str],
) -> anyhow::Result<Option<WindowsInstallDiscovery>> {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::{
        is_installer_like, parse_command_path, resolve_install_path_from_fields,
        resolve_launch_path,
    };
    use std::fs;

    #[test]
    fn parses_display_icon_command_paths_with_commas_and_quotes() {
        let path = parse_command_path(r#""C:\Program Files\ReleaseDock\ReleaseDock.exe",0"#)
            .expect("command path should parse");

        assert_eq!(
            path,
            std::path::PathBuf::from(r"C:\Program Files\ReleaseDock\ReleaseDock.exe")
        );
    }

    #[test]
    fn identifies_installer_like_names() {
        assert!(is_installer_like("ReleaseDock_0.2.5_x64-setup.exe"));
        assert!(is_installer_like("ReleaseDock-updater.exe"));
        assert!(!is_installer_like("ReleaseDock.exe"));
    }

    #[test]
    fn resolves_install_path_from_display_icon_and_ignores_msiexec_uninstallers() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app_dir = temp.path().join("ReleaseDock");
        fs::create_dir_all(&app_dir).expect("dir");
        let exe = app_dir.join("ReleaseDock.exe");
        fs::write(&exe, b"fake").expect("file");

        let install_path = resolve_install_path_from_fields(
            None,
            Some(&format!(r#""{}",0"#, exe.display())),
            Some(r#""C:\Windows\System32\msiexec.exe" /x {GUID}"#),
        )
        .expect("install path should resolve");

        assert_eq!(install_path, app_dir);
    }

    #[test]
    fn prefers_matching_launch_target_and_skips_installer_executables() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app_dir = temp.path().join("ReleaseDock");
        fs::create_dir_all(&app_dir).expect("dir");
        let setup_exe = app_dir.join("ReleaseDock-setup.exe");
        let app_exe = app_dir.join("ReleaseDock.exe");
        let helper_exe = app_dir.join("helper.exe");
        fs::write(&setup_exe, b"fake").expect("file");
        fs::write(&app_exe, b"fake").expect("file");
        fs::write(&helper_exe, b"fake").expect("file");

        let launch_path = resolve_launch_path(None, &app_dir, &["ReleaseDock"]);

        assert_eq!(launch_path, Some(app_exe));
    }
}
