# Desktop wrapper audit — 2026-09-11

## Scope

Reviewed Rust startup, navigation, native commands, notification integration, injected JavaScript, icons, installer configuration and release automation. This is a bounded wrapper audit, not proof that every bug in the application or upstream service has been found.

## Corrections

- Replaced completion polling with a debounced page observer and textContent reads that do not force layout.
- Added origin-checked, bounded, rate-limited native notifications with click-to-restore and bounded delivery diagnostics.
- Kept native WebView2 IPC intact.
- Restricted remote pages to the one completion command through generated Tauri command permissions.
- Preserved native authentication popup handling and made authentication logging opt-in and bounded.
- Distinguished actual Windows autostart from manually opening the application.
- Restored minimized windows when opening from the tray or another instance.
- Restricted external URL launching to web/mail schemes.
- Applied the supplied transparent Desktop icon to application, tray, installer and repository artwork.
- Updated Tauri and committed reproducible dependency resolution.
- Documented sequential local builds on a dedicated drive; release assets must be preserved before cleaning generated output.
- Disabled tag-triggered installer builds and automated release publication.

## Validation and limits

Rust unit tests and Node simulated-DOM regression tests are run locally, followed by an optimized Windows x64 NSIS build. These checks cover wrapper logic and packaging, not signed-in end-to-end behavior.

No measured page-rendering speedup is claimed. The changes remove wrapper scanning/layout overhead; provider latency and WebView2 rendering still require before/after profiling. DOM selectors can drift with upstream redesigns.

Real provider authentication, streaming responses, notification banner visibility/click activation, media keys and installation upgrades still need live acceptance testing. Windows notification settings and Do Not Disturb can suppress banners. A complete dependency vulnerability audit was not performed.

## Rollback

Previous GitHub release installers remain available. Close the application through its tray menu before reinstalling the prior version if authentication, playback or notification regressions occur. Do not delete the WebView2 user profile. No profile migration or user-data cleanup is part of this release.
