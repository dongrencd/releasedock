//! Windows 系统代理解析。
//!
//! ReleaseDock 不内置任何 GitHub 中转。此模块只读取当前 Windows 用户已经
//! 信任的网络策略（手工代理、PAC 或 WPAD），并把结果交给 HTTP 客户端使用。

use url::Url;

/// HTTP 客户端实际采用的代理来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxySource {
    /// ReleaseDock 配置或环境变量中的显式代理。
    Explicit,
    /// Windows 当前用户的手工代理配置。
    WindowsManual,
    /// Windows PAC 或 WPAD 自动代理配置。
    WindowsAuto,
    /// 没有可用代理，保持官方地址直连。
    Direct,
}

impl ProxySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicitProxy",
            Self::WindowsManual => "windowsManualProxy",
            Self::WindowsAuto => "windowsAutoProxy",
            Self::Direct => "direct",
        }
    }

    pub fn uses_proxy(self) -> bool {
        !matches!(self, Self::Direct)
    }
}

/// 解析后的 Windows Internet Settings。字符串均已从 Windows 分配的缓冲区复制，
/// 因此不会把系统 API 的所有权泄漏到调用方。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowsProxySettings {
    pub auto_detect: bool,
    pub auto_config_url: Option<String>,
    pub manual_proxy: Option<String>,
    pub manual_bypass: Option<String>,
}

impl WindowsProxySettings {
    pub fn preferred_source(&self) -> ProxySource {
        if self.auto_detect || self.auto_config_url.is_some() {
            ProxySource::WindowsAuto
        } else if self.manual_proxy.is_some() {
            ProxySource::WindowsManual
        } else {
            ProxySource::Direct
        }
    }

    /// 对没有 PAC/WPAD 的系统代理执行纯字符串解析，供 Windows 运行时和跨平台
    /// 单元测试共用。PAC/WPAD 的按 URL 决策由 Windows WinHTTP 完成。
    pub fn manual_proxy_for_url(&self, url: &Url) -> Option<String> {
        let host = url.host_str()?;
        if self
            .manual_bypass
            .as_deref()
            .is_some_and(|rules| bypass_matches(host, rules))
        {
            return None;
        }
        self.manual_proxy
            .as_deref()
            .and_then(|proxy| select_proxy_for_scheme(proxy, url.scheme()))
    }
}

/// 合并自动和手工策略。自动策略成功返回 `DIRECT` 时不能继续使用手工代理，
/// 否则 PAC 针对某个 GitHub 域名定义的直连例外会被破坏。
pub fn effective_proxy_for_url(
    settings: &WindowsProxySettings,
    url: &Url,
    automatic_proxy: Option<Option<String>>,
) -> Option<String> {
    automatic_proxy.unwrap_or_else(|| settings.manual_proxy_for_url(url))
}

/// 选择 Windows `ProxyServer` 中与请求 scheme 相符的代理。Windows 支持
/// `http=host:port;https=host:port` 和单一 `host:port` 两种格式。
pub fn select_proxy_for_scheme(proxy_list: &str, scheme: &str) -> Option<String> {
    let requested_scheme = scheme.trim().to_ascii_lowercase();
    let mut fallback = None;

    for item in proxy_list
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let (name, value) = match item.split_once('=') {
            Some((name, value)) => (Some(name.trim().to_ascii_lowercase()), value.trim()),
            None => (None, item),
        };
        if value.eq_ignore_ascii_case("direct") {
            continue;
        }
        if name.as_deref().is_none_or(|name| name == requested_scheme) {
            let normalized = normalize_proxy_url(value)?;
            if name.is_some() {
                return Some(normalized);
            }
            fallback = Some(normalized);
        }
    }

    fallback
}

