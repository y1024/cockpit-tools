# Changelog

English · [简体中文](CHANGELOG.zh-CN.md)

All notable changes to Cockpit Tools will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---
## [0.8.8] - 2026-02-21

### Changed
- **Codex quota windows now follow window presence**: Codex quota rendering is now driven by `primary_window` / `secondary_window` presence instead of always forcing two fixed lines.
- **Codex window labels now use Codex-style rules**: Window labels now use unified dynamic formatting (`5h`, `Weekly`, `Xd`, `Xh`, `Xm`) based on actual window minutes.
- **Multi-instance Codex account selector now shows plan badge**: Bound-account dropdown/list in Codex instances now shows subscription badges (`FREE/PLUS/PRO/TEAM/ENTERPRISE`) alongside account emails to reduce free-plan ambiguity.
- **Manual update check now always shows feedback**: Clicking `Check Updates` now shows loading state and explicit result feedback (`up to date` / `check failed`) instead of silent no-op when no new version is found.
- **Release workflow now auto-publishes checksums**: GitHub Release pipeline now automatically generates and uploads `SHA256SUMS.txt` from release assets, removing manual checksum upload.

### Refactored
- **Shared Codex quota-window helper introduced**: Codex account page, dashboard cards, and Codex instances now reuse the same window-label/window-visibility helper to keep display logic consistent.

---
## [0.8.7] - 2026-02-21

### Changed
- **Unknown-tier rendering and filtering added**: Accounts with missing subscription tier now resolve to `UNKNOWN` (instead of falling back to `FREE`) in cards/tables, and the account filter dropdown now supports `UNKNOWN` as a dedicated option.
- **Unknown badge now uses warning styling**: `UNKNOWN` tier badges are highlighted in red to visually distinguish tier-identification anomalies from normal `FREE` accounts.
- **Quota modal badge consistency**: Quota details modal now always shows a tier badge, including `UNKNOWN` when subscription tier is unavailable.

### Fixed
- **No stale tier carry-over after refresh**: Removed backend behavior that preserved previous `subscription_tier` when the new quota payload had no tier, preventing old `PRO/ULTRA` labels from persisting incorrectly.
- **Tier-identification diagnostics improved**: Subscription identification logs now emit explicit `UNKNOWN` failure reasons (including status/body snippets and loadCodeAssist context) to distinguish API errors from successful responses without tier data.

---
## [0.8.6] - 2026-02-21

### Changed
- **Model group auto-classification now ignores version suffixes**: Added prefix/pattern matching for model families so Claude and Gemini variants are grouped by family (Pro/Flash/Image) even when exact IDs are not pre-listed.
- **"Other Models" cleanup for Claude/Gemini variants**: Claude Sonnet/Opus variants and Gemini x Pro/Flash/Pro Image variants are now routed into their target default groups instead of falling into `Other Models`.
- **Default Gemini group labels renamed**: Group display names were updated from `G3-Pro`, `G3-Flash`, `G3-Image` to `Gemini Pro`, `Gemini Flash`, `Gemini Image` for version-agnostic naming.

### Fixed
- **Legacy group-name compatibility**: Existing saved group settings with legacy `G3-*` names are automatically migrated to the new Gemini labels on load.

---
## [0.8.5] - 2026-02-19

### Added
- **Kiro account ban detection**: Automatic detection of suspended/banned Kiro accounts. When the quota refresh API returns a ban signal (e.g. 403 FORBIDDEN), the account is automatically marked as `banned` with the reason stored.

### Changed
- **Banned account UI**: Account cards and table rows now show a 🔒 `forbidden` status badge and a greyed-out card style to visually distinguish banned accounts.
- **Banned account action restrictions**: The switch button is disabled for banned accounts; the dashboard recommendation algorithm and quota alert suggestions automatically exclude banned accounts.
- **Bulk refresh skips banned accounts**: Refresh-all now skips accounts already marked as banned, reducing unnecessary API calls, and logs the skipped count.
- **Quota alert excludes banned current account**: If the currently active account is banned, quota alert checks are skipped.

### Fixed
- **Error vs. ban state separation**: Refresh failures (`error`) and account bans (`banned`) are now recorded separately, preventing all refresh errors from being misclassified as generic errors.

---
## [0.8.4] - 2026-02-19

### Changed
- **Kiro JSON import now supports raw account snapshots**: The import pipeline now accepts Kiro-style raw JSON objects (and arrays) with fields like `accessToken`, `refreshToken`, `expiresAt`, `provider`, `profileArn`, and `usageData`, then maps them into normalized local accounts.
- **Kiro import parser is unified with OAuth snapshot mapping**: JSON import now reuses the same snapshot-to-payload extraction path as OAuth/local import, improving consistency of email/user/provider/plan/quota field parsing.

