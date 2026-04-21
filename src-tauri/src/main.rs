// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    webview::{NewWindowResponse, PageLoadEvent},
    Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;
#[cfg(windows)]
use webview2_com::{
    CoTaskMemPWSTR, WebMessageReceivedEventHandler, WebResourceRequestedEventHandler,
    WebResourceResponseReceivedEventHandler,
};
#[cfg(windows)]
use windows::core::Interface;

static IS_QUITTING: AtomicBool = AtomicBool::new(false);
static NEXT_POPUP_ID: AtomicUsize = AtomicUsize::new(1);

const TARGET_URL: &str = "https://grok.com/";
const WINDOW_TITLE: &str = "Grok";
const POPUP_URL: &str = "about:blank";
const DIAGNOSTICS_DIR: &str = "diagnostics";
const AUTH_DEBUG_LOG: &str = "auth-debug.log";
const AUTH_DIAGNOSTIC_CHANNEL: &str = "grok-auth-diag";
const WEBVIEW_DATA_DIR: &str = "webview2";
#[cfg(windows)]
const WEB_RESOURCE_FILTERS: &[&str] = &[
    "https://grok.com/*",
    "https://*.grok.com/*",
    "https://x.com/*",
    "https://*.x.com/*",
    "https://twitter.com/*",
    "https://*.twitter.com/*",
    "https://t.co/*",
];
const ADDITIONAL_BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";
const INITIALIZATION_SCRIPT: &str = r#"
    function __grokFilterBrands(brands) {
        return Array.isArray(brands)
            ? brands.filter((brand) => brand && !/webview2/i.test(String(brand.brand || '')))
            : brands;
    }

    if ('userAgentData' in navigator && navigator.userAgentData) {
        try {
            const originalUserAgentData = navigator.userAgentData;
            const patchedUserAgentData = {
                get brands() {
                    return __grokFilterBrands(originalUserAgentData.brands);
                },
                get mobile() {
                    return originalUserAgentData.mobile;
                },
                get platform() {
                    return originalUserAgentData.platform;
                },
                toJSON() {
                    const base =
                        typeof originalUserAgentData.toJSON === 'function'
                            ? originalUserAgentData.toJSON()
                            : {
                                  brands: originalUserAgentData.brands,
                                  mobile: originalUserAgentData.mobile,
                                  platform: originalUserAgentData.platform
                              };
                    return {
                        ...base,
                        brands: __grokFilterBrands(base.brands)
                    };
                },
                async getHighEntropyValues(hints = []) {
                    const values = await originalUserAgentData.getHighEntropyValues(hints);
                    if ('brands' in values) {
                        values.brands = __grokFilterBrands(values.brands);
                    }
                    if ('fullVersionList' in values) {
                        values.fullVersionList = __grokFilterBrands(values.fullVersionList);
                    }
                    return values;
                }
            };

            Object.defineProperty(navigator, 'userAgentData', {
                get: () => patchedUserAgentData,
                configurable: true
            });
        } catch (_) {}
    }

    if (window.chrome && 'webview' in window.chrome) {
        try {
            Object.defineProperty(window.chrome, 'webview', {
                get: () => undefined,
                configurable: true
            });
        } catch (_) {
            try {
                delete window.chrome.webview;
            } catch (_) {}
        }
    }
"#;

const ALLOWED_HOSTS: &[&str] = &[
    "grok.com",
    "grokipedia.com",
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

#[derive(Clone)]
struct AuthLogger {
    file: Arc<Mutex<File>>,
}

#[derive(serde::Deserialize)]
struct JsDiagnosticMessage {
    channel: String,
    kind: String,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    target: Option<String>,
}

impl AuthLogger {
    fn new(path: PathBuf) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            file: Arc::new(Mutex::new(file)),
        })
    }

    fn log(&self, scope: &str, window_label: Option<&str>, message: impl AsRef<str>) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let window_fragment = window_label
            .map(|label| format!(" [{label}]"))
            .unwrap_or_default();
        let line = format!(
            "[{timestamp}] [{scope}]{window_fragment} {}\n",
            message.as_ref()
        );

        if let Ok(mut file) = self.file.lock() {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }
}