/// WinHTTP 的 PAC 结果可能是 `PROXY host:port; DIRECT`；reqwest 只接受 URL，
/// 因此在保留 HTTPS 的前提下剥离 WinHTTP 指令关键字。
pub fn normalize_proxy_url(value: &str) -> Option<String> {
    let candidate = value
        .split(';')
        .map(str::trim)
        .find_map(|item| {
            let upper = item.to_ascii_uppercase();
            if upper == "DIRECT" || upper.starts_with("SOCKS") {
                return None;
            }
            Some(
                item.strip_prefix("PROXY ")
                    .or_else(|| item.strip_prefix("HTTPS "))
                    .or_else(|| item.strip_prefix("HTTP "))
                    .unwrap_or(item),
            )
        })?
        .trim();

    if candidate.is_empty() {
        return None;
    }
    let url = if candidate.contains("://") {
        Url::parse(candidate).ok()?
    } else {
        Url::parse(&format!("http://{candidate}")).ok()?
    };
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

/// 判断 Windows ProxyOverride 是否要求直连。GitHub 不应匹配 `<local>`，但仍
/// 完整处理该规则和常见通配符，避免把企业配置错误地应用到所有请求。
pub fn bypass_matches(host: &str, bypass_list: &str) -> bool {
    let normalized_host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    bypass_list
        .split([';', ','])
        .map(str::trim)
        .filter(|rule| !rule.is_empty())
        .any(|rule| {
            let rule = rule.trim().to_ascii_lowercase();
            if rule == "<local>" {
                return !normalized_host.contains('.');
            }
            let rule = rule.trim_start_matches("*.").trim_start_matches('.');
            normalized_host == rule || normalized_host.ends_with(&format!(".{rule}"))
        })
}

/// 读取当前平台提供的系统代理策略。非 Windows 平台始终返回空配置，确保 CLI
/// 与 Linux/macOS 构建保持现有直连/显式代理行为。
#[cfg(not(windows))]
pub fn current_user_settings() -> WindowsProxySettings {
    WindowsProxySettings::default()
}

#[cfg(windows)]
pub fn current_user_settings() -> WindowsProxySettings {
    use windows::{
        Win32::{
            Foundation::{GlobalFree, HGLOBAL},
            Networking::WinHttp::{
                WINHTTP_CURRENT_USER_IE_PROXY_CONFIG, WinHttpGetIEProxyConfigForCurrentUser,
            },
        },
        core::PWSTR,
    };

    // WinHTTP 将每个字符串分配为 GlobalAlloc 缓冲区；复制后立即释放，避免后台
    // 更新周期反复读取策略时积累 Windows 堆内存。
    unsafe fn take_global_string(value: PWSTR) -> Option<String> {
        if value.is_null() {
            return None;
        }
        let copied = value
            .to_string()
            .ok()
            .filter(|value| !value.trim().is_empty());
        let _ = GlobalFree(Some(HGLOBAL(value.0.cast())));
        copied
    }

    let mut raw = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG::default();
    if unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut raw) }.is_err() {
        return WindowsProxySettings::default();
    }

    WindowsProxySettings {
        auto_detect: raw.fAutoDetect.as_bool(),
        auto_config_url: unsafe { take_global_string(raw.lpszAutoConfigUrl) },
        manual_proxy: unsafe { take_global_string(raw.lpszProxy) },
        manual_bypass: unsafe { take_global_string(raw.lpszProxyBypass) },
    }
}