### Fixed
- **Slash datetime parsing for imported expiry**: Kiro token expiry values in `YYYY/MM/DD HH:mm:ss` format (e.g. `2026/02/19 02:01:47`) are now parsed correctly during import.
- **Bonus expiry fallback coverage**: `freeTrialExpiry` is now recognized as a fallback source when deriving Kiro add-on expiry days.

---
## [0.8.3] - 2026-02-18

### Changed
- **Tray platform matrix expanded with Kiro**: Added Kiro to tray platform ordering/display and account-count aggregation, and introduced Kiro account summary rendering in tray menus (plan + prompt/add-on remaining with reset time).
- **Legacy tray layout compatibility for Kiro rollout**: When loading old four-platform tray configs, Kiro is auto-appended only for legacy default layouts while preserving user-customized visibility/order.
- **Raw plan/tier labels enforced in account pages**: Antigravity/Codex/GitHub Copilot/Windsurf account cards, tables, and filter options now show original plan/tier values directly (no localized remapping).

### Fixed
- **Auto-switch threshold boundary**: Account auto-switch trigger now fires when remaining percentage is less than or equal to threshold (`<=`) to avoid missing boundary cases at exact threshold.

---
## [0.8.2] - 2026-02-18

### Changed
- **OAuth callback server hardening**: Rewrote the local OAuth callback server to loop over incoming requests, silently ignoring non-callback requests (e.g. favicon), and only processing the actual `/oauth-callback` path. Added CORS preflight (OPTIONS) support and an explicit 404 response for unmatched routes.
- **OAuth CSRF protection**: OAuth authorization URL now includes a `state` parameter generated per flow; the callback server validates the returned state to prevent cross-site request forgery.
- **OAuth flow timeout & cleanup**: Added a configurable timeout for the OAuth callback wait; on timeout or failure the flow state is automatically cleaned up, and a user-facing retry message is returned.
- **OAuth redirect host normalization**: Changed OAuth redirect URI from `127.0.0.1` to `localhost` for broader browser/OS redirection compatibility.
- **Account identity matching overhaul**: Replaced the previous email-only account matching with a strict multi-factor identity matcher (`session_id` → `refresh_token` → `email + project_id`), plus a legacy single-email fallback for backward compatibility during upsert.
- **Google user ID persistence**: OAuth `UserInfo` now parses and stores the Google `id` field, writing it into account data on login completion.

---
## [0.8.1] - 2026-02-17

### Changed
- **Plan/Tier labels now use raw values**: Account-card and table badges across Antigravity/Codex/GitHub Copilot/Windsurf/Kiro now display original backend/local plan values directly, while keeping existing style mapping.
- **Overview tabs use fixed default labels**: Platform overview tabs (`Account Overview` / `Multi-Instance`) now render default text directly to avoid cross-locale mismatch from platform-specific translation overrides.
- **Platform names are fixed to source labels**: Shared platform label rendering now always shows original platform names (`Antigravity`, `Codex`, `GitHub Copilot`, `Windsurf`, `Kiro`).
- **Codex switch behavior is configurable**: Added `codex_launch_on_switch` to backend/user config and wired it into Settings and Quick Settings so switching Codex can optionally skip auto launch/restart.

