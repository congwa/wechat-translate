# Changelog

All notable changes to this project will be documented in this file.

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
