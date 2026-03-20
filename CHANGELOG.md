# Changelog

All notable changes to this project will be documented in this file.

## [0.1.4] - 2026-03-20

### Highlights

Expand the macOS Rust/Tauri app into a more complete AI-assisted WeChat learning workspace with cross-chat AI summaries, Text2SQL Agent chat, system TTS playback, tray blink alerts for monitor failures, and a refreshed README that now leads with the macOS experience.

### Added
- Add cross-chat global summary generation with multilingual output support so recent activity across all chats can be summarized in one place.
- Add a dedicated AI Agent workflow with Text2SQL chat, cancel controls, configurable multi-turn reasoning, and fallback configuration for provider/model setup.
- Add macOS system TTS playback with utterance lifecycle events, sidebar speaking-state animation, and tray menu controls.
- Add tray icon blink alerts for consecutive monitoring failures to surface listener health issues more clearly.
- Add README showcase assets for the macOS AI summary card and Text2SQL Agent experience.

### Changed
- Refresh the Rust README and root README so the repository now leads with macOS screenshots and AI feature highlights instead of the older Python/Windows-first presentation.
- Improve summary generation UX with multilingual formatting support and dedicated UI entry points for AI-driven message recap.
- Refine Agent chat behavior with better duplicate-response suppression, clearer model capability errors, and improved tool-calling flow.
- Continue restructuring the Rust/Tauri application layers, runtime snapshots, sidebar orchestration, and interface boundaries to support the newer AI, TTS, and monitoring features.

### Fixed
- Fix monitor failure visibility by turning repeated polling issues into tray blink warnings instead of silent background degradation.
- Fix Agent interaction edge cases around duplicate initialization and response handling so chat sessions behave more predictably.
- Fix several sidebar/runtime consistency issues during feature expansion by tightening snapshot loading and lifecycle coordination.

## [0.1.1] - 2026-03-09

### Highlights

Refine the macOS Rust/Tauri monitoring pipeline so group-chat self messages are identified from the left session preview instead of the right chat pane, improving sidebar sender labeling and release packaging quality.

### Added
- Add preview sender-hint cache coverage and focused regression tests for group self-message inference in the macOS Rust monitor.
- Add AX research/bin updates to inspect sender hints, side probes, and richer debug output during macOS troubleshooting.

### Changed
- Prioritize the left session preview for group sender inference and keep the right-side AX tree limited to content reading on macOS.
- Fix unread-count comparison timing in the active-chat monitor loop so preview inference uses the previous unread baseline.
- Refresh macOS sidebar/database plumbing touched by the Rust monitor pipeline and align this patch release package metadata to `0.1.1`.

### Fixed
- Fix the case where messages sent by yourself in a group chat could appear without the correct self identity because the right chat pane overrode the left preview.
- Fix preview/body mismatch handling so sender inference only overrides when the left preview actually matches the latest chat content.

## [0.1.0] - 2026-03-09

### Highlights

Initial release of the macOS native app under `rust/`, built with Tauri 2 + React 19 + Rust.

### Added
- Add a native macOS desktop app in `rust/` for WeChat automation and translation.
- Add Tauri-based packaging that produces a macOS `.app` bundle.
- Add a distributable `.dmg` package for the macOS app release.

### Changed
- Add Rust/Tauri local build artifacts and local config ignores to `rust/.gitignore`.

### Notes
- This release targets Apple Silicon (`aarch64`) macOS packaging.