fn is_allowed_host(hostname: &str) -> bool {
    ALLOWED_HOSTS
        .iter()
        .any(|suffix| hostname == *suffix || hostname.ends_with(&format!(".{}", suffix)))
}

fn is_allowed_url(url: &url::Url) -> bool {
    match url.scheme() {
        "http" | "https" => url.host_str().is_some_and(is_allowed_host),
        "about" => url.path() == "blank",
        "blob" => url
            .path()
            .split_once(':')
            .and_then(|(scheme, rest)| match scheme {
                "http" | "https" => url::Url::parse(&format!("{scheme}:{rest}")).ok(),
                _ => None,
            })
            .is_some_and(|inner_url| inner_url.host_str().is_some_and(is_allowed_host)),
        _ => false,
    }
}

fn next_popup_label() -> String {
    format!("popup-{}", NEXT_POPUP_ID.fetch_add(1, Ordering::Relaxed))
}

fn clip_for_log(value: &str, max_chars: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let clipped: String = collapsed.chars().take(max_chars).collect();
    if collapsed.chars().count() > max_chars {
        format!("{clipped}...")
    } else {
        clipped
    }
}

fn summarize_url(url: &url::Url) -> String {
    let mut sanitized = url.clone();
    let query_keys = sanitized
        .query_pairs()
        .map(|(key, _)| key.into_owned())
        .fold(Vec::<String>::new(), |mut keys, key| {
            if !keys.contains(&key) {
                keys.push(key);
            }
            keys
        });

    sanitized.set_query(None);
    sanitized.set_fragment(None);

    let base = sanitized.to_string();
    if query_keys.is_empty() {
        base
    } else {
        format!("{base}?{}", query_keys.join(","))
    }
}

fn summarize_url_text(value: &str) -> String {
    url::Url::parse(value)
        .map(|url| summarize_url(&url))
        .unwrap_or_else(|_| clip_for_log(value, 180))
}

fn strip_webview2_brand(value: &str) -> Option<String> {
    let filtered = value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty() && !part.to_ascii_lowercase().contains("webview2"))
        .collect::<Vec<_>>();

    let joined = filtered.join(", ");
    (joined != value && !joined.is_empty()).then_some(joined)
}

#[cfg(windows)]
unsafe fn get_request_header(
    headers: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2HttpRequestHeaders,
    name: &str,
) -> Option<String> {
    let header_name = CoTaskMemPWSTR::from(name);
    let mut value = Default::default();
    headers
        .GetHeader(*header_name.as_ref().as_pcwstr(), &mut value)
        .ok()?;

    let value = CoTaskMemPWSTR::from(value).to_string();
    (!value.trim().is_empty()).then_some(value)
}

#[cfg(windows)]
unsafe fn get_response_header(
    headers: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2HttpResponseHeaders,
    name: &str,
) -> Option<String> {
    let header_name = CoTaskMemPWSTR::from(name);
    let mut value = Default::default();
    headers
        .GetHeader(*header_name.as_ref().as_pcwstr(), &mut value)
        .ok()?;

    let value = CoTaskMemPWSTR::from(value).to_string();
    (!value.trim().is_empty()).then_some(value)
}

fn is_auth_related_resource(url_text: &str) -> bool {
    let Ok(url) = url::Url::parse(url_text) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    if !is_allowed_host(host) {
        return false;
    }

    let path = url.path().to_ascii_lowercase();
    let query_keys = url
        .query_pairs()
        .map(|(key, _)| key.into_owned().to_ascii_lowercase())
        .collect::<Vec<_>>();

    path.contains("auth")
        || path.contains("oauth")
        || path.contains("login")
        || path.contains("authorize")
        || path.contains("onboarding")
        || path.contains("challenge")
        || path.contains("session")
        || path.contains("flow")
        || path.contains("task.json")
        || query_keys.iter().any(|key| {
            matches!(
                key.as_str(),
                "flow_name" | "redirect_after_login" | "screen_name" | "login_challenge"
            )
        })
}

