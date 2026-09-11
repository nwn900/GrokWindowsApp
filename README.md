# Grok Desktop

A Windows desktop app for Grok, built with Rust, Tauri 2 and Microsoft Edge WebView2.

![Grok](icon.png)

## Downloads

[Download v1.2.1](https://github.com/nwn900/GrokWindowsApp/releases/tag/v1.2.1) — Windows x64 NSIS installer, compiled locally.

## Features

- System tray, single instance, and hide on close.
- Manual launch opens the window; Windows startup uses the explicit `--autostart` argument.
- Answer-completion notifications while the main window is inactive. Click a notification to restore the app; use Test Notification in the tray menu to check Windows delivery.
- Native authentication popups and file-download dialogs.
- New transparent application and tray icons.

Completion detection observes page mutations and reads text without forcing layout. It does not continually scan an idle page. Selectors live in `src-tauri/src/notifications.js`; service redesigns can require selector updates. Native delivery status is recorded in a bounded `notifications.log` under the application's log directory.

## Build locally

Install Rust's MSVC toolchain, Visual Studio C++ Build Tools, WebView2, and the Tauri CLI (`cargo install tauri-cli --locked`). Use a Visual Studio developer shell with `CC=cl.exe` and `CXX=cl.exe`.

```powershell
cd src-tauri
cargo test --locked
cargo tauri build -- --locked
```

The installer is generated in `target/release/bundle/nsis/`. To limit disk usage, set `CARGO_TARGET_DIR` to a dedicated directory on a drive with free space, set `CARGO_INCREMENTAL=0`, and build one app at a time. Copy completed installers outside the target directory before running `cargo clean`.

Run JavaScript regression tests with `node --test tests/notifications.test.cjs`.

Releases are uploaded from local builds. Tag pushes do not run a GitHub installer build.

## Verification limits

Automated tests cover wrapper logic and simulated page signals. Signed-in provider flows, real response completion, Windows notification visibility, and rendering timings require live verification. Windows notification settings and Do Not Disturb can suppress visible banners.
