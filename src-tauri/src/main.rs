// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    webview::NewWindowResponse,
    Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;

static IS_QUITTING: AtomicBool = AtomicBool::new(false);
static NEXT_POPUP_ID: AtomicUsize = AtomicUsize::new(1);

const TARGET_URL: &str = "https://grok.com/";
const WINDOW_TITLE: &str = "Grok";
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36 Edg/124.0.0.0";

const ALLOWED_HOSTS: &[&str] = &[
    "grok.com",
    "x.ai",
    "x.com",
    "twitter.com",
    "t.co",
    "google.com",
    "googleusercontent.com",
    "microsoftonline.com",
    "live.com",
    "microsoft.com",
    "onedrive.com",
];

fn is_allowed_host(hostname: &str) -> bool {
    ALLOWED_HOSTS
        .iter()
        .any(|suffix| hostname == *suffix || hostname.ends_with(&format!(".{}", suffix)))
}

fn is_allowed_url(url: &url::Url) -> bool {
    match url.scheme() {
        "http" | "https" => url.host_str().is_some_and(is_allowed_host),
        "about" => url.path() == "blank",
        _ => false,
    }
}

fn next_popup_label() -> String {
    format!("popup-{}", NEXT_POPUP_ID.fetch_add(1, Ordering::Relaxed))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let target_url: url::Url = TARGET_URL.parse().unwrap();
            let popup_app_handle = app.handle().clone();

            let main_window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::External(target_url))
                    .title(WINDOW_TITLE)
                    .user_agent(USER_AGENT)
                    .inner_size(1200.0, 900.0)
                    .auto_resize()
                    .on_navigation(is_allowed_url)
                    .on_new_window(move |url, features| {
                        if !is_allowed_url(&url) {
                            return NewWindowResponse::Deny;
                        }

                        let popup_builder = WebviewWindowBuilder::new(
                            &popup_app_handle,
                            next_popup_label(),
                            WebviewUrl::External(url.clone()),
                        )
                        .title(WINDOW_TITLE)
                        .user_agent(USER_AGENT)
                        .window_features(features)
                        .on_navigation(is_allowed_url)
                        .on_document_title_changed(|window, title| {
                            let title = if title.trim().is_empty() {
                                WINDOW_TITLE.to_string()
                            } else {
                                title
                            };
                            let _ = window.set_title(&title);
                        });

                        match popup_builder.build() {
                            Ok(window) => NewWindowResponse::Create { window },
                            Err(_) => NewWindowResponse::Deny,
                        }
                    })
                    .build()?;

            // If autostart is enabled, launch minimized to tray
            if app.autolaunch().is_enabled().unwrap_or(false) {
                let _ = main_window.hide();
            }

            // Hide to tray on close
            let win_clone = main_window.clone();
            main_window.on_window_event(move |event| match event {
                WindowEvent::CloseRequested { api, .. } => {
                    if !IS_QUITTING.load(Ordering::SeqCst) {
                        api.prevent_close();
                        let _ = win_clone.hide();
                    }
                }
                _ => {}
            });

            // Build system tray menu
            let is_enabled = app.autolaunch().is_enabled().unwrap_or(false);

            let open_item = MenuItem::with_id(app, "open", "Open Grok", true, None::<&str>)?;
            let startup_item = CheckMenuItem::with_id(
                app,
                "startup",
                "Launch at system startup",
                true,
                is_enabled,
                None::<&str>,
            )?;
            let separator = PredefinedMenuItem::separator(app)?;
            let close_item = MenuItem::with_id(app, "close", "Close Grok", true, None::<&str>)?;

            let menu =
                Menu::with_items(app, &[&open_item, &startup_item, &separator, &close_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().unwrap())
                .tooltip(WINDOW_TITLE)
                .menu(&menu)
                .on_menu_event(move |app_handle, event| match event.id().as_ref() {
                    "open" => {
                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "startup" => {
                        let manager = app_handle.autolaunch();
                        let currently_enabled = manager.is_enabled().unwrap_or(false);
                        if currently_enabled {
                            let _ = manager.disable();
                        } else {
                            let _ = manager.enable();
                        }
                    }
                    "close" => {
                        IS_QUITTING.store(true, Ordering::SeqCst);
                        app_handle.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| match event {
                    tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } => {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .show_menu_on_left_click(false)
                .build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Grok");
}

#[cfg(test)]
mod tests {
    use super::{is_allowed_host, is_allowed_url};

    #[test]
    fn allows_supported_auth_hosts() {
        assert!(is_allowed_host("grok.com"));
        assert!(is_allowed_host("api.x.com"));
        assert!(is_allowed_host("subdomain.microsoftonline.com"));
    }

    #[test]
    fn rejects_unknown_hosts() {
        assert!(!is_allowed_host("example.com"));
        assert!(!is_allowed_host("grok.com.example.com"));
    }

    #[test]
    fn allows_about_blank_and_whitelisted_urls() {
        let grok_callback: url::Url = "https://grok.com/auth/callback".parse().unwrap();
        let blank: url::Url = "about:blank".parse().unwrap();
        let blocked: url::Url = "https://example.com/login".parse().unwrap();

        assert!(is_allowed_url(&grok_callback));
        assert!(is_allowed_url(&blank));
        assert!(!is_allowed_url(&blocked));
    }
}