### Fixed
- **Dashboard privacy consistency**: Dashboard account emails are now masked by the same privacy toggle used in account/instance pages, with focus/visibility/storage sync to keep masking state consistent.
- **OpenCode switch-token sync reliability**: Fixed a regression where GPT account switching did not effectively replace OpenCode login credentials in runtime scenarios, causing the app session to stay on the previous account. (#51)
- **Dashboard card layout balance**: Fixed the Antigravity account card width behavior to avoid obvious right-side whitespace in dashboard layouts and improve visual balance. (#49)

---
## [0.8.0] - 2026-02-17

### Added
- **Fifth platform is live**: Kiro officially joins the supported platform lineup with unified management alongside Antigravity, Codex, GitHub Copilot, and Windsurf.
- **Core Kiro flows are now available**: OAuth/Token/JSON/local import, account switching, quota refresh, multi-instance lifecycle, and app path configuration are all included.
- **Platform-layer refactor**: Instance services, account stores, and overview tabs were unified into reusable platform abstractions to reduce future integration cost.
- **Key fixes in this release**: Hardened Kiro import ID validation against path traversal and filled missing locale keys to reduce mixed-language fallback in non-default locales.

---
## [0.7.3] - 2026-02-15

### Added
- **Tray platform layout persistence**: Added backend tray layout config storage (`tray_layout.json`) and command `save_tray_platform_layout` to save tray visibility, order, and sort mode.
- **Tray visibility control in layout modal**: Added `Show in tray` toggle in platform layout management and synchronized the related locale key across supported languages.
- **Expanded tray platform coverage**: Added GitHub Copilot and Windsurf tray submenus with account/quota summary and direct navigation targets.

### Changed
- **Tray menu architecture**: Refactored tray menu generation to dynamic multi-platform rendering, supporting auto/manual ordering and overflow grouping (`More platforms`).
- **Tray refresh trigger points**: GitHub Copilot/Windsurf refresh, OAuth completion, token import, and account switch flows now refresh tray content immediately; language changes also trigger tray rebuild.
- **Frontend tray event handling**: `tray:refresh_quota` now refreshes Antigravity, Codex, GitHub Copilot, and Windsurf in one flow; tray navigation now recognizes `github-copilot` and `windsurf`.
- **Platform layout sync strategy**: Added debounced frontend-to-backend tray layout sync on reorder/visibility changes and on initial app load.

### Fixed
- **Tray visibility filtering correctness**: Fixed tray platform filtering so disabled platforms remain hidden and the empty-state item appears when no tray platform is selected.
- **Log privacy hardening**: Logger now masks email addresses in log messages to reduce exposure of sensitive identifiers.

---
## [0.7.2] - 2026-02-14

### Added
- **Group management "Other Models" bucket**: Added an auto-collected `Other Models` group that lists non-default models discovered from account quotas.
- **Auth-mode model blacklist filtering**: Added blacklist filtering by model ID/display name in group management to exclude blocked Gemini 2.5/chat variants.
- **Claude Opus 4.6 mapping coverage**: Added `claude-opus-4-6-thinking` and `MODEL_PLACEHOLDER_M26` to default/recommended model mappings and wakeup recommendations.

### Changed
- **Group settings migration behavior**: Loading group settings now incrementally backfills missing default mappings/names/order while preserving user custom configuration.
- **Group modal data source**: Accounts page now passes live model IDs/display names into group management, improving model label display and "other model" classification accuracy.
- **Model display order alignment**: Account quota display order was aligned to include Claude Opus 4.6 under the Claude group.

### Fixed
- **Locale completeness for group settings**: Synchronized `group_settings.other_group` across all supported locales to avoid missing-key fallbacks.

---
## [0.7.1] - 2026-02-14

### Added
- **Cross-platform quota alert workflow**: Added quota alert calculations and event dispatch for Antigravity, Codex, GitHub Copilot, and Windsurf, with per-platform model/credit metric detection.
- **Quota alert settings surface**: Added quota alert enable/threshold controls in both Settings and Quick Settings, with synchronized i18n keys.
- **Global modal infrastructure**: Added reusable global modal store/hook/component (`useGlobalModal` + Zustand + `GlobalModal`) for cross-module prompts and alert actions.
- **Notification capability integration**: Integrated Tauri notification capability in app runtime/capabilities with macOS click-to-focus handling.

### Changed
- **Quota refresh behavior**: Refresh-all and refresh-current flows for Codex/GitHub Copilot/Windsurf now trigger quota-alert checks after successful quota/token refresh.
- **Alert payload model**: `quota:alert` payload now carries `platform`, and the frontend modal quick-switch action now routes to the correct platform switch flow/page.
- **Settings input interaction**: Refresh interval and threshold controls now use preset + inline numeric input mode with Enter/blur apply behavior.
- **Config model propagation**: `quota_alert_enabled` and `quota_alert_threshold` are now persisted through command save/get and websocket language-save paths.
- **Log retention policy**: Logger initialization now cleans up expired `app.log*` files older than 3 days.

### Fixed
- **Quota alert listener lifecycle**: Prevented duplicate `quota:alert` subscriptions caused by async unlisten timing in React effect cleanup.
- **Threshold consistency at 0%**: Runtime threshold normalization now honors `0%`, matching frontend options and user expectations.

---
## [0.7.0] - 2026-02-12

### Added
- **Full Windsurf platform integration**: Added Windsurf account system end-to-end, including OAuth/Token/Local import, account persistence, quota sync, switch/inject/start flow, and multi-instance commands.
- **Windsurf frontend modules**: Added Windsurf account page, instance page, service/store/type layers, and dedicated icon/navigation assets.
- **Dashboard support for Windsurf**: Added Windsurf statistics and overview cards with quick refresh/switch actions, aligned with existing platform cards.
- **Platform layout capability**: Added layout management modal and platform layout store for platform ordering/visibility management in navigation.

### Changed
- **Navigation structure expansion**: Side navigation and routing were extended to include Windsurf and platform-layout entry points.
- **Settings model extension**: General settings now include Windsurf auto-refresh and app-path controls, plus corresponding quick-settings behavior.
- **Windows path detection pipeline**: App detection was upgraded with stronger multi-source probing, PowerShell `-File` fallback, and VS Code registry probing.

### Fixed
- **Path detection reliability on Windows**: Improved handling for empty/error-prone command output and reduced false-miss cases during VS Code/Windsurf path discovery.
- **Quota refresh fallback behavior**: Failed refresh now preserves the last valid quota snapshot to avoid clearing displayed quota to zeros.
- **Switch/injection robustness**: Improved handling and diagnostics around account binding and startup path mismatch cases.

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
