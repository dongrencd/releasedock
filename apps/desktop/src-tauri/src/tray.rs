// 系统托盘模块
//
// 功能：
// - 创建托盘图标和右键菜单（检查更新 / 打开窗口 / 退出）
// - 左键单击托盘图标恢复窗口
// - 通过 tooltip 表示更新计数（跨平台兼容）
// - 有更新 > 0 时图标带角标，否则使用默认图标

use releasedock_core::config::Language;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Runtime};

/// 创建系统托盘图标和菜单
pub fn build_tray<R: Runtime>(app: &tauri::AppHandle<R>, language: Language) -> tauri::Result<()> {
    rebuild_tray(app, language)
}

/// 语言切换后重建托盘菜单，避免 Windows 托盘菜单在保存设置后仍显示旧语言。
pub fn rebuild_tray<R: Runtime>(app: &tauri::AppHandle<R>, language: Language) -> tauri::Result<()> {
    if let Some(existing) = app.tray_by_id("main") {
        let _ = app.remove_tray_by_id(existing.id());
    }

    let check_item = MenuItem::with_id(
        app,
        "tray_check",
        &crate::tr_owned(language, "Check updates", "检查更新"),
        true,
        None::<&str>,
    )?;
    let show_item = MenuItem::with_id(
        app,
        "tray_show",
        &crate::tr_owned(language, "Open window", "打开窗口"),
        true,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(
        app,
        "tray_quit",
        &crate::tr_owned(language, "Quit", "退出"),
        true,
        None::<&str>,
    )?;

    let menu = Menu::with_items(app, &[&check_item, &show_item, &separator, &quit_item])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .expect("app should have a default window icon");

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .menu(&menu)
        .tooltip("ReleaseDock")
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "tray_check" => {
                    // 触发前端刷新（通过事件通知前端调用 refreshDashboard）
                    let _ = app.emit("tray-check-updates", ());
                }
                "tray_show" => {
                    crate::restore_main_window(app);
                }
                "tray_quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = event
            {
                crate::restore_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

/// 更新托盘 tooltip 显示更新计数
///
/// 跨平台兼容方案：Linux 没有统一 badge API，
/// 用 tooltip 文案传达"N updates available"信息。
pub fn update_tray_tooltip<R: Runtime>(app: &tauri::AppHandle<R>, update_count: usize, language: Language) {
    let tooltip = if update_count > 0 {
        let suffix = match language {
            Language::En => format!("· {update_count} updates available"),
            Language::ZhCn => format!("· {update_count} 个有更新"),
        };
        format!("ReleaseDock {suffix}")
    } else {
        "ReleaseDock".to_string()
    };

    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(&tooltip));
    }
}
