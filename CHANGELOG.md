# Changelog

All notable changes to this project will be documented in this file.

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
