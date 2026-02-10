# Changelog

English · [简体中文](CHANGELOG.zh-CN.md)

All notable changes to Cockpit Tools will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---
## [0.6.10] - 2026-02-10

### Added
- **Privacy mode for screenshots**: Added Eye/EyeOff toggle and masking for email-like identifiers in Antigravity/Codex/GitHub Copilot account overviews and instance pages.
- **GitHub Copilot one-click switching pipeline**: Added default-profile VS Code switching path with token injection and restart integration.
- **Cross-instance window focus/open support**: Added and localized `openWindow` action and improved focus behavior by PID for Antigravity/Codex/VS Code instances.
- **Quota/switch diagnostics**: Added richer runtime logs and metadata outputs for refresh/switch troubleshooting.
- **Codex multi-team identity support**: Added account matching based on `account_id`/`organization_id` to support multi-team scenarios.
- **macOS distribution postflight hook**: Added Cask postflight logic to auto-remove quarantine attributes.
- **Release process templates/scripts**: Added release checklist/docs and helper scripts for preflight validation and checksum generation.

### Changed
- **Unified switch flow (overview -> default instance)**: Antigravity/Codex/GitHub Copilot overview switching now follows default-instance startup logic (PID-targeted close -> inject -> start).
- **GitHub Copilot flow alignment**: Overview switching and multi-instance startup now share the same injection/start semantics.
- **Instance lifecycle alignment**: Unified start/stop/close behavior across Antigravity/Codex/VS Code with managed-directory matching and PID tracking.
- **Windows VS Code launch strategy**: Switched to `cmd /C code` for `.cmd` wrapper compatibility.
- **PID resolution semantics alignment**: VS Code PID resolving/focus now uses `Option<&str>` semantics (`None` => default instance), matching Antigravity behavior and reducing default-instance mismatch edge cases.
- **Docs and settings guidance**: Updated README/security/settings guidance for new switching and path behaviors.
- **Localization synchronization**: Updated locale keys across all supported languages for Copilot switching, open-window action, privacy mode, and related error messages.

### Fixed
- **Error compatibility and messaging**: Improved non-success status handling paths and user-facing error propagation for refresh/switch operations.
- **PR review follow-ups**: Improved error handling, added SQLite transaction safeguards in injection flow, and fixed branding inconsistencies.
- **Build hygiene**: Cleaned Windows-specific warnings and removed/quieted stale dead-code warnings.

### Removed
- **Deprecated Copilot injection entrypoint**: Removed unused legacy wrapper in favor of the unified instance-based switching pipeline.

---
## [0.6.0] - 2026-02-08

### Added
- **GitHub Copilot account management**: OAuth/Token/JSON import, quota status, plan badges, tags, batch actions, and account overview UI.
- **GitHub Copilot multi-instance**: Manage VS Code Copilot instances with isolated profiles, settings, and lifecycle actions.

### Changed
- **Dashboard & navigation**: Added GitHub Copilot entry and overview panel alongside Antigravity/Codex.
- **App-path behavior**: Rolled back the recent app-path re-detect changes to restore the previous detection flow.

### Fixed
- **Windows build warnings**: Tightened platform-specific process helpers and avoided moved environment values.

---
## [0.5.4] - 2026-02-07

### Added
- **Codex OAuth login session API**: Added command set `codex_oauth_login_start` / `codex_oauth_login_completed` / `codex_oauth_login_cancel` with `loginId + authUrl` response model.
- **OAuth timeout event contract**: Added backend timeout event payload (`loginId`, `callbackUrl`, `timeoutSeconds`) for frontend-driven retry UX.

### Changed
- **Codex OAuth flow alignment**: Switched from code-push completion to login-session completion (backend stores callback code by session, frontend completes by `loginId`).
- **UI authorization flow**: OAuth link is prepared and shown in modal first; browser open remains explicit user action.
- **Timeout retry UX**: On timeout, the main OAuth CTA switches to `Refresh authorization link`; after refresh succeeds, it switches back to `Open in Browser`.
- **Timeout behavior**: Timeout no longer triggers automatic authorization re-creation loops; retry is user-triggered.
- **OAuth observability**: Refined OAuth logs to concise operational checkpoints (session creation/start/timeout/cancel/complete), removing verbose full-payload noise.

### Removed
- **Legacy Codex OAuth commands**: Removed `prepare_codex_oauth_url`, `complete_codex_oauth`, `cancel_codex_oauth` and related frontend/service fallback paths.

### Fixed
- **Duplicate callback completion risk**: Hardened frontend callback handling with session and in-flight guards to reduce duplicate-complete races.
- **OAuth timeout UI duplication**: Resolved repeated timeout error presentation in modal by consolidating timeout-state rendering.