/// 使用 Windows PAC/WPAD 为某个 URL 解析实际代理。
///
/// 外层 `None` 表示自动解析未运行或失败，调用方可以降级到手工系统代理；
/// `Some(None)` 表示 PAC 明确给出 `DIRECT`（或不受支持的代理类型），必须保持
/// 直连，不能错误回退到手工代理。不会把 PAC 地址、账户或代理地址暴露给 UI。
#[cfg(windows)]
pub fn auto_proxy_for_url(settings: &WindowsProxySettings, url: &Url) -> Option<Option<String>> {
    use windows::{
        Win32::{
            Foundation::{GlobalFree, HGLOBAL},
            Networking::WinHttp::{
                WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_AUTO_DETECT_TYPE_DHCP,
                WINHTTP_AUTO_DETECT_TYPE_DNS_A, WINHTTP_AUTOPROXY_AUTO_DETECT,
                WINHTTP_AUTOPROXY_CONFIG_URL, WINHTTP_AUTOPROXY_OPTIONS, WINHTTP_PROXY_INFO,
                WinHttpCloseHandle, WinHttpGetProxyForUrl, WinHttpOpen,
            },
        },
        core::{PCWSTR, PWSTR},
    };

    unsafe fn free_proxy_info(info: &mut WINHTTP_PROXY_INFO) {
        for value in [info.lpszProxy, info.lpszProxyBypass] {
            if !value.is_null() {
                let _ = GlobalFree(Some(HGLOBAL(value.0.cast())));
            }
        }
    }

    fn resolve_auto_proxy(
        session: *mut core::ffi::c_void,
        url: &[u16],
        options: &mut WINHTTP_AUTOPROXY_OPTIONS,
    ) -> Option<Option<String>> {
        let mut info = WINHTTP_PROXY_INFO::default();
        let result =
            unsafe { WinHttpGetProxyForUrl(session, PCWSTR(url.as_ptr()), options, &mut info) };
        if result.is_err() {
            return None;
        }
        let proxy = if info.lpszProxy.is_null() {
            None
        } else {
            unsafe { PWSTR(info.lpszProxy.0).to_string().ok() }
                .and_then(|value| normalize_proxy_url(&value))
        };
        unsafe { free_proxy_info(&mut info) };
        Some(proxy)
    }

    let url_wide = url
        .as_str()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let agent = "ReleaseDock"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let session = unsafe {
        WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_NO_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    };
    if session.is_null() {
        return None;
    }

    let mut options = WINHTTP_AUTOPROXY_OPTIONS::default();
    if let Some(auto_config_url) = settings.auto_config_url.as_deref() {
        let pac = auto_config_url
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        options.dwFlags = WINHTTP_AUTOPROXY_CONFIG_URL;
        options.lpszAutoConfigUrl = PCWSTR(pac.as_ptr());
        let proxy = resolve_auto_proxy(session, &url_wide, &mut options);
        unsafe { WinHttpCloseHandle(session) };
        return proxy;
    }
    if settings.auto_detect {
        options.dwFlags = WINHTTP_AUTOPROXY_AUTO_DETECT;
        options.dwAutoDetectFlags = WINHTTP_AUTO_DETECT_TYPE_DHCP | WINHTTP_AUTO_DETECT_TYPE_DNS_A;
        let proxy = resolve_auto_proxy(session, &url_wide, &mut options);
        unsafe { WinHttpCloseHandle(session) };
        return proxy;
    }

    unsafe { WinHttpCloseHandle(session) };
    None
}

#[cfg(not(windows))]
pub fn auto_proxy_for_url(_settings: &WindowsProxySettings, _url: &Url) -> Option<Option<String>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_scheme_specific_windows_proxy() {
        assert_eq!(
            select_proxy_for_scheme("http=http-proxy:8080;https=secure-proxy:8443", "https"),
            Some("http://secure-proxy:8443/".to_string())
        );
        assert_eq!(
            select_proxy_for_scheme("shared-proxy:8080", "https"),
            Some("http://shared-proxy:8080/".to_string())
        );
    }

    #[test]
    fn parses_pac_proxy_results_without_accepting_socks() {
        assert_eq!(
            normalize_proxy_url("PROXY proxy.example.com:8080; DIRECT"),
            Some("http://proxy.example.com:8080/".to_string())
        );
        assert_eq!(
            normalize_proxy_url("SOCKS socks.example.com:1080; DIRECT"),
            None
        );
    }

    #[test]
    fn respects_windows_bypass_rules() {
        assert!(bypass_matches("api.github.com", "*.github.com;localhost"));
        assert!(bypass_matches("intranet", "<local>"));
        assert!(!bypass_matches("api.github.com", "<local>;*.example.com"));
    }

    #[test]
    fn preserves_a_pac_direct_rule_instead_of_falling_back_to_manual_proxy() {
        let settings = WindowsProxySettings {
            manual_proxy: Some("https=manual-proxy.example:8443".to_string()),
            ..WindowsProxySettings::default()
        };
        let url = Url::parse("https://api.github.com/rate_limit").expect("valid URL");

        assert_eq!(effective_proxy_for_url(&settings, &url, Some(None)), None);
        assert_eq!(
            effective_proxy_for_url(&settings, &url, None),
            Some("http://manual-proxy.example:8443/".to_string())
        );
    }
}
