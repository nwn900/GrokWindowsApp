// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;
use std::sync::atomic::{AtomicBool, Ordering};

static IS_QUITTING: AtomicBool = AtomicBool::new(false);

const TARGET_URL: &str = "https://grok.com/";

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
    ALLOWED_HOSTS.iter().any(|suffix| {
        hostname == *suffix || hostname.ends_with(&format!(".{}", suffix))
    })
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

            let main_window = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(target_url),
            )
            .title("Grok")
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36 Edg/124.0.0.0")
            .initialization_script(r#"
                window.open = function(url, name, features) {
                    if (url) { window.location.assign(url); }
                    return { close: function(){}, focus: function(){} };
                };
                document.addEventListener('click', function(e) {
                    let target = e.target.closest('a');
                    if (target && target.getAttribute('target') === '_blank') {
                        target.setAttribute('target', '_self');
                    }
                }, true);
                window.addEventListener('load', function() {
                    if (window.location.href.includes('callback') || window.location.href.includes('/auth/')) {
                        setTimeout(function() {
                            window.location.assign('/');
                        }, 1500);
                    }
                });
            "#)
            .inner_size(1200.0, 900.0)
            .auto_resize()
            .on_navigation(|url| {
                // Allow navigation to Gemini and Google auth domains
                if let Some(host) = url.host_str() {
                    is_allowed_host(host)
                } else {
                    false
                }
            })
            .build()?;

            // Hide to tray on close
            let win_clone = main_window.clone();
            main_window.on_window_event(move |event| {
                match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        if !IS_QUITTING.load(Ordering::SeqCst) {
                            api.prevent_close();
                            let _ = win_clone.hide();
                        }
                    }
                    _ => {}
                }
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

            let menu = Menu::with_items(app, &[&open_item, &startup_item, &separator, &close_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().unwrap())
                .tooltip("Grok")
                .menu(&menu)
                .on_menu_event(move |app_handle, event| {
                    match event.id().as_ref() {
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
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    match event {
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
                    }
                })
                .show_menu_on_left_click(false)
                .build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Grok");
}