---
## [0.5.3] - 2026-02-06

### Added
- **Blank instance initialization mode**: Added a new initialization option when creating instances (`Copy source instance` / `Blank instance`) so users can create an empty directory without copying profile data.
- **Uninitialized-instance guide modal**: Clicking account binding on an uninitialized blank instance now opens a guide modal with a **Start now** action.
- **Instance sorting controls**: Added sort field selection (`Creation time` / `Launch time`) and ascending/descending toggle in the multi-instance toolbar.
- **In-app delete confirmation modal**: Instance deletion now uses an internal modal (with top-right close action) instead of relying on the system dialog.

### Changed
- **Instance status model**: Added `initialized` to Antigravity/Codex instance view payloads and wired it through frontend state.
- **Binding safety checks**: Binding is now blocked for uninitialized instances (disabled UI + backend validation with explicit error).
- **Instance list layout**: Status is shown in a dedicated column next to instance name; actions column is now sticky/opaque so it stays visible on narrow windows without content bleed-through.
- **Dropdown rendering split**: Inline list account dropdown renders via portal (outside container), while modal dropdown keeps in-container rendering to avoid clipping and style conflicts.
- **PID visibility rule**: PID is hidden when an instance is not running.
- **Post-start delayed refresh**: Added delayed refresh (~2s) after start to reduce stale `pending initialization` state after first boot.
- **i18n alignment**: Added and synchronized new instance-flow keys across all 17 locale files.

### Fixed
- **Delete-confirm freeze**: Fixed a scenario where delete confirmation actions could become unresponsive.

---
## [0.5.2] - 2026-02-06

### Changed
- **Account switch binding sync**: When switching Antigravity account, default instance binding now updates automatically to the selected account.
- **Codex account switch binding sync**: When switching Codex account, default Codex instance binding now updates automatically to the selected account.
- **Instance account dropdown interaction**: Inline account dropdown now uses unified open-state control so only one instance dropdown is open at a time.
- **Instances page UI polish**: Refined list/table layout, inline account selector readability, and dark mode/responsive presentation.

## [0.5.1] - 2026-02-05

### Added
- **Wakeup scheduler backend sync**: Added scheduler sync command and backend-side history load/clear APIs.
- **Download directory helper**: Exposed a system API to resolve the downloads directory.
- **App path management**: Added Codex app path to general settings and introduced app-path detect/set commands.

### Changed
- **Wakeup history storage**: Moved history persistence to backend storage with higher retention (up to 100 items).
- **macOS launch strategy**: Prefer direct executable launch (PID available), fallback to `open -a` for `.app` paths.
- **App path reset**: Reset now auto-detects and fills the path instead of clearing it.
- **Account switching**: Update default instance PID after launch; emit app-path-missing events when needed.
- **Documentation**: Added multi-instance sections and image placeholders for Antigravity/Codex.
- **i18n**: Added new app-path related keys and ensured locale consistency.

### Fixed
- **macOS app selection**: Improved `.app` selection/launch flow to reduce permission errors.

## [0.5.0] - 2026-02-04

### Added
- **Antigravity Latest Version Compatibility**: Enhanced account switching support for Antigravity 1.16.5+.
  - Support for new unified state sync format (`antigravityUnifiedStateSync.oauthToken`).
  - Backward compatible with legacy format for older versions.
- **Antigravity Multi-Instance Support**: Run multiple Antigravity IDE instances simultaneously.
  - Each instance runs with an isolated user profile and data directory.
  - Support for different accounts logged in to different instances concurrently.
  - Create, launch, restart, and delete instances with a dedicated management interface.
  - Auto-detect running instances and display their status in real-time.
- **Codex Desktop Multi-Instance Support**: Run multiple Codex desktop instances simultaneously on macOS.
  - Each instance runs with an isolated user profile and app data directory.
  - Support for different accounts logged in to different instances concurrently.
  - Create, launch, restart, and delete instances with a dedicated management interface.
  - Auto-detect running instances and display their status in real-time.
  - Smart restart strategy: choose between "Always Restart", "Never Restart", or "Ask Me" when switching accounts.

### Changed
- **Instance Management UI**: New dedicated instance management page with modern list-based interface.
- **Navigation**: Added "Instances" menu item to sidebar for quick access to instance management.

---
## [0.4.10] - 2026-01-31

### Changed
- **Single account quota refresh**: Single card refresh now always fetches from the real-time API, bypassing the 60-second cache.
- **Cache directory isolation**: Desktop quota cache moved to `quota_api_v1_desktop` to prevent sharing/overwriting with the extension.

## [0.4.9] - 2026-01-31