fn log_js_diagnostic(logger: &AuthLogger, window_label: &str, message_json: &str) {
    match serde_json::from_str::<JsDiagnosticMessage>(message_json) {
        Ok(message) if message.channel == AUTH_DIAGNOSTIC_CHANNEL => {
            let mut parts = vec![format!("event={}", clip_for_log(&message.kind, 120))];

            if let Some(location) = message.location.as_deref() {
                parts.push(format!("location={}", summarize_url_text(location)));
            }
            if let Some(title) = message.title.as_deref() {
                parts.push(format!("title={}", clip_for_log(title, 120)));
            }
            if let Some(detail) = message.detail.as_deref() {
                parts.push(format!("detail={}", clip_for_log(detail, 180)));
            }
            if let Some(target) = message.target.as_deref() {
                parts.push(format!("target={}", clip_for_log(target, 180)));
            }

            logger.log("js", Some(window_label), parts.join(" "));
        }
        Ok(_) => {}
        Err(error) => logger.log(
            "js",
            Some(window_label),
            format!(
                "failed_to_parse={} payload={}",
                clip_for_log(&error.to_string(), 120),
                clip_for_log(message_json, 220)
            ),
        ),
    }
}

#[cfg(windows)]
fn attach_webview2_diagnostics<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
    logger: AuthLogger,
) {
    let label = window.label().to_string();
    let logger_for_handler = logger.clone();
    let label_for_handler = label.clone();
    let attach_result = window.with_webview(move |platform_webview| unsafe {
        match platform_webview.controller().CoreWebView2() {
            Ok(webview) => {
                let handler_logger = logger_for_handler.clone();
                let handler_label = label_for_handler.clone();
                let mut token = 0;

                match webview.add_WebMessageReceived(
                    &WebMessageReceivedEventHandler::create(Box::new(move |_sender, args| {
                        if let Some(args) = args {
                            let mut message = Default::default();
                            if args.WebMessageAsJson(&mut message).is_ok() {
                                let message = CoTaskMemPWSTR::from(message).to_string();
                                log_js_diagnostic(&handler_logger, &handler_label, &message);
                            }
                        }

                        Ok(())
                    })),
                    &mut token,
                ) {
                    Ok(_) => logger_for_handler.log(
                        "webview2",
                        Some(&label_for_handler),
                        "web message diagnostics attached",
                    ),
                    Err(error) => logger_for_handler.log(
                        "webview2",
                        Some(&label_for_handler),
                        format!("failed_to_attach_web_messages={error}"),
                    ),
                }

                for filter in WEB_RESOURCE_FILTERS {
                    let filter_pattern = CoTaskMemPWSTR::from(*filter);
                    if let Err(error) = webview.AddWebResourceRequestedFilter(
                        *filter_pattern.as_ref().as_pcwstr(),
                        webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                    ) {
                        logger_for_handler.log(
                            "resource-filter",
                            Some(&label_for_handler),
                            format!("pattern={} error={error}", filter),
                        );
                    }
                }

                let request_logger = logger_for_handler.clone();
                let request_label = label_for_handler.clone();
                let mut request_token = 0;
                match webview.add_WebResourceRequested(
                    &WebResourceRequestedEventHandler::create(Box::new(move |_sender, args| {
                        if let Some(args) = args {
                            if let Ok(request) = args.Request() {
                                let mut uri = Default::default();
                                let mut method = Default::default();
                                if request.Uri(&mut uri).is_ok() && request.Method(&mut method).is_ok() {
                                    let uri = CoTaskMemPWSTR::from(uri).to_string();
                                    if is_auth_related_resource(&uri) {
                                        let method = CoTaskMemPWSTR::from(method).to_string();
                                        let mut extra = String::new();
                                        if let Ok(headers) = request.Headers() {
                                            for header_name in
                                                ["sec-ch-ua", "sec-ch-ua-full-version-list"]
                                            {
                                                if let Some(original_value) =
                                                    get_request_header(&headers, header_name)
                                                {
                                                    if let Some(filtered_value) =
                                                        strip_webview2_brand(&original_value)
                                                    {
                                                        let header_name_ptr =
                                                            CoTaskMemPWSTR::from(header_name);
                                                        let filtered_value_ptr =
                                                            CoTaskMemPWSTR::from(
                                                                filtered_value.as_str(),
                                                            );
                                                        let _ = headers.SetHeader(
                                                            *header_name_ptr
                                                                .as_ref()
                                                                .as_pcwstr(),
                                                            *filtered_value_ptr
                                                                .as_ref()
                                                                .as_pcwstr(),
                                                        );
                                                    }
                                                }
                                            }

                                            if let Some(user_agent) =
                                                get_request_header(&headers, "user-agent")
                                            {
                                                extra.push_str(&format!(
                                                    " ua={}",
                                                    clip_for_log(&user_agent, 80)
                                                ));
                                            }
                                            if let Some(client_hints) =
                                                get_request_header(&headers, "sec-ch-ua")
                                            {
                                                extra.push_str(&format!(
                                                    " sec-ch-ua={}",
                                                    clip_for_log(&client_hints, 80)
                                                ));
                                            }
                                        }
                                        request_logger.log(
                                            "resource-request",
                                            Some(&request_label),
                                            format!(
                                                "method={} url={}{}",
                                                clip_for_log(&method, 12),
                                                summarize_url_text(&uri),
                                                extra
                                            ),
                                        );
                                    }
                                }
                            }
                        }

                        Ok(())
                    })),
                    &mut request_token,
                ) {
                    Ok(_) => logger_for_handler.log(
                        "resource-request",
                        Some(&label_for_handler),
                        "web resource diagnostics attached",
                    ),
                    Err(error) => logger_for_handler.log(
                        "resource-request",
                        Some(&label_for_handler),
                        format!("failed_to_attach_resource_handler={error}"),
                    ),
                }

                match webview.cast::<webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_2>() {
                    Ok(webview_v2) => {
                        let response_logger = logger_for_handler.clone();
                        let response_label = label_for_handler.clone();
                        let mut response_token = 0;
                        match webview_v2.add_WebResourceResponseReceived(
                            &WebResourceResponseReceivedEventHandler::create(Box::new(
                                move |_sender, args| {
                                    if let Some(args) = args {
                                        if let Ok(request) = args.Request() {
                                            let mut uri = Default::default();
                                            if request.Uri(&mut uri).is_ok() {
                                                let uri = CoTaskMemPWSTR::from(uri).to_string();
                                                if is_auth_related_resource(&uri) {
                                                    if let Ok(response) = args.Response() {
                                                        let mut status_code = 0;
                                                        let _ = response.StatusCode(&mut status_code);
                                                        let mut reason_phrase = Default::default();
                                                        let reason = if response
                                                            .ReasonPhrase(&mut reason_phrase)
                                                            .is_ok()
                                                        {
                                                            clip_for_log(
                                                                &CoTaskMemPWSTR::from(
                                                                    reason_phrase,
                                                                )
                                                                .to_string(),
                                                                40,
                                                            )
                                                        } else {
                                                            String::new()
                                                        };
                                                        let mut extra = String::new();
                                                        if let Ok(headers) = response.Headers() {
                                                            if let Some(location) =
                                                                get_response_header(
                                                                    &headers, "location",
                                                                )
                                                            {
                                                                extra.push_str(&format!(
                                                                    " location={}",
                                                                    summarize_url_text(&location)
                                                                ));
                                                            }
                                                        }

                                                        response_logger.log(
                                                            "resource-response",
                                                            Some(&response_label),
                                                            format!(
                                                                "status={} reason={} url={}{}",
                                                                status_code,
                                                                if reason.is_empty() {
                                                                    "-"
                                                                } else {
                                                                    &reason
                                                                },
                                                                summarize_url_text(&uri),
                                                                extra
                                                            ),
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    Ok(())
                                },
                            )),
                            &mut response_token,
                        ) {
                            Ok(_) => logger_for_handler.log(
                                "resource-response",
                                Some(&label_for_handler),
                                "web resource response diagnostics attached",
                            ),
                            Err(error) => logger_for_handler.log(
                                "resource-response",
                                Some(&label_for_handler),
                                format!("failed_to_attach_response_handler={error}"),
                            ),
                        }
                    }
                    Err(error) => logger_for_handler.log(
                        "resource-response",
                        Some(&label_for_handler),
                        format!("failed_to_cast_core_webview2_2={error}"),
                    ),
                }
            }
            Err(error) => logger_for_handler.log(
                "webview2",
                Some(&label_for_handler),
                format!("failed_to_access_core_webview2={error}"),
            ),
        }
    });

    if let Err(error) = attach_result {
        logger.log(
            "webview2",
            Some(&label),
            format!("with_webview_failed={error}"),
        );
    }
}

#[cfg(not(windows))]
fn attach_webview2_diagnostics<R: tauri::Runtime>(
    _window: &tauri::WebviewWindow<R>,
    _logger: AuthLogger,
) {
}

fn instrument_webview_builder<'a, R: tauri::Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
    window_label: String,
    logger: AuthLogger,
) -> WebviewWindowBuilder<'a, R, M> {
    let nav_logger = logger.clone();
    let nav_label = window_label.clone();
    let page_logger = logger.clone();
    let page_label = window_label.clone();
    let title_logger = logger;
    let title_label = window_label;

    builder
        .on_navigation(move |url| {
            let allowed = is_allowed_url(url);
            nav_logger.log(
                "navigation",
                Some(nav_label.as_str()),
                format!("allowed={allowed} url={}", summarize_url(url)),
            );
            allowed
        })
        .on_page_load(move |_window, payload| {
            let event = match payload.event() {
                PageLoadEvent::Started => "started",
                PageLoadEvent::Finished => "finished",
            };

            page_logger.log(
                "page-load",
                Some(page_label.as_str()),
                format!("event={event} url={}", summarize_url(payload.url())),
            );
        })
        .on_document_title_changed(move |window, title| {
            let window_title = if title.trim().is_empty() {
                WINDOW_TITLE.to_string()
            } else {
                title
            };

            title_logger.log(
                "title",
                Some(title_label.as_str()),
                format!("title={}", clip_for_log(&window_title, 140)),
            );
            let _ = window.set_title(&window_title);
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
            let popup_app_handle = app.handle().clone();
            let app_data_dir = app.path().app_local_data_dir()?;
            let diagnostics_dir = app_data_dir.join(DIAGNOSTICS_DIR);
            let auth_log_path = diagnostics_dir.join(AUTH_DEBUG_LOG);
            let auth_log_path_for_menu = auth_log_path.clone();
            let logger = AuthLogger::new(auth_log_path.clone())?;
            let webview_data_dir = app_data_dir.join(WEBVIEW_DATA_DIR);
            fs::create_dir_all(&webview_data_dir)?;
            logger.log(
                "startup",
                None,
                format!(
                    "session_start version={} pid={} log={} webview_data={}",
                    app.package_info().version,
                    std::process::id(),
                    auth_log_path.display(),
                    webview_data_dir.display()
                ),
            );

            let main_window = instrument_webview_builder(
                WebviewWindowBuilder::new(app, "main", WebviewUrl::External(target_url))
                    .title(WINDOW_TITLE)
                    .additional_browser_args(ADDITIONAL_BROWSER_ARGS)
                    .data_directory(webview_data_dir.clone())
                    .initialization_script(INITIALIZATION_SCRIPT)
                    .inner_size(1200.0, 900.0)
                    .auto_resize()
                    .on_new_window({
                        let popup_logger = logger.clone();
                        move |url, features| {
                            let popup_label = next_popup_label();
                            popup_logger.log(
                                "popup-request",
                                Some("main"),
                                format!("label={} url={}", popup_label, summarize_url(&url)),
                            );

                            if !is_allowed_url(&url) {
                                popup_logger.log(
                                    "popup-request",
                                    Some("main"),
                                    format!("label={} denied=true", popup_label),
                                );
                                return NewWindowResponse::Deny;
                            }

                            let popup_url: url::Url = POPUP_URL.parse().unwrap();
                            let popup_builder = instrument_webview_builder(
                                WebviewWindowBuilder::new(
                                    &popup_app_handle,
                                    popup_label.clone(),
                                    WebviewUrl::External(popup_url),
                                )
                                // Tauri/Wry binds requested popup URL/opener state into the created webview.
                                // Starting popup at about:blank avoids double-loading the target URL.
                                .title(url.as_str())
                                .additional_browser_args(ADDITIONAL_BROWSER_ARGS)
                                .data_directory(webview_data_dir.clone())
                                .initialization_script(INITIALIZATION_SCRIPT)
                                .window_features(features),
                                popup_label.clone(),
                                popup_logger.clone(),
                            );

                            match popup_builder.build() {
                                Ok(window) => {
                                    popup_logger.log(
                                        "popup-request",
                                        Some("main"),
                                        format!("label={} created=true", popup_label),
                                    );
                                    attach_webview2_diagnostics(&window, popup_logger.clone());
                                    NewWindowResponse::Create { window }
                                }
                                Err(error) => {
                                    popup_logger.log(
                                        "popup-request",
                                        Some("main"),
                                        format!("label={} build_error={error}", popup_label),
                                    );
                                    NewWindowResponse::Deny
                                }
                            }
                        }
                    }),
                "main".to_string(),
                logger.clone(),
            )
            .build()?;
            attach_webview2_diagnostics(&main_window, logger.clone());

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
            let log_item = MenuItem::with_id(
                app,
                "open_auth_log",
                "Open Auth Debug Log",
                true,
                None::<&str>,
            )?;
            let separator = PredefinedMenuItem::separator(app)?;
            let close_item = MenuItem::with_id(app, "close", "Close Grok", true, None::<&str>)?;

            let menu = Menu::with_items(
                app,
                &[
                    &open_item,
                    &startup_item,
                    &log_item,
                    &separator,
                    &close_item,
                ],
            )?;

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
                    "open_auth_log" => {
                        logger.log(
                            "shell",
                            Some("main"),
                            format!("open_auth_log path={}", auth_log_path_for_menu.display()),
                        );
                        let _ = std::process::Command::new("explorer.exe")
                            .arg(format!("/select,{}", auth_log_path_for_menu.display()))
                            .spawn();
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
    use super::{is_allowed_host, is_allowed_url, summarize_url};

    #[test]
    fn allows_supported_auth_hosts() {
        assert!(is_allowed_host("grok.com"));
        assert!(is_allowed_host("auth.grokipedia.com"));
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
        let blob: url::Url = "blob:https://x.com/12345678-1234-1234-1234-123456789012"
            .parse()
            .unwrap();
        let blocked: url::Url = "https://example.com/login".parse().unwrap();

        assert!(is_allowed_url(&grok_callback));
        assert!(is_allowed_url(&blank));
        assert!(is_allowed_url(&blob));
        assert!(!is_allowed_url(&blocked));
    }

    #[test]
    fn redacts_query_values_in_logged_urls() {
        let url: url::Url =
            "https://x.com/i/flow/login?redirect_after_login=%2Fhome&state=secret#step=email"
                .parse()
                .unwrap();

        assert_eq!(
            summarize_url(&url),
            "https://x.com/i/flow/login?redirect_after_login,state"
        );
    }
}