### Added
- **Quota error details**: Store the last quota error per account and show it in a dedicated error details modal (with link rendering).
- **Forbidden status UI**: Show 403 forbidden status with a lock badge and an in-place quota banner.

### Changed
- **Quota fetch results**: Return structured error info (code/message) and persist it into account state.
- **Account status hints**: Combine disabled/warning/forbidden hints in tooltips.
- **Account actions UI**: Tightened action button spacing and size for account cards.

### Fixed
- **i18n**: Filled missing translations for account error actions and error detail fields.

## [0.4.8] - 2026-01-30

### Added
- **OpenCode sync toggle**: Add a switch in Codex account management to control OpenCode sync/restart.

### Changed
- **OpenCode auth sync**: Sync OpenCode auth.json on account switch with full OAuth fields and platform-aware path.
- **OpenCode restart**: Start OpenCode when not running; restart when running.
- **AccountId alignment**: Align account_id extraction with the official extension (access_token only).
- **UI copy**: Settings OpenCode path hint now generic without a hardcoded default path.

### Fixed
- **i18n**: Filled missing translations and ensured locale keys are consistent across languages.

## [0.4.7] - 2026-01-30

### Added
- **Authorized API cache**: Cache raw authorized API responses in `cache/quota_api_v1`.
- **Cache source marker**: Store `customSource` in API cache records to identify the writer.
- **Cache hit logging**: Log API cache hits/expiry during quota refresh.

### Changed
- **Legacy cache reader**: Reads the new API cache payload to preserve fast startup behavior.

## [0.4.6] - 2026-01-29

### Added
- **Update Notification**: Update dialog now displays release notes with localized content (English/Chinese).

### Fixed
- **i18n**: Fixed missing translations in Codex add account modal (OAuth, Token, Import tabs).
- **Accessibility**: Improved FREE tier badge contrast for better readability in light mode.
- **i18n**: Fixed hardcoded Chinese strings in tag deletion confirmation dialog.

---
## [0.4.3] - 2026-01-29

### Added
- **Codex Tag Management**: Added global tag deletion for Codex accounts.
- **Account Filtering & Tagging**:
  - Support for managing account tags (add/remove).
  - Support for filtering accounts by tags.
- **Compact View**:
  - Added compact view mode for account list.
  - Added status icons for disabled or warning states in compact view.
  - Support customizable model grouping in compact view.

### Changed
- **Smart Recommendations**: Improved dashboard recommendation logic to exclude disabled, forbidden, or empty accounts.
- **UI Improvements**:
  - Refined compact view interactions.
  - Removed redundant tag rendering in list views.
  
## [0.4.2] - 2026-01-29

### Added
- **Update Modal**: Unified update check into a modal dialog, including the entry in Settings → About.
- **Refresh Frequency**: Added Codex auto refresh interval settings (default 10 minutes).
- **Account Warnings**: Show refresh warnings in the account list, including invalid-credential hints.

### Changed
- **Update UX**: Update prompt now uses a non-transparent modal consistent with existing dialogs.

## [0.4.1] - 2026-01-29

### Added
- **Close Confirmation**: New close dialog with minimize/quit actions and a “remember choice” option.
- **Close Behavior Setting**: Configure the default close action in Settings → General.
- **Tray Menu**: System tray menu with navigation shortcuts and quota refresh actions.
- **Sorting Enhancements**: Sort by reset time for Antigravity group quotas and Codex weekly/hourly quotas.

### Changed
- **i18n**: Updated translations for close dialog, close behavior, and reset-time sorting across all 17 languages.
- **UI Polish**: Refined styling to support the new close dialog and related layout updates.


## [0.4.0] - 2026-01-28

### Added
- **Visual Dashboard**: Brand new dashboard providing a one-stop overview of both Antigravity and Codex accounts status.
- **Codex Support**: Full support for Codex account management.
  - View Hourly (5H) and Weekly quotas.
  - Automatic Plan recognition (Basic, Plus, Team, Enterprise).
  - Independent account list and card view.
- **Rebranding**: Project officially renamed to **Cockpit Tools**.
- **Sponsor & Feedback**: Added "Sponsor" and "Feedback" sections in Settings -> About for better community engagement.

### Changed
- **UI Overhaul**: Redesigned dashboard cards for extreme compactness and symmetry.
- **Typography**: switched default font to **Inter** for better readability.
- **Documentation**: Comprehensive update to README with fresh screenshots and structured feature overview.
- **i18n**: Updated translations for all 17 languages to cover new Dashboard and Codex features.


## [0.3.3] - 2026-01-24

### Added
- **Account Management**: Added sorting by creation time. Accounts are now sorted by creation time (descending) by default.
- **Database**: Added `created_at` field to the `accounts` table for precise account tracking.
- **i18n**: Added "Creation Time" related translations for all 17 supported languages.

## [0.3.2] - 2026-01-23

### Added
- **Engineering**: Added automatic version synchronization script. `package.json` version now automatically syncs to `tauri.conf.json` and `Cargo.toml`.
- **Engineering**: Added git pre-commit hook to strictly enforce Changelog updates when version changes.

## [0.3.1] - 2026-01-23

### Changed
- **Maintenance**: Routine version update and dependency maintenance.

## [0.3.0] - 2026-01-22

### Added
- **Model Grouping Management**: New grouping modal to customize model group display names.
  - Four fixed groups: Claude 4.5, G3-Pro, G3-Flash, G3-Image.
  - Custom group names are applied to account cards and sorting dropdowns.
  - Group settings are persisted locally and auto-initialized on first launch.
- **Account Sorting**: Added sorting options for account list.
  - Default sorting by overall quota.
  - Sort by specific group quota (e.g., by Claude 4.5 quota).
  - Secondary sorting by overall quota when group quotas are equal.
- **i18n**: Added sorting and group management translations for all 17 supported languages.

### Changed
- Model names on account cards now dynamically reflect custom group names.
- Removed "Other" group display to simplify the grouping model.
- Decoupled grouping configuration between desktop app and VS Code extension.

---

## [0.2.0] - 2026-01-21

### Added
- **Update Checker**: Implemented automatic update checking via GitHub Releases API.
  - On startup, the app checks for new versions (once every 24 hours by default).
  - A beautiful glassmorphism notification card appears in the top-right corner when an update is available.
  - Manual "Check for Updates" button added to **Settings → About** page with real-time status feedback.
  - Clicking the notification opens the GitHub release page for download.
- **i18n**: Added update notification translations for all 17 supported languages.

---

## [0.1.0] - 2025-01-21

### Added
- **Account Management**: Complete account management with OAuth authorization support.
  - Add accounts via Google OAuth authorization flow.
  - Import accounts from Antigravity Tools (`~/.antigravity_tools/`), local Antigravity client, or VS Code extension.
  - Export accounts to JSON for backup and migration.
  - Delete single or multiple accounts with confirmation.
  - Drag-and-drop reordering of account list.
- **Quota Monitoring**: Real-time monitoring of model quotas for all accounts.
  - Card view and list view display modes.
  - Filter accounts by subscription tier (PRO/ULTRA/FREE).
  - Auto-refresh with configurable intervals (2/5/10/15 minutes or disabled).
  - Quick switch between accounts with one click.
- **Device Fingerprints**: Comprehensive device fingerprint management.
  - Generate new fingerprints with customizable names.
  - Capture current device fingerprint.
  - Bind fingerprints to accounts for device simulation.
  - Import fingerprints from Antigravity Tools or JSON files.
  - Preview fingerprint profile details.
- **Wakeup Tasks**: Automated account wakeup scheduling system.
  - Create multiple wakeup tasks with independent controls.
  - Supports scheduled, Crontab, and quota-reset trigger modes.
  - Multi-model and multi-account selection.
  - Custom wakeup prompts and max token limits.
  - Trigger history with detailed logs.
  - Global wakeup toggle for quick enable/disable.
- **Antigravity Cockpit Integration**: Deep integration with the VS Code extension.
  - WebSocket server for bidirectional communication.
  - Remote account switching from the extension.
  - Account import/export synchronization.
- **Settings**: Comprehensive application settings.
  - Language selection (17 languages supported).
  - Theme switching (Light/Dark/System).
  - WebSocket service configuration with custom port support.
  - Data and fingerprint directory shortcuts.
- **i18n**: Full internationalization support for 17 languages.
  - 🇨🇳 简体中文, 🇹🇼 繁體中文, 🇺🇸 English
  - 🇯🇵 日本語, 🇰🇷 한국어, 🇻🇳 Tiếng Việt
  - 🇩🇪 Deutsch, 🇫🇷 Français, 🇪🇸 Español, 🇮🇹 Italiano, 🇵🇹 Português
  - 🇷🇺 Русский, 🇹🇷 Türkçe, 🇵🇱 Polski, 🇨🇿 Čeština, 🇸🇦 العربية
- **UI/UX**: Modern, polished user interface.
  - Glassmorphism design with smooth animations.
  - Responsive sidebar navigation.
  - Dark mode support with seamless theme transitions.
  - Native macOS window controls and drag region.

### Technical
- Built with Tauri 2.0 + React + TypeScript.
- SQLite database for local data persistence.
- Secure credential storage using system keychain.
- Cross-platform support (macOS primary, Windows/Linux planned).
