use crate::models::claude::{
    ClaudeAccount, ClaudeAccountIndex, ClaudeAuthMode, ClaudeDesktopLoginStartResponse,
    ClaudeOAuthStartResponse, ClaudeQuota, ClaudeQuotaErrorInfo,
};
use crate::modules::{account, atomic_write, logger};
#[cfg(target_os = "macos")]
use aes::Aes128;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
#[cfg(target_os = "macos")]
use cbc::cipher::block_padding::Pkcs7;
#[cfg(target_os = "macos")]
use cbc::cipher::{BlockDecryptMut, KeyIvInit};
#[cfg(target_os = "macos")]
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(target_os = "macos")]
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use url::{form_urlencoded, Url};

const ACCOUNTS_INDEX_FILE: &str = "claude_accounts.json";
const ACCOUNTS_DIR: &str = "claude_accounts";
const CLAUDE_OAUTH_AUTHORIZE_URL: &str = "https://claude.com/cai/oauth/authorize";
const CLAUDE_OAUTH_MANUAL_REDIRECT_URL: &str = "https://platform.claude.com/oauth/code/callback";
const CLAUDE_OAUTH_TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CLAUDE_OAUTH_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CLAUDE_OAUTH_PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";
const CLAUDE_OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const CLAUDE_OAUTH_BETA_HEADER: &str = "oauth-2025-04-20";
const CLAUDE_TOKEN_EXPIRY_BUFFER_MS: i64 = 5 * 60 * 1000;
const CLAUDE_OAUTH_TIMEOUT_SECONDS: i64 = 600;
const CLAUDE_OAUTH_STATE_FILE: &str = "claude_oauth_pending.json";
const CLAUDE_CODE_CREDENTIALS_FILE: &str = ".credentials.json";
const CLAUDE_CODE_CONFIG_FILE: &str = ".config.json";
const CLAUDE_CODE_GLOBAL_CONFIG_FILE: &str = ".claude.json";
const CLAUDE_CODE_SETTINGS_FILE: &str = "settings.json";
const CLAUDE_CODE_SETTINGS_MANAGED_ENV_KEYS_FILE: &str =
    "claude_cli_settings_managed_env_keys.json";
const CLAUDE_CODE_KEYCHAIN_SERVICE_PREFIX: &str = "Claude Code";
const CLAUDE_CODE_KEYCHAIN_CREDENTIALS_SUFFIX: &str = "-credentials";
const CLAUDE_CODE_API_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
    "CLAUDE_CODE_ATTRIBUTION_HEADER",
];
const CLAUDE_OAUTH_SCOPES: [&str; 6] = [
    "org:create_api_key",
    "user:profile",
    "user:inference",
    "user:sessions:claude_code",
    "user:mcp_servers",
    "user:file_upload",
];
const CLAUDE_DESKTOP_LOGIN_STATE_FILE: &str = "claude_desktop_login_pending.json";
const CLAUDE_DESKTOP_PROFILES_DIR: &str = "claude_desktop_profiles";
const CLAUDE_DESKTOP_LOGIN_DIR: &str = "claude_desktop_login";
const CLAUDE_DESKTOP_BACKUPS_DIR: &str = "claude_desktop_backups";
const CLAUDE_DESKTOP_AUTH_HELPER_SCRIPT: &str = "scripts/claude-desktop-auth-helper.cjs";
const CLAUDE_DESKTOP_AUTH_STATUS_FILE: &str = "claude_desktop_auth_status.json";
const CLAUDE_DESKTOP_AUTH_EXPORT_FILE: &str = "claude_desktop_auth_export.json";
const CLAUDE_DESKTOP_COOKIE_EXPORT_FILE: &str = "claude_desktop_cookie_probe_cookies.json";
const CLAUDE_DESKTOP_BUNDLE_ID_MACOS: &str = "com.anthropic.claudefordesktop";
const CLAUDE_DESKTOP_LOGIN_TIMEOUT_SECONDS: i64 = 30 * 60;
const CLAUDE_DESKTOP_AUTH_EXPORT_WAIT_SECONDS: u64 = 8;
const CLAUDE_DESKTOP_REQUIRED_COOKIE_NAMES: &[&str] = &["sessionKey", "lastActiveOrg"];
const CLAUDE_DESKTOP_LOCAL_PROFILE_MAX_FILES: usize = 600;
const CLAUDE_DESKTOP_LOCAL_PROFILE_MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
const CLAUDE_DESKTOP_LOCAL_PROFILE_SCAN_DIRS: &[&str] = &[
    "IndexedDB",
    "Local Storage",
    "Session Storage",
    "Cache/Cache_Data",
];
const CLAUDE_DESKTOP_PROFILE_ITEMS: &[&str] = &[
    "Cookies",
    "Cookies-journal",
    "Local State",
    "Preferences",
    "Network Persistent State",
    "DIPS",
    "DIPS-wal",
    "Trust Tokens",
    "Trust Tokens-journal",
    "SharedStorage",
    "SharedStorage-wal",
    "WebStorage",
    "Local Storage",
    "IndexedDB",
    "Session Storage",
    "Service Worker",
    "ant-did",
    "config.json",
    "claude_desktop_config.json",
];
static CLAUDE_ACCOUNT_INDEX_LOCK: std::sync::LazyLock<Mutex<()>> =
    std::sync::LazyLock::new(|| Mutex::new(()));
static CLAUDE_PENDING_OAUTH_LOGIN: std::sync::LazyLock<Mutex<Option<PendingClaudeOAuthState>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));
static CLAUDE_PENDING_DESKTOP_LOGIN: std::sync::LazyLock<
    Mutex<Option<PendingClaudeDesktopLoginState>>,
> = std::sync::LazyLock::new(|| Mutex::new(None));
static EMAIL_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?i)[a-z0-9._%+\-]{1,64}@[a-z0-9.\-]{2,253}\.[a-z]{2,24}")
        .expect("valid email regex")
});
static UUID_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?i)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
        .expect("valid uuid regex")
});
#[cfg(target_os = "macos")]
type Aes128CbcDec = cbc::Decryptor<Aes128>;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingClaudeOAuthState {
    login_id: String,
    state: String,
    code_verifier: String,
    auth_url: String,
    expires_at: i64,
    cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingClaudeDesktopLoginState {
    login_id: String,
    user_data_dir: PathBuf,
    #[serde(default)]
    status_file: PathBuf,
    #[serde(default)]
    export_file: PathBuf,
    #[serde(default)]
    helper_pid: Option<u32>,
    expires_at: i64,
    cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClaudeDesktopAuthCookieExport {
    cookies: Vec<ClaudeDesktopAuthCookie>,
    #[serde(default, rename = "webProfile")]
    web_profile: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClaudeDesktopAuthCookie {
    name: String,
    value: String,
    domain: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    secure: bool,
    #[serde(default, rename = "httpOnly")]
    http_only: bool,
    #[serde(default, rename = "expirationDate")]
    expiration_date: Option<f64>,
}

#[derive(Debug, Clone)]
struct ClaudeDesktopProfileMetadata {
    source: String,
    has_session_key: bool,
    has_last_active_org: bool,
    last_active_org: Option<String>,
    session_expires_at: Option<i64>,
    cookie_names: Vec<String>,
    web_profile: Option<Value>,
}

#[derive(Debug, Clone, Default)]
struct ClaudeDesktopLocalProfile {
    email: Option<String>,
    account_uuid: Option<String>,
    full_name: Option<String>,
    display_name: Option<String>,
    organization_uuid: Option<String>,
    organization_name: Option<String>,
    source: Option<String>,
}

impl ClaudeDesktopLocalProfile {
    fn score(&self) -> i32 {
        let mut score = 0;
        if self.email.is_some() {
            score += 100;
        }
        if self.account_uuid.is_some() {
            score += 20;
        }
        if self.organization_uuid.is_some() {
            score += 10;
        }
        if self.organization_name.is_some() {
            score += 5;
        }
        if self.display_name.is_some() || self.full_name.is_some() {
            score += 3;
        }
        score
    }

    fn has_identity(&self) -> bool {
        self.email.is_some()
            || self.account_uuid.is_some()
            || self.organization_uuid.is_some()
            || self.organization_name.is_some()
    }
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

fn now_ts_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn normalize_non_empty(value: Option<&str>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        (!trimmed.is_empty()).then_some(trimmed.to_string())
    })
}

fn generate_random_url_token(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len.max(16)];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn generate_pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn read_string_path(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    normalize_non_empty(current.as_str())
}

fn read_i64_value(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Number(number)) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|v| v as i64)),
        Some(Value::String(text)) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn read_f64_value(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(text)) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn read_bool_value(value: Option<&Value>) -> Option<bool> {
    match value {
        Some(Value::Bool(value)) => Some(*value),
        Some(Value::String(text)) => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn parse_reset_seconds(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Number(number)) => {
            let raw = number
                .as_i64()
                .or_else(|| number.as_f64().map(|v| v as i64))?;
            if raw <= 0 {
                None
            } else if raw > 10_000_000_000 {
                Some(raw / 1000)
            } else {
                Some(raw)
            }
        }
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return None;
            }
            if let Ok(raw) = trimmed.parse::<i64>() {
                return if raw > 10_000_000_000 {
                    Some(raw / 1000)
                } else {
                    Some(raw)
                };
            }
            chrono::DateTime::parse_from_rfc3339(trimmed)
                .ok()
                .map(|dt| dt.timestamp())
        }
        _ => None,
    }
}

fn clamp_percentage(value: Option<f64>) -> i32 {
    let raw = value.unwrap_or(0.0);
    if !raw.is_finite() {
        return 0;
    }
    raw.round().clamp(0.0, 100.0) as i32
}

fn get_data_dir() -> Result<PathBuf, String> {
    account::get_data_dir()
}

fn get_accounts_dir() -> Result<PathBuf, String> {
    let dir = get_data_dir()?.join(ACCOUNTS_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("创建 Claude 账号目录失败: {}", e))?;
    Ok(dir)
}

fn get_accounts_index_path() -> Result<PathBuf, String> {
    Ok(get_data_dir()?.join(ACCOUNTS_INDEX_FILE))
}

pub fn accounts_index_path_string() -> Result<String, String> {
    Ok(get_accounts_index_path()?.to_string_lossy().to_string())
}

fn account_file_path(account_id: &str) -> Result<PathBuf, String> {
    Ok(get_accounts_dir()?.join(format!("{}.json", account_id)))
}

fn load_index() -> Result<ClaudeAccountIndex, String> {
    let path = get_accounts_index_path()?;
    if !path.exists() {
        return Ok(ClaudeAccountIndex::new());
    }
    let content =
        fs::read_to_string(&path).map_err(|e| format!("读取 Claude 账号索引失败: {}", e))?;
    if content.trim().is_empty() {
        return Ok(ClaudeAccountIndex::new());
    }
    atomic_write::parse_json_with_auto_restore::<ClaudeAccountIndex>(&path, &content)
        .map_err(|e| format!("解析 Claude 账号索引失败: {}", e))
}

fn save_index(index: &ClaudeAccountIndex) -> Result<(), String> {
    let path = get_accounts_index_path()?;
    let content = serde_json::to_string_pretty(index)
        .map_err(|e| format!("序列化 Claude 账号索引失败: {}", e))?;
    atomic_write::write_string_atomic(&path, &content)
}

fn write_account_file(account: &ClaudeAccount) -> Result<(), String> {
    let path = account_file_path(&account.id)?;
    let content = serde_json::to_string_pretty(account)
        .map_err(|e| format!("序列化 Claude 账号失败: {}", e))?;
    atomic_write::write_string_atomic(&path, &content)
}

fn load_account_file(account_id: &str) -> Option<ClaudeAccount> {
    let path = account_file_path(account_id).ok()?;
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn load_account(account_id: &str) -> Option<ClaudeAccount> {
    load_account_file(account_id)
}

pub fn list_accounts() -> Vec<ClaudeAccount> {
    list_accounts_checked().unwrap_or_default()
}

fn normalized_account_uuid(account: &ClaudeAccount) -> Option<String> {
    account
        .account_uuid
        .as_deref()
        .and_then(|value| normalize_non_empty(Some(value)))
        .map(|value| value.to_ascii_lowercase())
}

fn normalized_account_email(account: &ClaudeAccount) -> Option<String> {
    normalize_non_empty(Some(account.email.as_str()))
        .filter(|value| value.contains('@'))
        .map(|value| value.to_ascii_lowercase())
}

fn is_real_email(value: &str) -> bool {
    value
        .split_once('@')
        .map(|(_, domain)| domain.contains('.'))
        .unwrap_or(false)
}

fn desktop_accounts_same_identity(a: &ClaudeAccount, b: &ClaudeAccount) -> bool {
    if a.auth_mode != ClaudeAuthMode::DesktopOAuth || b.auth_mode != ClaudeAuthMode::DesktopOAuth {
        return false;
    }
    match (normalized_account_uuid(a), normalized_account_uuid(b)) {
        (Some(left), Some(right)) => left == right,
        _ => match (normalized_account_email(a), normalized_account_email(b)) {
            (Some(left), Some(right)) => left == right,
            _ => false,
        },
    }
}

fn merge_tags(left: Option<Vec<String>>, right: Option<Vec<String>>) -> Option<Vec<String>> {
    let mut tags = BTreeSet::new();
    for tag in left
        .into_iter()
        .flatten()
        .chain(right.into_iter().flatten())
    {
        let normalized = tag.trim();
        if !normalized.is_empty() {
            tags.insert(normalized.to_string());
        }
    }
    (!tags.is_empty()).then(|| tags.into_iter().collect())
}

fn choose_desktop_duplicate_base<'a>(
    left: &'a ClaudeAccount,
    right: &'a ClaudeAccount,
    current_id: Option<&str>,
) -> &'a ClaudeAccount {
    if current_id == Some(left.id.as_str()) {
        return left;
    }
    if current_id == Some(right.id.as_str()) {
        return right;
    }
    let left_score = (left.last_used, left.created_at);
    let right_score = (right.last_used, right.created_at);
    if right_score > left_score {
        right
    } else {
        left
    }
}

fn merge_desktop_account_fields(base: &ClaudeAccount, incoming: &ClaudeAccount) -> ClaudeAccount {
    let mut merged = base.clone();
    if is_real_email(&incoming.email) || !is_real_email(&merged.email) {
        merged.email = incoming.email.clone();
    }
    if incoming.account_uuid.is_some() {
        merged.account_uuid = incoming.account_uuid.clone();
    }
    if incoming.organization_uuid.is_some() {
        merged.organization_uuid = incoming.organization_uuid.clone();
    }
    if incoming
        .organization_name
        .as_deref()
        .and_then(|value| normalize_non_empty(Some(value)))
        .map(|value| !value.eq_ignore_ascii_case("Claude Desktop"))
        .unwrap_or(false)
    {
        merged.organization_name = incoming.organization_name.clone();
    }
    if incoming.plan_type.is_some() {
        merged.plan_type = incoming.plan_type.clone();
    } else if merged
        .plan_type
        .as_deref()
        .map(is_desktop_plan_placeholder)
        .unwrap_or(false)
    {
        merged.plan_type = None;
    }
    if incoming.avatar_url.is_some() {
        merged.avatar_url = incoming.avatar_url.clone();
    }
    if incoming.profile_updated_at.is_some() {
        merged.profile_updated_at = incoming.profile_updated_at;
    }
    if incoming.quota.is_some() {
        merged.quota = incoming.quota.clone();
    }
    if incoming.usage_updated_at.is_some() {
        merged.usage_updated_at = incoming.usage_updated_at;
    }
    merged.quota_error = incoming.quota_error.clone();
    merged.status = incoming.status.clone();
    merged.status_reason = incoming.status_reason.clone();
    if incoming.desktop_profile_dir.is_some() {
        merged.desktop_profile_dir = incoming.desktop_profile_dir.clone();
    }
    if incoming.desktop_profile_imported_at.is_some() {
        merged.desktop_profile_imported_at = incoming.desktop_profile_imported_at;
    }
    if incoming.claude_credentials_raw.is_some() {
        merged.claude_credentials_raw = incoming.claude_credentials_raw.clone();
    }
    if incoming.claude_config_raw.is_some() {
        merged.claude_config_raw = incoming.claude_config_raw.clone();
    }
    if incoming.claude_usage_raw.is_some() {
        merged.claude_usage_raw = incoming.claude_usage_raw.clone();
    }
    merged.tags = merge_tags(merged.tags.take(), incoming.tags.clone());
    if incoming.account_note.is_some() {
        merged.account_note = incoming.account_note.clone();
    }
    merged.created_at = merged.created_at.min(incoming.created_at);
    merged.last_used = merged.last_used.max(incoming.last_used);
    merged
}

fn remove_desktop_snapshot_if_unused(snapshot: Option<&str>, keep_snapshot: Option<&str>) {
    let Some(snapshot) = snapshot.and_then(|value| normalize_non_empty(Some(value))) else {
        return;
    };
    if keep_snapshot
        .and_then(|value| normalize_non_empty(Some(value)))
        .map(|keep| keep == snapshot)
        .unwrap_or(false)
    {
        return;
    }
    let snapshot_path = PathBuf::from(snapshot);
    if snapshot_path.exists() {
        if let Err(error) = remove_path_if_exists(&snapshot_path) {
            logger::log_warn(&format!(
                "[Claude Desktop] 删除重复账号快照失败: path={}, error={}",
                snapshot_path.display(),
                error
            ));
        }
    }
}

fn delete_account_file_silent(account_id: &str) {
    if let Ok(path) = account_file_path(account_id) {
        if path.exists() {
            if let Err(error) = fs::remove_file(&path) {
                logger::log_warn(&format!(
                    "[Claude Desktop] 删除重复账号文件失败: path={}, error={}",
                    path.display(),
                    error
                ));
            }
        }
    }
}

fn find_existing_desktop_account(incoming: &ClaudeAccount) -> Option<ClaudeAccount> {
    let index = load_index().ok()?;
    index
        .accounts
        .into_iter()
        .filter(|summary| summary.id != incoming.id)
        .filter_map(|summary| load_account_file(&summary.id))
        .find(|account| desktop_accounts_same_identity(account, incoming))
}

fn save_desktop_account_with_dedupe(incoming: ClaudeAccount) -> Result<ClaudeAccount, String> {
    let old_snapshot = incoming.desktop_profile_dir.clone();
    let Some(existing) = find_existing_desktop_account(&incoming) else {
        return save_account_and_index(incoming);
    };
    let existing_snapshot = existing.desktop_profile_dir.clone();
    let merged = merge_desktop_account_fields(&existing, &incoming);
    let saved = save_account_and_index(merged)?;
    remove_desktop_snapshot_if_unused(
        existing_snapshot.as_deref(),
        saved.desktop_profile_dir.as_deref(),
    );
    if saved.desktop_profile_dir.as_deref() != old_snapshot.as_deref() {
        remove_desktop_snapshot_if_unused(
            old_snapshot.as_deref(),
            saved.desktop_profile_dir.as_deref(),
        );
    }
    Ok(saved)
}

fn dedupe_desktop_accounts(accounts: Vec<ClaudeAccount>) -> Result<Vec<ClaudeAccount>, String> {
    let current_id = crate::modules::provider_current_state::get_current_account_id("claude")
        .ok()
        .flatten();
    let mut kept: Vec<ClaudeAccount> = Vec::with_capacity(accounts.len());
    let mut removed_ids = Vec::new();
    let mut rewired_current: Option<String> = None;

    for account in accounts {
        let Some(index) = kept
            .iter()
            .position(|existing| desktop_accounts_same_identity(existing, &account))
        else {
            kept.push(account);
            continue;
        };

        let existing = kept.remove(index);
        let base =
            choose_desktop_duplicate_base(&existing, &account, current_id.as_deref()).clone();
        let other = if base.id == existing.id {
            account
        } else {
            existing
        };
        let old_base_snapshot = base.desktop_profile_dir.clone();
        let other_snapshot = other.desktop_profile_dir.clone();
        let mut merged = merge_desktop_account_fields(&base, &other);
        merged.id = base.id.clone();
        if current_id.as_deref() == Some(other.id.as_str()) {
            rewired_current = Some(base.id.clone());
        }
        remove_desktop_snapshot_if_unused(
            other_snapshot.as_deref(),
            merged.desktop_profile_dir.as_deref(),
        );
        if merged.desktop_profile_dir.as_deref() != old_base_snapshot.as_deref() {
            remove_desktop_snapshot_if_unused(
                old_base_snapshot.as_deref(),
                merged.desktop_profile_dir.as_deref(),
            );
        }
        delete_account_file_silent(&other.id);
        removed_ids.push(other.id.clone());
        kept.push(merged);
    }

    if removed_ids.is_empty() {
        return Ok(kept);
    }

    for account in &kept {
        write_account_file(account)?;
    }
    let mut index = ClaudeAccountIndex::new();
    index.accounts = kept.iter().map(|account| account.summary()).collect();
    index.accounts.sort_by(|a, b| b.last_used.cmp(&a.last_used));
    save_index(&index)?;
    if let Some(next_current) = rewired_current {
        let _ = crate::modules::provider_current_state::set_current_account_id(
            "claude",
            Some(next_current.as_str()),
        );
    }
    logger::log_info(&format!(
        "[Claude Desktop] 已合并重复账号: removed={}",
        removed_ids.join(",")
    ));
    Ok(kept)
}

pub fn list_accounts_checked() -> Result<Vec<ClaudeAccount>, String> {
    let index = load_index()?;
    let mut accounts = Vec::new();
    for summary in index.accounts {
        if let Some(account) = load_account_file(&summary.id) {
            let mut account = account;
            let mut should_save = false;
            if normalize_account_plan_from_snapshots(&mut account) {
                should_save = true;
            }
            if account.auth_mode == ClaudeAuthMode::DesktopOAuth
                && !desktop_account_has_real_profile_data(&account)
            {
                if let Some(snapshot_dir) = account
                    .desktop_profile_dir
                    .as_deref()
                    .and_then(|value| normalize_non_empty(Some(value)))
                    .map(PathBuf::from)
                {
                    if apply_desktop_local_profile(&mut account, &snapshot_dir) {
                        account.quota_error = None;
                        account.status_reason = None;
                        should_save = true;
                    }
                }
            }
            if slim_claude_account_snapshots(&mut account) {
                should_save = true;
            }
            if should_save {
                if let Err(error) = save_account_and_index(account.clone()) {
                    logger::log_warn(&format!(
                        "[Claude] 账号自动迁移保存失败: account_id={}, error={}",
                        account.id, error
                    ));
                }
            }
            accounts.push(account);
        }
    }
    dedupe_desktop_accounts(accounts)
}

fn save_account_and_index(mut account: ClaudeAccount) -> Result<ClaudeAccount, String> {
    slim_claude_account_snapshots(&mut account);
    write_account_file(&account)?;
    let mut index = load_index()?;
    index.accounts.retain(|item| item.id != account.id);
    index.accounts.push(account.summary());
    index.accounts.sort_by(|a, b| b.last_used.cmp(&a.last_used));
    save_index(&index)?;
    Ok(account)
}

fn to_oauth_start_response(state: &PendingClaudeOAuthState) -> ClaudeOAuthStartResponse {
    ClaudeOAuthStartResponse {
        login_id: state.login_id.clone(),
        verification_uri: state.auth_url.clone(),
        expires_in: state
            .expires_at
            .saturating_sub(now_ts())
            .max(0)
            .try_into()
            .unwrap_or(0),
        interval_seconds: 1,
    }
}

fn to_desktop_login_start_response(
    state: &PendingClaudeDesktopLoginState,
) -> ClaudeDesktopLoginStartResponse {
    ClaudeDesktopLoginStartResponse {
        login_id: state.login_id.clone(),
        user_data_dir: state.user_data_dir.to_string_lossy().to_string(),
        expires_in: state
            .expires_at
            .saturating_sub(now_ts())
            .max(0)
            .try_into()
            .unwrap_or(0),
        interval_seconds: 2,
    }
}

fn get_desktop_profiles_dir() -> Result<PathBuf, String> {
    let dir = get_data_dir()?.join(CLAUDE_DESKTOP_PROFILES_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("创建 Claude Desktop 账号快照目录失败: {}", e))?;
    Ok(dir)
}

fn get_desktop_login_root_dir() -> Result<PathBuf, String> {
    let dir = get_data_dir()?.join(CLAUDE_DESKTOP_LOGIN_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("创建 Claude Desktop 登录工作目录失败: {}", e))?;
    Ok(dir)
}

fn get_desktop_backups_dir() -> Result<PathBuf, String> {
    let dir = get_data_dir()?.join(CLAUDE_DESKTOP_BACKUPS_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("创建 Claude Desktop 切号备份目录失败: {}", e))?;
    Ok(dir)
}

pub fn get_default_claude_desktop_user_data_dir() -> Result<PathBuf, String> {
    if let Ok(value) = std::env::var("CLAUDE_DESKTOP_USER_DATA_DIR") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    let data_dir = dirs::data_dir().ok_or_else(|| "无法获取系统应用数据目录".to_string())?;
    Ok(data_dir.join("Claude"))
}

pub fn get_default_claude_code_config_dir() -> Result<PathBuf, String> {
    if let Ok(value) = std::env::var("CLAUDE_CONFIG_DIR") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    let home = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
    Ok(home.join(".claude"))
}

fn get_effective_claude_code_config_dir(config_dir: Option<&Path>) -> Result<PathBuf, String> {
    match config_dir {
        Some(path) => Ok(path.to_path_buf()),
        None => get_default_claude_code_config_dir(),
    }
}

fn get_claude_code_credentials_path(config_dir: &Path) -> PathBuf {
    config_dir.join(CLAUDE_CODE_CREDENTIALS_FILE)
}

fn get_claude_code_settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join(CLAUDE_CODE_SETTINGS_FILE)
}

fn get_claude_code_settings_managed_env_keys_path() -> Result<PathBuf, String> {
    Ok(get_data_dir()?.join(CLAUDE_CODE_SETTINGS_MANAGED_ENV_KEYS_FILE))
}

fn get_claude_code_global_config_path(config_dir: &Path) -> Result<PathBuf, String> {
    let config_json = config_dir.join(CLAUDE_CODE_CONFIG_FILE);
    if config_json.exists() {
        return Ok(config_json);
    }
    if std::env::var("CLAUDE_CONFIG_DIR")
        .ok()
        .and_then(|value| normalize_non_empty(Some(&value)))
        .is_some()
    {
        return Ok(config_dir.join(CLAUDE_CODE_GLOBAL_CONFIG_FILE));
    }
    let home = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
    Ok(home.join(CLAUDE_CODE_GLOBAL_CONFIG_FILE))
}

fn set_pending_oauth_login(state: Option<PendingClaudeOAuthState>) {
    if let Ok(mut guard) = CLAUDE_PENDING_OAUTH_LOGIN.lock() {
        *guard = state.clone();
    }
    let result = match state.as_ref() {
        Some(value) => crate::modules::oauth_pending_state::save(CLAUDE_OAUTH_STATE_FILE, value),
        None => crate::modules::oauth_pending_state::clear(CLAUDE_OAUTH_STATE_FILE),
    };
    if let Err(error) = result {
        logger::log_warn(&format!(
            "[Claude OAuth] 持久化 OAuth pending 状态失败，已忽略: {}",
            error
        ));
    }
}

fn load_pending_oauth_login_from_disk() -> Option<PendingClaudeOAuthState> {
    match crate::modules::oauth_pending_state::load::<PendingClaudeOAuthState>(
        CLAUDE_OAUTH_STATE_FILE,
    ) {
        Ok(Some(state)) => {
            if state.cancelled || now_ts() > state.expires_at {
                let _ = crate::modules::oauth_pending_state::clear(CLAUDE_OAUTH_STATE_FILE);
                None
            } else {
                Some(state)
            }
        }
        Ok(None) => None,
        Err(error) => {
            logger::log_warn(&format!(
                "[Claude OAuth] 读取 OAuth pending 状态失败，已忽略: {}",
                error
            ));
            let _ = crate::modules::oauth_pending_state::clear(CLAUDE_OAUTH_STATE_FILE);
            None
        }
    }
}

fn hydrate_pending_oauth_login_if_missing() {
    if let Ok(mut guard) = CLAUDE_PENDING_OAUTH_LOGIN.lock() {
        if guard.is_none() {
            *guard = load_pending_oauth_login_from_disk();
        }
    }
}

fn get_pending_oauth_login_for(login_id: &str) -> Result<PendingClaudeOAuthState, String> {
    hydrate_pending_oauth_login_if_missing();
    let state = CLAUDE_PENDING_OAUTH_LOGIN
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().cloned())
        .ok_or_else(|| "Claude OAuth 授权流程不存在，请重新开始".to_string())?;
    if state.login_id != login_id {
        return Err("Claude OAuth 授权会话已变更，请重新开始".to_string());
    }
    if state.cancelled {
        return Err("Claude OAuth 授权已取消".to_string());
    }
    if now_ts() > state.expires_at {
        clear_pending_oauth_login_if_matches(login_id);
        return Err("Claude OAuth 授权已超时，请重新开始".to_string());
    }
    Ok(state)
}

fn clear_pending_oauth_login_if_matches(login_id: &str) {
    let should_clear = CLAUDE_PENDING_OAUTH_LOGIN
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|state| state.login_id == login_id))
        .unwrap_or(false);
    if should_clear {
        set_pending_oauth_login(None);
    }
}

fn build_oauth_authorize_url(state: &str, code_challenge: &str) -> Result<String, String> {
    let mut url = Url::parse(CLAUDE_OAUTH_AUTHORIZE_URL)
        .map_err(|e| format!("构建 Claude OAuth 授权地址失败: {}", e))?;
    url.query_pairs_mut()
        .append_pair("code", "true")
        .append_pair("client_id", CLAUDE_OAUTH_CLIENT_ID)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", CLAUDE_OAUTH_MANUAL_REDIRECT_URL)
        .append_pair("scope", &CLAUDE_OAUTH_SCOPES.join(" "))
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state);
    Ok(url.to_string())
}

fn clean_authorization_code(raw: &str) -> (String, Option<String>) {
    let mut code = raw.trim();
    let mut state = None;
    if let Some((before, after)) = code.split_once('#') {
        code = before;
        state = normalize_non_empty(Some(after));
    }
    if let Some((before, _after)) = code.split_once('&') {
        code = before;
    }
    (code.trim().to_string(), state)
}

fn is_claude_oauth_authorize_url(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    (host.eq_ignore_ascii_case("claude.com") || host.eq_ignore_ascii_case("www.claude.com"))
        && url.path().eq_ignore_ascii_case("/cai/oauth/authorize")
}

fn oauth_authorize_url_input_error() -> String {
    "你粘贴的是 OAuth 授权入口链接，不是授权完成后的 code。请先在浏览器完成授权，然后复制最终页面地址或页面显示的 code。".to_string()
}

fn parse_oauth_callback_input(input: &str) -> Result<(String, Option<String>), String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("授权回调链接或 code 不能为空".to_string());
    }

    let mut query_like = None;
    if let Ok(url) = Url::parse(trimmed) {
        if is_claude_oauth_authorize_url(&url) {
            return Err(oauth_authorize_url_input_error());
        }
        let pairs: std::collections::HashMap<String, String> = url
            .query_pairs()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        if pairs
            .get("code")
            .map(|value| value == "true")
            .unwrap_or(false)
            && pairs.get("client_id").is_some()
        {
            return Err(oauth_authorize_url_input_error());
        }
        if let Some(code) = pairs
            .get("code")
            .and_then(|value| normalize_non_empty(Some(value.as_str())))
        {
            let (code, state_from_code) = clean_authorization_code(&code);
            return Ok((code, pairs.get("state").cloned().or(state_from_code)));
        }
        if let Some(fragment) = normalize_non_empty(url.fragment()) {
            query_like = Some(fragment);
        }
    } else if trimmed.starts_with("code=")
        || trimmed.starts_with("state=")
        || trimmed.contains("&code=")
        || trimmed.contains("?code=")
    {
        query_like = Some(
            trimmed
                .split_once('?')
                .map(|(_, query)| query)
                .unwrap_or_else(|| trimmed.trim_start_matches('?'))
                .to_string(),
        );
    }

    if let Some(query) = query_like {
        let pairs: std::collections::HashMap<String, String> =
            form_urlencoded::parse(query.as_bytes())
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect();
        if let Some(code) = pairs
            .get("code")
            .and_then(|value| normalize_non_empty(Some(value.as_str())))
        {
            let (code, state_from_code) = clean_authorization_code(&code);
            return Ok((code, pairs.get("state").cloned().or(state_from_code)));
        }
    }

    let (raw_code, raw_state) = clean_authorization_code(trimmed);
    let code = normalize_non_empty(Some(raw_code.trim_start_matches("code=")))
        .ok_or_else(|| "授权 code 不能为空".to_string())?;
    Ok((code, raw_state))
}

fn set_pending_desktop_login(state: Option<PendingClaudeDesktopLoginState>) {
    if let Ok(mut guard) = CLAUDE_PENDING_DESKTOP_LOGIN.lock() {
        *guard = state.clone();
    }
    let result = match state.as_ref() {
        Some(value) => {
            crate::modules::oauth_pending_state::save(CLAUDE_DESKTOP_LOGIN_STATE_FILE, value)
        }
        None => crate::modules::oauth_pending_state::clear(CLAUDE_DESKTOP_LOGIN_STATE_FILE),
    };
    if let Err(error) = result {
        logger::log_warn(&format!(
            "[Claude Desktop] 持久化登录 pending 状态失败，已忽略: {}",
            error
        ));
    }
}

fn load_pending_desktop_login_from_disk() -> Option<PendingClaudeDesktopLoginState> {
    match crate::modules::oauth_pending_state::load::<PendingClaudeDesktopLoginState>(
        CLAUDE_DESKTOP_LOGIN_STATE_FILE,
    ) {
        Ok(Some(state)) => {
            if state.cancelled || now_ts() > state.expires_at {
                let _ = crate::modules::oauth_pending_state::clear(CLAUDE_DESKTOP_LOGIN_STATE_FILE);
                None
            } else {
                Some(state)
            }
        }
        Ok(None) => None,
        Err(error) => {
            logger::log_warn(&format!(
                "[Claude Desktop] 读取登录 pending 状态失败，已忽略: {}",
                error
            ));
            let _ = crate::modules::oauth_pending_state::clear(CLAUDE_DESKTOP_LOGIN_STATE_FILE);
            None
        }
    }
}

fn hydrate_pending_desktop_login_if_missing() {
    if let Ok(mut guard) = CLAUDE_PENDING_DESKTOP_LOGIN.lock() {
        if guard.is_none() {
            *guard = load_pending_desktop_login_from_disk();
        }
    }
}

fn get_pending_desktop_login_for(login_id: &str) -> Result<PendingClaudeDesktopLoginState, String> {
    hydrate_pending_desktop_login_if_missing();
    let state = CLAUDE_PENDING_DESKTOP_LOGIN
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().cloned())
        .ok_or_else(|| "Claude Desktop 登录流程不存在，请重新开始".to_string())?;
    if state.login_id != login_id {
        return Err("Claude Desktop 登录会话已变更，请重新开始".to_string());
    }
    if state.cancelled {
        return Err("Claude Desktop 登录已取消".to_string());
    }
    if now_ts() > state.expires_at {
        clear_pending_desktop_login_if_matches(login_id);
        return Err("Claude Desktop 登录已超时，请重新开始".to_string());
    }
    Ok(state)
}

fn clear_pending_desktop_login_if_matches(login_id: &str) {
    let should_clear = CLAUDE_PENDING_DESKTOP_LOGIN
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|state| state.login_id == login_id))
        .unwrap_or(false);
    if should_clear {
        set_pending_desktop_login(None);
    }
}

fn build_account_id(email: &str, account_uuid: Option<&str>, org_uuid: Option<&str>) -> String {
    let identity = format!(
        "{}:{}:{}",
        email.trim().to_ascii_lowercase(),
        account_uuid.unwrap_or_default().trim(),
        org_uuid.unwrap_or_default().trim()
    );
    format!("claude_{:x}", md5::compute(identity.as_bytes()))
}

#[derive(Debug, Clone, Default)]
pub struct ClaudeApiKeyProviderConfig {
    pub api_base_url: Option<String>,
    pub api_provider_id: Option<String>,
    pub api_provider_name: Option<String>,
    pub api_provider_source_tag: Option<String>,
    pub api_provider_website: Option<String>,
    pub api_provider_api_key_url: Option<String>,
    pub api_key_field: Option<String>,
    pub api_model_catalog: Option<Vec<String>>,
    pub api_extra_env: Option<BTreeMap<String, String>>,
}

fn build_api_key_account_id(api_key: &str, api_base_url: Option<&str>) -> String {
    let identity = format!(
        "{}:{}",
        api_base_url.unwrap_or_default().trim().to_ascii_lowercase(),
        api_key
    );
    format!("claude_apikey_{:x}", md5::compute(identity.as_bytes()))
}

fn build_api_key_display_name(
    api_key: &str,
    account_name: Option<&str>,
    provider_name: Option<&str>,
) -> String {
    if let Some(name) = normalize_non_empty(account_name) {
        return name;
    }
    let suffix: String = api_key
        .chars()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if let Some(provider_name) = normalize_non_empty(provider_name) {
        return format!("{} {}", provider_name, suffix);
    }
    format!("API Key {}", suffix)
}

fn normalize_api_provider_base_url(raw: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = raw.and_then(|value| normalize_non_empty(Some(value))) else {
        return Ok(None);
    };
    let parsed = Url::parse(&value).map_err(|_| "供应商 Base URL 不是有效 URL".to_string())?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("供应商 Base URL 仅支持 http/https".to_string());
    }
    Ok(Some(value.trim_end_matches('/').to_string()))
}

fn normalize_api_key_field(value: Option<&str>, api_base_url: Option<&str>) -> String {
    match value
        .and_then(|item| normalize_non_empty(Some(item)))
        .map(|item| item.to_ascii_uppercase())
        .as_deref()
    {
        Some("ANTHROPIC_API_KEY") => "ANTHROPIC_API_KEY".to_string(),
        Some("ANTHROPIC_AUTH_TOKEN") => "ANTHROPIC_AUTH_TOKEN".to_string(),
        _ if is_official_anthropic_api_base_url(api_base_url) => "ANTHROPIC_API_KEY".to_string(),
        _ => "ANTHROPIC_AUTH_TOKEN".to_string(),
    }
}

fn is_official_anthropic_api_base_url(api_base_url: Option<&str>) -> bool {
    let Some(value) = api_base_url.and_then(|value| normalize_non_empty(Some(value))) else {
        return true;
    };
    Url::parse(&value)
        .ok()
        .map(|url| {
            let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
            host == "api.anthropic.com" || host == "api.claude.com"
        })
        .unwrap_or(false)
}

fn normalize_model_catalog(value: Option<Vec<String>>) -> Option<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut models = Vec::new();
    for model in value.into_iter().flatten() {
        let normalized = model.trim();
        if normalized.is_empty() {
            continue;
        }
        let key = normalized.to_ascii_lowercase();
        if seen.insert(key) {
            models.push(normalized.to_string());
        }
    }
    (!models.is_empty()).then_some(models)
}

fn normalize_api_extra_env(
    value: Option<BTreeMap<String, String>>,
) -> Option<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for (key, value) in value.into_iter().flatten() {
        let key = key.trim().to_ascii_uppercase();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            continue;
        }
        if matches!(
            key.as_str(),
            "ANTHROPIC_API_KEY" | "ANTHROPIC_AUTH_TOKEN" | "ANTHROPIC_BASE_URL"
        ) {
            continue;
        }
        result.insert(key, value.to_string());
    }
    (!result.is_empty()).then_some(result)
}

fn normalize_claude_api_key(raw: &str, require_anthropic_key: bool) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err("请输入 API Key".to_string());
    }
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Err("API Key 不能填写 URL".to_string());
    }
    if value.chars().any(char::is_whitespace) {
        return Err("API Key 不能包含空白字符".to_string());
    }
    if require_anthropic_key && !value.starts_with("sk-ant-") {
        return Err("请输入以 sk-ant- 开头的 Anthropic API Key".to_string());
    }
    Ok(value.to_string())
}

fn credentials_oauth(credentials: &Value) -> Option<&Value> {
    credentials.get("claudeAiOauth")
}

fn credentials_refresh_token(credentials: &Value) -> Option<String> {
    read_string_path(credentials, &["claudeAiOauth", "refreshToken"])
}

fn credentials_access_token(credentials: &Value) -> Option<String> {
    read_string_path(credentials, &["claudeAiOauth", "accessToken"])
}

fn credentials_expires_at(credentials: &Value) -> Option<i64> {
    read_i64_value(
        credentials
            .get("claudeAiOauth")
            .and_then(|item| item.get("expiresAt")),
    )
}

fn token_is_expired(credentials: &Value) -> bool {
    let Some(expires_at) = credentials_expires_at(credentials) else {
        return false;
    };
    now_ts_ms() + CLAUDE_TOKEN_EXPIRY_BUFFER_MS >= expires_at
}

fn config_oauth_account(config: &Value) -> Option<&Value> {
    config.get("oauthAccount")
}

fn slim_claude_code_config_snapshot(config: &Value) -> Value {
    let mut object = serde_json::Map::new();

    if let Some(oauth_account) = config.get("oauthAccount").cloned() {
        object.insert("oauthAccount".to_string(), oauth_account);
    }
    if let Some(email) = config.get("email").cloned() {
        object.insert("email".to_string(), email);
    }
    if let Some(has_completed_onboarding) = config.get("hasCompletedOnboarding").cloned() {
        object.insert(
            "hasCompletedOnboarding".to_string(),
            has_completed_onboarding,
        );
    } else if object.contains_key("oauthAccount") {
        object.insert("hasCompletedOnboarding".to_string(), Value::Bool(true));
    }

    Value::Object(object)
}

fn slim_claude_account_snapshots(account: &mut ClaudeAccount) -> bool {
    if !matches!(
        account.auth_mode,
        ClaudeAuthMode::OAuth | ClaudeAuthMode::SetupToken
    ) {
        return false;
    }
    let Some(config_raw) = account.claude_config_raw.as_ref() else {
        return false;
    };
    let slimmed = slim_claude_code_config_snapshot(config_raw);
    if &slimmed == config_raw {
        return false;
    }
    account.claude_config_raw = Some(slimmed);
    true
}

fn read_bool_path(value: &Value, path: &[&str]) -> Option<bool> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    read_bool_value(Some(current))
}

fn derive_oauth_snapshot_plan_type(
    credentials_raw: &Value,
    oauth_account: &Value,
) -> Option<String> {
    let credentials_oauth = credentials_oauth(credentials_raw);
    let profile = credentials_oauth.and_then(|value| value.get("profile"));

    for raw in [
        read_string_path(oauth_account, &["subscriptionType"]),
        credentials_oauth.and_then(|value| read_string_path(value, &["subscriptionType"])),
        read_string_path(oauth_account, &["organizationType"]),
        profile.and_then(|value| read_string_path(value, &["organization", "organization_type"])),
        read_string_path(oauth_account, &["rateLimitTier"]),
        credentials_oauth.and_then(|value| read_string_path(value, &["rateLimitTier"])),
        profile.and_then(|value| read_string_path(value, &["organization", "rate_limit_tier"])),
    ] {
        if let Some(plan) = normalize_desktop_plan_value(raw) {
            return Some(plan);
        }
    }

    if profile
        .and_then(|value| read_bool_path(value, &["account", "has_claude_max"]))
        .unwrap_or(false)
    {
        return Some("Max".to_string());
    }
    if profile
        .and_then(|value| read_bool_path(value, &["account", "has_claude_pro"]))
        .unwrap_or(false)
    {
        return Some("Pro".to_string());
    }

    None
}

fn is_claude_billing_source_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "apple_subscription" | "apple subscription" | "stripe_subscription" | "stripe subscription"
    )
}

fn normalize_account_plan_from_snapshots(account: &mut ClaudeAccount) -> bool {
    let Some(config_raw) = account.claude_config_raw.as_ref() else {
        return false;
    };
    let Some(oauth_account) = config_oauth_account(config_raw) else {
        return false;
    };
    let credentials_raw = account
        .claude_credentials_raw
        .as_ref()
        .unwrap_or(&Value::Null);
    let Some(plan_type) = derive_oauth_snapshot_plan_type(credentials_raw, oauth_account) else {
        return false;
    };
    if account.plan_type.as_deref() == Some(plan_type.as_str()) {
        return false;
    }
    let should_replace = account
        .plan_type
        .as_deref()
        .map(|value| is_claude_billing_source_value(value) || is_desktop_plan_placeholder(value))
        .unwrap_or(true);
    if !should_replace {
        return false;
    }
    account.plan_type = Some(plan_type);
    true
}

fn derive_account_from_snapshots(
    credentials_raw: Value,
    config_raw: Value,
    existing: Option<ClaudeAccount>,
) -> Result<ClaudeAccount, String> {
    if credentials_oauth(&credentials_raw).is_none() {
        return Err("Claude credentials 缺少 claudeAiOauth 字段".to_string());
    }
    let oauth_account = config_oauth_account(&config_raw)
        .ok_or_else(|| "Claude config 缺少 oauthAccount 字段".to_string())?;
    let email = read_string_path(oauth_account, &["emailAddress"])
        .or_else(|| read_string_path(&config_raw, &["email"]))
        .ok_or_else(|| "Claude config 缺少账号邮箱".to_string())?;
    let account_uuid = read_string_path(oauth_account, &["accountUuid"]);
    let organization_uuid = read_string_path(oauth_account, &["organizationUuid"]);
    let organization_name = read_string_path(oauth_account, &["organizationName"]);
    let avatar_url = read_string_path(oauth_account, &["avatarUrl"])
        .or_else(|| read_string_path(oauth_account, &["avatar_url"]));
    let plan_type = derive_oauth_snapshot_plan_type(&credentials_raw, oauth_account);
    let id = build_account_id(
        &email,
        account_uuid.as_deref(),
        organization_uuid.as_deref(),
    );
    let now = now_ts_ms();
    let mut account = existing.unwrap_or_else(|| ClaudeAccount {
        id: id.clone(),
        email: email.clone(),
        auth_mode: ClaudeAuthMode::OAuth,
        account_uuid: None,
        organization_uuid: None,
        organization_name: None,
        plan_type: None,
        avatar_url: None,
        profile_updated_at: None,
        quota: None,
        quota_error: None,
        usage_updated_at: None,
        status: None,
        status_reason: None,
        api_key: None,
        api_base_url: None,
        api_provider_id: None,
        api_provider_name: None,
        api_provider_source_tag: None,
        api_provider_website: None,
        api_provider_api_key_url: None,
        api_key_field: None,
        api_model_catalog: None,
        api_extra_env: None,
        desktop_profile_dir: None,
        desktop_profile_imported_at: None,
        claude_credentials_raw: None,
        claude_config_raw: None,
        claude_usage_raw: None,
        tags: None,
        account_note: None,
        created_at: now,
        last_used: now,
    });
    account.id = id;
    account.email = email;
    account.auth_mode = if credentials_refresh_token(&credentials_raw).is_some() {
        ClaudeAuthMode::OAuth
    } else {
        ClaudeAuthMode::SetupToken
    };
    account.account_uuid = account_uuid;
    account.organization_uuid = organization_uuid;
    account.organization_name = organization_name;
    account.plan_type = plan_type;
    account.avatar_url = avatar_url;
    account.profile_updated_at = Some(now);
    account.api_key = None;
    account.api_base_url = None;
    account.api_provider_id = None;
    account.api_provider_name = None;
    account.api_provider_source_tag = None;
    account.api_provider_website = None;
    account.api_provider_api_key_url = None;
    account.api_key_field = None;
    account.api_model_catalog = None;
    account.api_extra_env = None;
    account.claude_credentials_raw = Some(credentials_raw);
    account.claude_config_raw = Some(config_raw);
    account.last_used = now;
    account.status = None;
    account.status_reason = None;
    account.desktop_profile_dir = None;
    account.desktop_profile_imported_at = None;
    Ok(account)
}

pub fn upsert_account_from_snapshots(
    credentials_raw: Value,
    config_raw: Value,
) -> Result<ClaudeAccount, String> {
    let temp = derive_account_from_snapshots(credentials_raw, config_raw, None)?;
    let existing = load_account_file(&temp.id);
    let account = derive_account_from_snapshots(
        temp.claude_credentials_raw.clone().unwrap_or(Value::Null),
        temp.claude_config_raw.clone().unwrap_or(Value::Null),
        existing,
    )?;
    save_desktop_account_with_dedupe(account)
}

pub fn import_api_key(
    api_key: &str,
    account_name: Option<&str>,
    provider_config: ClaudeApiKeyProviderConfig,
) -> Result<ClaudeAccount, String> {
    let api_base_url = normalize_api_provider_base_url(provider_config.api_base_url.as_deref())?;
    let require_anthropic_key = is_official_anthropic_api_base_url(api_base_url.as_deref());
    let api_key = normalize_claude_api_key(api_key, require_anthropic_key)?;
    let api_key_field = normalize_api_key_field(
        provider_config.api_key_field.as_deref(),
        api_base_url.as_deref(),
    );
    let api_provider_name = normalize_non_empty(provider_config.api_provider_name.as_deref())
        .or_else(|| {
            api_base_url.as_deref().and_then(|value| {
                Url::parse(value).ok().and_then(|url| {
                    url.host_str()
                        .map(|host| host.trim_start_matches("www.").to_string())
                })
            })
        })
        .or_else(|| Some("Anthropic Official".to_string()));
    let api_provider_id = normalize_non_empty(provider_config.api_provider_id.as_deref());
    let api_provider_source_tag =
        normalize_non_empty(provider_config.api_provider_source_tag.as_deref());
    let api_provider_website = normalize_non_empty(provider_config.api_provider_website.as_deref());
    let api_provider_api_key_url =
        normalize_non_empty(provider_config.api_provider_api_key_url.as_deref());
    let api_model_catalog = normalize_model_catalog(provider_config.api_model_catalog);
    let api_extra_env = normalize_api_extra_env(provider_config.api_extra_env);
    let id = build_api_key_account_id(&api_key, api_base_url.as_deref());
    let display_name =
        build_api_key_display_name(&api_key, account_name, api_provider_name.as_deref());
    let now = now_ts_ms();
    let mut account = load_account_file(&id).unwrap_or_else(|| ClaudeAccount {
        id: id.clone(),
        email: display_name.clone(),
        auth_mode: ClaudeAuthMode::ApiKey,
        account_uuid: None,
        organization_uuid: None,
        organization_name: None,
        plan_type: None,
        avatar_url: None,
        profile_updated_at: None,
        quota: None,
        quota_error: None,
        usage_updated_at: None,
        status: None,
        status_reason: None,
        api_key: None,
        api_base_url: None,
        api_provider_id: None,
        api_provider_name: None,
        api_provider_source_tag: None,
        api_provider_website: None,
        api_provider_api_key_url: None,
        api_key_field: None,
        api_model_catalog: None,
        api_extra_env: None,
        desktop_profile_dir: None,
        desktop_profile_imported_at: None,
        claude_credentials_raw: None,
        claude_config_raw: None,
        claude_usage_raw: None,
        tags: None,
        account_note: None,
        created_at: now,
        last_used: now,
    });
    let key_hash = format!("{:x}", md5::compute(api_key.as_bytes()));
    let provider_snapshot = json!({
        "id": api_provider_id.clone(),
        "name": api_provider_name.clone(),
        "baseUrl": api_base_url.clone(),
        "sourceTag": api_provider_source_tag.clone(),
        "website": api_provider_website.clone(),
        "apiKeyUrl": api_provider_api_key_url.clone(),
        "keyField": api_key_field.clone(),
        "modelCatalog": api_model_catalog.clone(),
        "extraEnv": api_extra_env.clone(),
    });
    account.id = id;
    account.email = display_name;
    account.auth_mode = ClaudeAuthMode::ApiKey;
    account.account_uuid = None;
    account.organization_uuid = None;
    account.organization_name = None;
    account.plan_type = api_provider_name
        .clone()
        .or_else(|| Some("API Key".to_string()));
    account.avatar_url = None;
    account.profile_updated_at = None;
    account.quota = None;
    account.quota_error = None;
    account.usage_updated_at = None;
    account.status = None;
    account.status_reason = None;
    account.api_key = Some(api_key.clone());
    account.api_base_url = api_base_url.clone();
    account.api_provider_id = api_provider_id.clone();
    account.api_provider_name = api_provider_name.clone();
    account.api_provider_source_tag = api_provider_source_tag.clone();
    account.api_provider_website = api_provider_website.clone();
    account.api_provider_api_key_url = api_provider_api_key_url.clone();
    account.api_key_field = Some(api_key_field.clone());
    account.api_model_catalog = api_model_catalog.clone();
    account.api_extra_env = api_extra_env.clone();
    account.desktop_profile_dir = None;
    account.desktop_profile_imported_at = None;
    account.claude_credentials_raw = Some(json!({
        "authMode": "api_key",
        "anthropicApiKey": api_key,
        "apiKeyField": api_key_field,
        "apiProvider": provider_snapshot.clone(),
    }));
    account.claude_config_raw = Some(json!({
        "apiKeyAccount": {
            "label": account.email.clone(),
            "keyHash": key_hash,
            "provider": provider_snapshot,
        },
        "hasCompletedOnboarding": true,
    }));
    account.last_used = now;
    save_account_and_index(account)
}

fn desktop_account_display_name(account_name: Option<&str>) -> String {
    if let Some(name) = normalize_non_empty(account_name) {
        return name;
    }
    format!(
        "Claude Desktop {}",
        chrono::Utc::now().format("%Y-%m-%d %H:%M")
    )
}

fn build_desktop_account_id(label: &str) -> String {
    let random = generate_random_url_token(18);
    format!(
        "claude_desktop_{:x}",
        md5::compute(format!("{}:{}:{}", label, now_ts_ms(), random).as_bytes())
    )
}

fn cookies_db_has_required_desktop_session(cookies_path: &Path) -> Result<bool, String> {
    if !cookies_path.exists() {
        return Ok(false);
    }
    let conn = Connection::open_with_flags(cookies_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| {
            format!(
                "读取 Claude Desktop Cookies 失败: path={}, error={}",
                cookies_path.display(),
                e
            )
        })?;
    let count: i64 = conn
        .query_row(
            "select count(distinct name) from cookies \
             where name in ('sessionKey', 'lastActiveOrg') \
             and (host_key like '%claude.ai' or host_key like '%claude.com') \
             and (length(value) > 0 or length(encrypted_value) > 0)",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("查询 Claude Desktop Cookies 失败: {}", e))?;
    Ok(count >= 2)
}

fn ensure_desktop_profile_logged_in(profile_dir: &Path) -> Result<(), String> {
    if !profile_dir.exists() {
        return Err(format!(
            "Claude Desktop profile 不存在: {}",
            profile_dir.display()
        ));
    }
    if !cookies_db_has_required_desktop_session(&profile_dir.join("Cookies"))? {
        return Err(
            "未检测到 Claude Desktop 登录态，请在授权窗口或官方 Claude Desktop 完成登录后再导入。"
                .to_string(),
        );
    }
    Ok(())
}

fn chromium_cookie_expires_utc_to_unix_ms(expires_utc: i64) -> Option<i64> {
    if expires_utc <= 0 {
        return None;
    }
    let unix_ms = expires_utc / 1000 - 11_644_473_600_000;
    (unix_ms > 0).then_some(unix_ms)
}

fn desktop_session_expiration_to_ms(expiration_date: Option<f64>) -> Option<i64> {
    let seconds = expiration_date?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }
    Some((seconds * 1000.0).round() as i64)
}

fn desktop_cookie_names(cookies: &[ClaudeDesktopAuthCookie]) -> Vec<String> {
    let mut names = BTreeSet::new();
    for cookie in cookies {
        if is_claude_cookie_domain(&cookie.domain) {
            names.insert(cookie.name.clone());
        }
    }
    names.into_iter().collect()
}

fn desktop_profile_metadata_from_export(
    export: &ClaudeDesktopAuthCookieExport,
    source: &str,
) -> ClaudeDesktopProfileMetadata {
    let session_key = export.cookies.iter().find(|cookie| {
        cookie.name == "sessionKey"
            && !cookie.value.is_empty()
            && is_claude_cookie_domain(&cookie.domain)
    });
    let last_active_org = export.cookies.iter().find(|cookie| {
        cookie.name == "lastActiveOrg"
            && !cookie.value.is_empty()
            && is_claude_cookie_domain(&cookie.domain)
    });
    ClaudeDesktopProfileMetadata {
        source: source.to_string(),
        has_session_key: session_key.is_some(),
        has_last_active_org: last_active_org.is_some(),
        last_active_org: last_active_org
            .and_then(|cookie| normalize_non_empty(Some(&cookie.value))),
        session_expires_at: session_key
            .and_then(|cookie| desktop_session_expiration_to_ms(cookie.expiration_date)),
        cookie_names: desktop_cookie_names(&export.cookies),
        web_profile: export.web_profile.clone(),
    }
}

fn desktop_profile_metadata_from_cookies_db(
    profile_dir: &Path,
    source: &str,
) -> Result<ClaudeDesktopProfileMetadata, String> {
    let cookies_path = profile_dir.join("Cookies");
    let conn = Connection::open_with_flags(&cookies_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| {
            format!(
                "读取 Claude Desktop Cookies 失败: path={}, error={}",
                cookies_path.display(),
                e
            )
        })?;
    let mut stmt = conn
        .prepare(
            "select name, value, coalesce(length(encrypted_value), 0), expires_utc from cookies \
             where (host_key like '%claude.ai' or host_key like '%claude.com')",
        )
        .map_err(|e| format!("查询 Claude Desktop Cookies 失败: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(|e| format!("读取 Claude Desktop Cookies 失败: {}", e))?;

    let mut cookie_names = BTreeSet::new();
    let mut has_session_key = false;
    let mut has_last_active_org = false;
    let mut last_active_org = None;
    let mut session_expires_at = None;
    for row in rows {
        let (name, value, encrypted_len, expires_utc) =
            row.map_err(|e| format!("读取 Claude Desktop Cookie 行失败: {}", e))?;
        let has_value = !value.is_empty() || encrypted_len > 0;
        if !has_value {
            continue;
        }
        cookie_names.insert(name.clone());
        if name == "sessionKey" {
            has_session_key = true;
            session_expires_at = chromium_cookie_expires_utc_to_unix_ms(expires_utc);
        } else if name == "lastActiveOrg" {
            has_last_active_org = true;
            last_active_org = normalize_non_empty(Some(&value));
        }
    }

    Ok(ClaudeDesktopProfileMetadata {
        source: source.to_string(),
        has_session_key,
        has_last_active_org,
        last_active_org,
        session_expires_at,
        cookie_names: cookie_names.into_iter().collect(),
        web_profile: None,
    })
}

fn desktop_profile_metadata(
    profile_dir: &Path,
    source: &str,
) -> Result<ClaudeDesktopProfileMetadata, String> {
    match read_desktop_auth_cookie_export(profile_dir)
        .and_then(|export| ensure_desktop_auth_export_logged_in(&export).map(|_| export))
    {
        Ok(export) => Ok(desktop_profile_metadata_from_export(&export, source)),
        Err(_) => desktop_profile_metadata_from_cookies_db(profile_dir, source),
    }
}

fn printable_ascii(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| {
            if (32..=126).contains(byte) {
                *byte as char
            } else {
                ' '
            }
        })
        .collect()
}

fn normalize_profile_text_value(raw: &str) -> Option<String> {
    let mut result = String::new();
    let mut last_was_space = false;
    for ch in raw.chars() {
        if ch == '"' || ch == '\\' || ch == '{' || ch == '}' || ch == '[' || ch == ']' {
            break;
        }
        if ch.is_ascii_control() {
            break;
        }
        let keep = ch.is_ascii_alphanumeric()
            || matches!(
                ch,
                ' ' | '_' | '-' | '.' | '\'' | '@' | '&' | '(' | ')' | '+'
            );
        if !keep {
            if result.is_empty() {
                continue;
            }
            break;
        }
        if ch.is_ascii_whitespace() {
            if !result.is_empty() && !last_was_space {
                result.push(' ');
            }
            last_was_space = true;
        } else {
            result.push(ch);
            last_was_space = false;
        }
        if result.len() >= 120 {
            break;
        }
    }
    normalize_non_empty(Some(result.trim()))
}

fn extract_text_after_key(text: &str, key: &str) -> Option<String> {
    let pos = text.find(key)?;
    let after = &text[pos + key.len()..text.len().min(pos + key.len() + 220)];
    let start = after
        .char_indices()
        .find(|(_, ch)| ch.is_ascii_alphanumeric())?
        .0;
    normalize_profile_text_value(&after[start..])
}

fn is_ignored_profile_email(email: &str) -> bool {
    let email = email.trim().to_ascii_lowercase();
    let Some((local, domain)) = email.split_once('@') else {
        return true;
    };
    if local.len() < 2 || !domain.contains('.') {
        return true;
    }
    if email.contains("example")
        || email.contains("placeholder")
        || email.contains("noreply")
        || email.contains("no-reply")
        || domain.contains("sentry")
        || domain == "w3.org"
        || domain == "schema.org"
        || domain == "chromium.org"
    {
        return true;
    }
    false
}

fn extract_desktop_local_profile_from_bytes(
    source: &Path,
    bytes: &[u8],
) -> Option<ClaudeDesktopLocalProfile> {
    let text = printable_ascii(bytes);
    let mut best: Option<ClaudeDesktopLocalProfile> = None;
    for email_match in EMAIL_RE.find_iter(&text) {
        let email = email_match.as_str().to_ascii_lowercase();
        if is_ignored_profile_email(&email) {
            continue;
        }
        let start = email_match.start().saturating_sub(900);
        let end = (email_match.end() + 2200).min(text.len());
        let window = &text[start..end];
        let email_local_index = email_match.start().saturating_sub(start);
        let before_email = &window[..email_local_index.min(window.len())];
        let after_email = &window[email_local_index.min(window.len())..];
        let profile_context = window.contains("email_address")
            || window.contains("display_name")
            || window.contains("full_name")
            || window.contains("memberships")
            || window.contains("organization");
        if !profile_context {
            continue;
        }

        let account_uuid = UUID_RE
            .find_iter(before_email)
            .last()
            .map(|item| item.as_str().to_string());
        let organization_window = after_email
            .find("organization")
            .map(|pos| &after_email[pos..after_email.len().min(pos + 1200)]);
        let organization_uuid = organization_window
            .and_then(|value| UUID_RE.find(value))
            .map(|item| item.as_str().to_string());
        let organization_name =
            organization_window.and_then(|value| extract_text_after_key(value, "name"));
        let full_name = extract_text_after_key(after_email, "full_name");
        let display_name = extract_text_after_key(after_email, "display_name");
        let candidate = ClaudeDesktopLocalProfile {
            email: Some(email),
            account_uuid,
            full_name,
            display_name,
            organization_uuid,
            organization_name,
            source: Some(source.to_string_lossy().to_string()),
        };
        if best
            .as_ref()
            .map(|current| candidate.score() > current.score())
            .unwrap_or(true)
        {
            best = Some(candidate);
        }
    }
    best
}

fn collect_desktop_local_profile_files(root: &Path, files: &mut Vec<PathBuf>) {
    if files.len() >= CLAUDE_DESKTOP_LOCAL_PROFILE_MAX_FILES {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if files.len() >= CLAUDE_DESKTOP_LOCAL_PROFILE_MAX_FILES {
            return;
        }
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            collect_desktop_local_profile_files(&path, files);
        } else if metadata.is_file()
            && metadata.len() > 0
            && metadata.len() <= CLAUDE_DESKTOP_LOCAL_PROFILE_MAX_FILE_BYTES
        {
            files.push(path);
        }
    }
}

fn read_desktop_local_profile(profile_dir: &Path) -> Option<ClaudeDesktopLocalProfile> {
    let mut files = Vec::new();
    for dir in CLAUDE_DESKTOP_LOCAL_PROFILE_SCAN_DIRS {
        let root = profile_dir.join(dir);
        if root.exists() {
            collect_desktop_local_profile_files(&root, &mut files);
        }
    }

    let mut best: Option<ClaudeDesktopLocalProfile> = None;
    for file in files {
        let Ok(bytes) = fs::read(&file) else {
            continue;
        };
        let Some(mut candidate) = extract_desktop_local_profile_from_bytes(&file, &bytes) else {
            continue;
        };
        candidate.source = file
            .strip_prefix(profile_dir)
            .ok()
            .map(|value| value.to_string_lossy().to_string())
            .or(candidate.source);
        if !candidate.has_identity() {
            continue;
        }
        if best
            .as_ref()
            .map(|current| candidate.score() > current.score())
            .unwrap_or(true)
        {
            best = Some(candidate);
        }
    }
    best
}

fn upsert_desktop_profile_json(account: &mut ClaudeAccount, key: &str, value: Value) {
    if account.claude_config_raw.is_none() {
        account.claude_config_raw = Some(json!({}));
    }
    let Some(config) = account.claude_config_raw.as_mut() else {
        return;
    };
    if !config.is_object() {
        *config = json!({});
    }
    if let Some(object) = config.as_object_mut() {
        let desktop_profile = object
            .entry("desktopProfile".to_string())
            .or_insert_with(|| json!({}));
        if !desktop_profile.is_object() {
            *desktop_profile = json!({});
        }
        if let Some(desktop_object) = desktop_profile.as_object_mut() {
            desktop_object.insert(key.to_string(), value);
        }
    }
}

fn apply_desktop_local_profile(account: &mut ClaudeAccount, profile_dir: &Path) -> bool {
    let Some(local_profile) = read_desktop_local_profile(profile_dir) else {
        return false;
    };
    let mut changed = false;
    if let Some(email) = local_profile.email.as_ref() {
        if account.email.trim() != email {
            account.email = email.clone();
            changed = true;
        }
    }
    if account.account_uuid.is_none() && local_profile.account_uuid.is_some() {
        account.account_uuid = local_profile.account_uuid.clone();
        changed = true;
    }
    if account.organization_uuid.is_none() && local_profile.organization_uuid.is_some() {
        account.organization_uuid = local_profile.organization_uuid.clone();
        changed = true;
    }
    if let Some(organization_name) = local_profile.organization_name.as_ref() {
        let should_update = account
            .organization_name
            .as_deref()
            .map(|value| value.trim().is_empty() || value.eq_ignore_ascii_case("Claude Desktop"))
            .unwrap_or(true);
        if should_update {
            account.organization_name = Some(organization_name.clone());
            changed = true;
        }
    }
    if account
        .plan_type
        .as_deref()
        .map(is_desktop_plan_placeholder)
        .unwrap_or(false)
    {
        account.plan_type = None;
        changed = true;
    }
    if changed {
        account.profile_updated_at = Some(now_ts_ms());
    }
    let summary = json!({
        "email": local_profile.email,
        "accountUuid": local_profile.account_uuid,
        "fullName": local_profile.full_name,
        "displayName": local_profile.display_name,
        "organizationUuid": local_profile.organization_uuid,
        "organizationName": local_profile.organization_name,
        "source": local_profile.source,
        "fetchedAt": chrono::Utc::now().to_rfc3339(),
    });
    upsert_desktop_profile_json(account, "localProfileSummary", summary);
    changed
}

fn desktop_profile_metadata_json(
    metadata: &ClaudeDesktopProfileMetadata,
    snapshot_dir: &Path,
    imported_at: i64,
) -> Value {
    json!({
        "snapshotDir": snapshot_dir.to_string_lossy().to_string(),
        "importedAt": imported_at,
        "source": metadata.source.clone(),
        "profileSnapshot": true,
        "hasSessionKey": metadata.has_session_key,
        "hasLastActiveOrg": metadata.has_last_active_org,
        "lastActiveOrg": metadata.last_active_org.clone(),
        "sessionExpiresAt": metadata.session_expires_at,
        "cookieNames": metadata.cookie_names.clone(),
        "webProfileFetchedAt": metadata.web_profile.as_ref().and_then(|profile| read_string_path(profile, &["fetchedAt"])),
        "webProfileErrors": metadata.web_profile.as_ref().and_then(|profile| profile.get("errors")).cloned(),
    })
}

fn desktop_auth_export_path(profile_dir: &Path) -> PathBuf {
    profile_dir.join(CLAUDE_DESKTOP_AUTH_EXPORT_FILE)
}

fn read_desktop_auth_cookie_export(
    profile_dir: &Path,
) -> Result<ClaudeDesktopAuthCookieExport, String> {
    let path = desktop_auth_export_path(profile_dir);
    let content = fs::read_to_string(&path).map_err(|e| {
        format!(
            "读取 Claude Desktop 授权 cookie 导出失败: path={}, error={}",
            path.display(),
            e
        )
    })?;
    serde_json::from_str(&content).map_err(|e| {
        format!(
            "解析 Claude Desktop 授权 cookie 导出失败: path={}, error={}",
            path.display(),
            e
        )
    })
}

#[cfg(target_os = "macos")]
fn read_claude_safe_storage_password() -> Result<String, String> {
    for account in ["Claude", "Claude Key"] {
        let output = std::process::Command::new("security")
            .args([
                "find-generic-password",
                "-a",
                account,
                "-s",
                "Claude Safe Storage",
                "-w",
            ])
            .output()
            .map_err(|e| format!("读取 Claude Safe Storage Keychain 失败: {}", e))?;
        if output.status.success() {
            let password = String::from_utf8_lossy(&output.stdout)
                .trim_end_matches(['\r', '\n'])
                .to_string();
            if !password.is_empty() {
                return Ok(password);
            }
        }
    }
    Err("未找到 Claude Safe Storage Keychain 密钥，无法解密 Claude Desktop Cookies。".to_string())
}

#[cfg(target_os = "macos")]
fn decrypt_chromium_v10_cookie(
    host_key: &str,
    encrypted_value: &[u8],
    password: &str,
) -> Result<String, String> {
    const V10_PREFIX: &[u8] = b"v10";
    if !encrypted_value.starts_with(V10_PREFIX) {
        return Err("Claude Desktop Cookie 使用了暂不支持的加密格式。".to_string());
    }
    let mut key = [0u8; 16];
    pbkdf2_hmac::<Sha1>(password.as_bytes(), b"saltysalt", 1003, &mut key);
    let iv = [0x20u8; 16];
    let mut buffer = encrypted_value[V10_PREFIX.len()..].to_vec();
    let cipher = Aes128CbcDec::new_from_slices(&key, &iv)
        .map_err(|e| format!("初始化 Claude Cookie 解密器失败: {}", e))?;
    let mut plaintext = cipher
        .decrypt_padded_mut::<Pkcs7>(&mut buffer)
        .map_err(|e| format!("解密 Claude Desktop Cookie 失败: {}", e))?
        .to_vec();

    // Chromium DB schema >= 24 prefixes encrypted cookie plaintext with SHA256(host_key).
    let host_digest = Sha256::digest(host_key.as_bytes());
    if plaintext.len() > 32 && plaintext[..32] == host_digest[..] {
        plaintext = plaintext[32..].to_vec();
    }

    if plaintext.iter().any(|byte| !(0x20..=0x7e).contains(byte)) {
        return Err("解密后的 Claude Desktop Cookie 含有非法字符。".to_string());
    }
    String::from_utf8(plaintext).map_err(|e| format!("解析 Claude Desktop Cookie 失败: {}", e))
}

#[cfg(target_os = "macos")]
fn read_decrypted_desktop_cookie_export(
    profile_dir: &Path,
) -> Result<ClaudeDesktopAuthCookieExport, String> {
    let cookies_path = profile_dir.join("Cookies");
    if !cookies_path.exists() {
        return Err(format!(
            "Claude Desktop Cookies 不存在: {}",
            cookies_path.display()
        ));
    }
    let password = read_claude_safe_storage_password()?;
    let conn = Connection::open_with_flags(&cookies_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| {
            format!(
                "读取 Claude Desktop Cookies 失败: path={}, error={}",
                cookies_path.display(),
                e
            )
        })?;
    let mut stmt = conn
        .prepare(
            "select host_key, path, name, value, encrypted_value, expires_utc, is_secure, is_httponly \
             from cookies \
             where (host_key like '%claude.ai' or host_key like '%claude.com') \
             and (length(value) > 0 or length(encrypted_value) > 0)",
        )
        .map_err(|e| format!("查询 Claude Desktop Cookies 失败: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })
        .map_err(|e| format!("读取 Claude Desktop Cookies 失败: {}", e))?;

    let mut cookies = Vec::new();
    for row in rows {
        let (domain, path, name, value, encrypted_value, expires_utc, is_secure, is_httponly) =
            row.map_err(|e| format!("读取 Claude Desktop Cookie 行失败: {}", e))?;
        if !is_claude_cookie_domain(&domain) {
            continue;
        }
        let cookie_value = if !value.is_empty() {
            value
        } else if !encrypted_value.is_empty() {
            decrypt_chromium_v10_cookie(&domain, &encrypted_value, &password)?
        } else {
            String::new()
        };
        if cookie_value.is_empty() {
            continue;
        }
        cookies.push(ClaudeDesktopAuthCookie {
            name,
            value: cookie_value,
            domain,
            path,
            secure: is_secure != 0,
            http_only: is_httponly != 0,
            expiration_date: chromium_cookie_expires_utc_to_unix_ms(expires_utc)
                .map(|ms| ms as f64 / 1000.0),
        });
    }
    let export = ClaudeDesktopAuthCookieExport {
        cookies,
        web_profile: None,
    };
    ensure_desktop_auth_export_logged_in(&export)?;
    Ok(export)
}

fn is_claude_cookie_domain(domain: &str) -> bool {
    let domain = domain.trim().trim_start_matches('.').to_ascii_lowercase();
    domain == "claude.ai"
        || domain.ends_with(".claude.ai")
        || domain == "claude.com"
        || domain.ends_with(".claude.com")
}

fn exported_cookie_host_key(cookie: &ClaudeDesktopAuthCookie) -> String {
    let domain = cookie.domain.trim();
    if domain.is_empty() {
        return "claude.ai".to_string();
    }
    domain.to_string()
}

fn exported_cookie_path(cookie: &ClaudeDesktopAuthCookie) -> &str {
    let path = cookie.path.trim();
    if path.is_empty() {
        "/"
    } else {
        path
    }
}

fn ensure_desktop_auth_export_logged_in(
    export: &ClaudeDesktopAuthCookieExport,
) -> Result<(), String> {
    let has_session_key = export.cookies.iter().any(|cookie| {
        cookie.name == "sessionKey"
            && !cookie.value.is_empty()
            && is_claude_cookie_domain(&cookie.domain)
    });
    let has_last_active_org = export.cookies.iter().any(|cookie| {
        cookie.name == "lastActiveOrg"
            && !cookie.value.is_empty()
            && is_claude_cookie_domain(&cookie.domain)
    });
    if !has_session_key || !has_last_active_org {
        return Err("未检测到 Claude Desktop 登录态，请在授权窗口完成登录后再导入。".to_string());
    }
    Ok(())
}

fn wait_for_desktop_auth_export_logged_in(
    profile_dir: &Path,
) -> Result<ClaudeDesktopAuthCookieExport, String> {
    let started_at = Instant::now();
    let timeout = Duration::from_secs(CLAUDE_DESKTOP_AUTH_EXPORT_WAIT_SECONDS);
    let mut last_error =
        "未检测到 Claude Desktop 登录态，请在授权窗口完成登录后再导入。".to_string();

    while started_at.elapsed() <= timeout {
        match read_desktop_auth_cookie_export(profile_dir)
            .and_then(|export| ensure_desktop_auth_export_logged_in(&export).map(|_| export))
        {
            Ok(export) => return Ok(export),
            Err(error) => {
                last_error = error;
                std::thread::sleep(Duration::from_millis(400));
            }
        }
    }

    Err(last_error)
}

fn wait_for_desktop_web_profile_export(
    profile_dir: &Path,
    timeout: Duration,
) -> Result<ClaudeDesktopAuthCookieExport, String> {
    let started_at = Instant::now();
    let mut last_error = "未读取到 Claude Desktop 账号资料".to_string();

    while started_at.elapsed() <= timeout {
        match read_desktop_auth_cookie_export(profile_dir)
            .and_then(|export| ensure_desktop_auth_export_logged_in(&export).map(|_| export))
        {
            Ok(export) if export.web_profile.is_some() => return Ok(export),
            Ok(_) => {
                last_error = "Claude Desktop 登录态已读取，但资料接口未返回数据".to_string();
            }
            Err(error) => {
                last_error = error;
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    Err(last_error)
}

fn desktop_web_profile_has_usage_error(profile: &Value) -> bool {
    profile
        .get("errors")
        .and_then(|value| value.as_object())
        .and_then(|errors| errors.get("organizationUsage"))
        .is_some()
}

fn write_desktop_cookie_probe_file(
    path: &Path,
    export: &ClaudeDesktopAuthCookieExport,
) -> Result<(), String> {
    let content = serde_json::to_string_pretty(export)
        .map_err(|e| format!("序列化 Claude Desktop Cookie 探测文件失败: {}", e))?;
    atomic_write::write_string_atomic(path, &content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn probe_desktop_web_profile_with_decrypted_cookies(profile_dir: &Path) -> Result<Value, String> {
    let cookie_export = read_decrypted_desktop_cookie_export(profile_dir)?;
    let probe_root = get_desktop_login_root_dir()?
        .join(format!("cookie_probe_{}", generate_random_url_token(18)));
    let user_data_dir = probe_root.join("profile");
    let status_file = user_data_dir.join(CLAUDE_DESKTOP_AUTH_STATUS_FILE);
    let export_file = user_data_dir.join(CLAUDE_DESKTOP_AUTH_EXPORT_FILE);
    let cookie_file = probe_root.join(CLAUDE_DESKTOP_COOKIE_EXPORT_FILE);
    fs::create_dir_all(&user_data_dir)
        .map_err(|e| format!("创建 Claude Desktop Cookie 探测目录失败: {}", e))?;
    let result = (|| {
        write_desktop_cookie_probe_file(&cookie_file, &cookie_export)?;
        let helper_pid = launch_platform_desktop_auth_helper_with_args(
            &user_data_dir,
            &status_file,
            &export_file,
            "cookie_probe",
            &[("--cookie-file", cookie_file.as_path())],
        )?;
        let result = wait_for_desktop_web_profile_export(&user_data_dir, Duration::from_secs(24))
            .and_then(|export| {
                export
                    .web_profile
                    .ok_or_else(|| "Claude Desktop 资料接口未返回数据".to_string())
            });
        terminate_desktop_auth_helper(Some(helper_pid));
        result
    })();
    let _ = remove_path_if_exists(&probe_root);
    result
}

#[cfg(not(target_os = "macos"))]
fn probe_desktop_web_profile_with_decrypted_cookies(_profile_dir: &Path) -> Result<Value, String> {
    Err("当前平台不支持解密 Claude Desktop Cookies。".to_string())
}

fn probe_desktop_web_profile(profile_dir: &Path) -> Result<Value, String> {
    ensure_desktop_profile_logged_in(profile_dir)?;
    let status_file = profile_dir.join("claude_desktop_profile_probe_status.json");
    let export_file = desktop_auth_export_path(profile_dir);
    let _ = remove_path_if_exists(&status_file);
    let _ = remove_path_if_exists(&export_file);
    let helper_pid =
        launch_platform_desktop_auth_helper(profile_dir, &status_file, &export_file, "probe")?;
    let result = wait_for_desktop_web_profile_export(profile_dir, Duration::from_secs(18))
        .and_then(|export| {
            export
                .web_profile
                .ok_or_else(|| "Claude Desktop 资料接口未返回数据".to_string())
        });
    terminate_desktop_auth_helper(Some(helper_pid));
    match result {
        Ok(profile)
            if desktop_web_usage_to_quota(&profile).is_some()
                || !desktop_web_profile_has_usage_error(&profile) =>
        {
            Ok(profile)
        }
        Ok(profile) => match probe_desktop_web_profile_with_decrypted_cookies(profile_dir) {
            Ok(fallback) => Ok(fallback),
            Err(error) => {
                logger::log_warn(&format!(
                    "[Claude Desktop] Cookie 页面上下文刷新失败，保留原始资料结果: {}",
                    error
                ));
                Ok(profile)
            }
        },
        Err(error) => match probe_desktop_web_profile_with_decrypted_cookies(profile_dir) {
            Ok(fallback) => Ok(fallback),
            Err(fallback_error) => Err(format!(
                "{}；Cookie 页面上下文刷新也失败: {}",
                error, fallback_error
            )),
        },
    }
}

fn rewrite_desktop_cookies_with_exported_plaintext(
    profile_dir: &Path,
    export: &ClaudeDesktopAuthCookieExport,
) -> Result<(), String> {
    ensure_desktop_auth_export_logged_in(&export)?;
    let cookies_path = profile_dir.join("Cookies");
    if !cookies_path.exists() {
        return Err(format!(
            "Claude Desktop Cookies 不存在: {}",
            cookies_path.display()
        ));
    }

    let conn = Connection::open_with_flags(
        &cookies_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| {
        format!(
            "打开 Claude Desktop Cookies 失败: path={}, error={}",
            cookies_path.display(),
            e
        )
    })?;
    let empty_encrypted_value: Vec<u8> = Vec::new();
    let mut updated_required_names = HashSet::new();
    for cookie in export
        .cookies
        .iter()
        .filter(|cookie| !cookie.value.is_empty() && is_claude_cookie_domain(&cookie.domain))
    {
        let host_key = exported_cookie_host_key(cookie);
        let cookie_path = exported_cookie_path(cookie);
        let updated_count = conn
            .execute(
                "update cookies set value = ?1, encrypted_value = ?2 \
             where host_key = ?3 and name = ?4 and path = ?5",
                params![
                    cookie.value.as_str(),
                    empty_encrypted_value.as_slice(),
                    host_key.as_str(),
                    cookie.name.as_str(),
                    cookie_path
                ],
            )
            .map_err(|e| format!("写入 Claude Desktop plaintext cookie 失败: {}", e))?;
        if updated_count > 0
            && CLAUDE_DESKTOP_REQUIRED_COOKIE_NAMES
                .iter()
                .any(|name| *name == cookie.name)
        {
            updated_required_names.insert(cookie.name.as_str());
        }
    }

    let missing_required_names = CLAUDE_DESKTOP_REQUIRED_COOKIE_NAMES
        .iter()
        .filter(|name| !updated_required_names.contains(**name))
        .copied()
        .collect::<Vec<_>>();
    if !missing_required_names.is_empty() {
        return Err(format!(
            "Claude Desktop Cookies 写入不完整，缺少: {}",
            missing_required_names.join(", ")
        ));
    }
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|e| format!("读取路径信息失败: path={}, error={}", path.display(), e))?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
    .map_err(|e| format!("删除旧路径失败: path={}, error={}", path.display(), e))
}

fn copy_path_overwrite(src: &Path, dst: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(src)
        .map_err(|e| format!("读取源路径失败: path={}, error={}", src.display(), e))?;
    if metadata.is_dir() {
        remove_path_if_exists(dst)?;
        fs::create_dir_all(dst)
            .map_err(|e| format!("创建目标目录失败: path={}, error={}", dst.display(), e))?;
        for entry in fs::read_dir(src)
            .map_err(|e| format!("读取源目录失败: path={}, error={}", src.display(), e))?
        {
            let entry = entry.map_err(|e| format!("读取目录项失败: {}", e))?;
            let file_name = entry.file_name();
            if file_name == "LOCK" {
                continue;
            }
            copy_path_overwrite(&entry.path(), &dst.join(file_name))?;
        }
        return Ok(());
    }

    if metadata.is_file() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!("创建目标父目录失败: path={}, error={}", parent.display(), e)
            })?;
        }
        remove_path_if_exists(dst)?;
        fs::copy(src, dst).map_err(|e| {
            format!(
                "复制文件失败: from={}, to={}, error={}",
                src.display(),
                dst.display(),
                e
            )
        })?;
    }
    Ok(())
}

fn copy_desktop_profile_snapshot(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("创建 Claude Desktop 快照目录失败: {}", e))?;
    for item in CLAUDE_DESKTOP_PROFILE_ITEMS {
        let source = src.join(item);
        if !source.exists() {
            continue;
        }
        copy_path_overwrite(&source, &dst.join(item))?;
    }
    Ok(())
}

fn merge_desktop_config_token(
    snapshot_config_path: &Path,
    target_config_path: &Path,
) -> Result<(), String> {
    if !snapshot_config_path.exists() {
        return Ok(());
    }
    let snapshot = read_config_file(snapshot_config_path)?.unwrap_or_else(|| json!({}));
    let Some(token_cache) = snapshot.get("oauth:tokenCache").cloned() else {
        return Ok(());
    };
    let mut target = read_config_file(target_config_path)?.unwrap_or_else(|| json!({}));
    if !target.is_object() {
        target = json!({});
    }
    let object = target
        .as_object_mut()
        .ok_or_else(|| "Claude Desktop config.json 结构非法".to_string())?;
    object.insert("oauth:tokenCache".to_string(), token_cache);
    write_config_file(target_config_path, &target)
}

fn restore_desktop_profile_snapshot(snapshot_dir: &Path, target_dir: &Path) -> Result<(), String> {
    if !snapshot_dir.exists() {
        return Err(format!(
            "Claude Desktop 快照目录不存在: {}",
            snapshot_dir.display()
        ));
    }
    fs::create_dir_all(target_dir)
        .map_err(|e| format!("创建 Claude Desktop profile 目录失败: {}", e))?;
    for item in CLAUDE_DESKTOP_PROFILE_ITEMS {
        let source = snapshot_dir.join(item);
        if !source.exists() {
            continue;
        }
        if *item == "config.json" {
            merge_desktop_config_token(&source, &target_dir.join(item))?;
        } else {
            copy_path_overwrite(&source, &target_dir.join(item))?;
        }
    }
    Ok(())
}

pub fn restore_desktop_account_to_profile(
    account_id: &str,
    target_dir: &Path,
    backup_existing: bool,
) -> Result<(), String> {
    let account = load_account(account_id).ok_or_else(|| "Claude 账号不存在".to_string())?;
    if account.auth_mode != ClaudeAuthMode::DesktopOAuth {
        return Err("绑定账号不是 Claude Desktop 登录态，无法写入 Desktop profile。".to_string());
    }
    let snapshot_dir = account
        .desktop_profile_dir
        .as_deref()
        .and_then(|value| normalize_non_empty(Some(value)))
        .map(PathBuf::from)
        .ok_or_else(|| "Claude Desktop 账号缺少 profile 快照".to_string())?;

    if backup_existing {
        let _backup_dir = backup_current_desktop_profile(target_dir)?;
    }
    restore_desktop_profile_snapshot(&snapshot_dir, target_dir)?;

    let mut updated = account.clone();
    updated.last_used = now_ts_ms();
    save_account_and_index(updated)?;
    Ok(())
}

pub fn restore_desktop_account_to_default_profile(account_id: &str) -> Result<(), String> {
    let target_dir = get_default_claude_desktop_user_data_dir()?;
    quit_claude_desktop_for_profile_write()?;
    restore_desktop_account_to_profile(account_id, &target_dir, true)
}

fn backup_current_desktop_profile(target_dir: &Path) -> Result<Option<PathBuf>, String> {
    if !target_dir.exists() {
        return Ok(None);
    }
    let backup_dir = get_desktop_backups_dir()?.join(format!("{}", now_ts_ms()));
    copy_desktop_profile_snapshot(target_dir, &backup_dir)?;
    Ok(Some(backup_dir))
}

fn find_desktop_auth_helper_script() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join(CLAUDE_DESKTOP_AUTH_HELPER_SCRIPT));
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut current = exe.parent();
        while let Some(dir) = current {
            candidates.push(dir.join(CLAUDE_DESKTOP_AUTH_HELPER_SCRIPT));
            current = dir.parent();
        }
    }
    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| {
            format!(
                "未找到 Claude Desktop 授权 helper 脚本，请确认 {} 存在。",
                CLAUDE_DESKTOP_AUTH_HELPER_SCRIPT
            )
        })
}

fn find_electron_executable_for_desktop_auth() -> Result<PathBuf, String> {
    if let Ok(value) = std::env::var("CLAUDE_DESKTOP_AUTH_ELECTRON") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if path.exists() {
                return Ok(path);
            }
        }
    }

    let bin_name = if cfg!(target_os = "windows") {
        "electron.cmd"
    } else {
        "electron"
    };
    let mut candidates = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join("node_modules").join(".bin").join(bin_name));
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut current = exe.parent();
        while let Some(dir) = current {
            candidates.push(dir.join("node_modules").join(".bin").join(bin_name));
            current = dir.parent();
        }
    }

    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| {
            "未找到 Electron 运行时，无法在平台内打开 Claude Desktop 授权窗口。请先执行 npm install，或设置 CLAUDE_DESKTOP_AUTH_ELECTRON。"
                .to_string()
        })
}

fn launch_platform_desktop_auth_helper(
    user_data_dir: &Path,
    status_file: &Path,
    export_file: &Path,
    mode: &str,
) -> Result<u32, String> {
    launch_platform_desktop_auth_helper_with_args(
        user_data_dir,
        status_file,
        export_file,
        mode,
        &[],
    )
}

fn launch_platform_desktop_auth_helper_with_args(
    user_data_dir: &Path,
    status_file: &Path,
    export_file: &Path,
    mode: &str,
    extra_args: &[(&str, &Path)],
) -> Result<u32, String> {
    let helper_script = find_desktop_auth_helper_script()?;
    let electron = find_electron_executable_for_desktop_auth()?;
    let mut command = std::process::Command::new(electron);
    command
        .arg(helper_script)
        .arg("--user-data-dir")
        .arg(user_data_dir)
        .arg("--status-file")
        .arg(status_file)
        .arg("--export-file")
        .arg(export_file)
        .arg("--mode")
        .arg(mode)
        .arg("--url")
        .arg(if mode == "cookie_probe" {
            "https://claude.ai/settings/usage"
        } else {
            "https://claude.ai/"
        })
        .arg("--probe-timeout-ms")
        .arg("15000")
        .env("ELECTRON_DISABLE_SECURITY_WARNINGS", "true")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    for (name, path) in extra_args {
        command.arg(name).arg(path);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let child = command
        .spawn()
        .map_err(|e| format!("启动 Claude Desktop 授权窗口失败: {}", e))?;
    Ok(child.id())
}

fn terminate_desktop_auth_helper(pid: Option<u32>) {
    let Some(pid) = pid else {
        return;
    };
    #[cfg(unix)]
    {
        unsafe {
            let _ = libc::kill(pid as i32, libc::SIGTERM);
        }
    }
    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("taskkill");
        command.args(["/PID", &pid.to_string(), "/T", "/F"]);
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        let _ = command.output();
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
}

#[cfg(target_os = "macos")]
fn is_claude_desktop_running() -> bool {
    std::process::Command::new("pgrep")
        .args(["-x", "Claude"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn is_claude_desktop_running() -> bool {
    false
}

#[cfg(target_os = "macos")]
fn quit_claude_desktop_for_profile_write() -> Result<(), String> {
    if !is_claude_desktop_running() {
        return Ok(());
    }
    let _ = std::process::Command::new("osascript")
        .args([
            "-e",
            &format!(
                "tell application id \"{}\" to quit",
                CLAUDE_DESKTOP_BUNDLE_ID_MACOS
            ),
        ])
        .output();
    for _ in 0..30 {
        if !is_claude_desktop_running() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    Err("Claude Desktop 仍在运行，无法安全写入登录态。请先退出 Claude 后重试。".to_string())
}

#[cfg(not(target_os = "macos"))]
fn quit_claude_desktop_for_profile_write() -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn launch_default_claude_desktop() {
    let _ = std::process::Command::new("open")
        .args(["-b", CLAUDE_DESKTOP_BUNDLE_ID_MACOS])
        .spawn();
}

#[cfg(not(target_os = "macos"))]
fn launch_default_claude_desktop() {}

fn import_desktop_profile_snapshot(
    source_dir: &Path,
    account_name: Option<&str>,
    source: &str,
) -> Result<ClaudeAccount, String> {
    ensure_desktop_profile_logged_in(source_dir)?;
    let label = desktop_account_display_name(account_name);
    let id = build_desktop_account_id(&label);
    let snapshot_dir = get_desktop_profiles_dir()?.join(&id);
    let metadata = desktop_profile_metadata(source_dir, source)?;
    remove_path_if_exists(&snapshot_dir)?;
    copy_desktop_profile_snapshot(source_dir, &snapshot_dir)?;

    let now = now_ts_ms();
    let desktop_profile = desktop_profile_metadata_json(&metadata, &snapshot_dir, now);
    let mut account = ClaudeAccount {
        id: id.clone(),
        email: label,
        auth_mode: ClaudeAuthMode::DesktopOAuth,
        account_uuid: None,
        organization_uuid: metadata.last_active_org.clone(),
        organization_name: None,
        plan_type: None,
        avatar_url: None,
        profile_updated_at: None,
        quota: None,
        quota_error: None,
        usage_updated_at: None,
        status: None,
        status_reason: None,
        api_key: None,
        api_base_url: None,
        api_provider_id: None,
        api_provider_name: None,
        api_provider_source_tag: None,
        api_provider_website: None,
        api_provider_api_key_url: None,
        api_key_field: None,
        api_model_catalog: None,
        api_extra_env: None,
        desktop_profile_dir: Some(snapshot_dir.to_string_lossy().to_string()),
        desktop_profile_imported_at: Some(now),
        claude_credentials_raw: Some(json!({
            "authMode": "desktop_oauth",
            "profileSnapshot": true,
            "source": metadata.source,
        })),
        claude_config_raw: Some(json!({
            "desktopProfile": desktop_profile
        })),
        claude_usage_raw: None,
        tags: None,
        account_note: None,
        created_at: now,
        last_used: now,
    };
    let local_profile_applied = apply_desktop_local_profile(&mut account, &snapshot_dir);
    let mut profile_error = None;
    let web_profile =
        metadata
            .web_profile
            .clone()
            .or_else(|| match probe_desktop_web_profile(&snapshot_dir) {
                Ok(profile) => Some(profile),
                Err(error) => {
                    logger::log_warn(&format!(
                        "[Claude Desktop] 导入后自动刷新账号资料失败，已保留本地快照: {}",
                        error
                    ));
                    profile_error = Some(format!("Claude Desktop 资料刷新失败: {}", error));
                    None
                }
            });
    if let Some(web_profile) = web_profile.as_ref() {
        if apply_desktop_web_profile(&mut account, web_profile) {
            account.status_reason = None;
        } else {
            account.status_reason =
                if local_profile_applied || desktop_account_has_real_profile_data(&account) {
                    None
                } else {
                    desktop_web_profile_error_message(web_profile).or_else(|| {
                        Some("Claude Desktop 资料接口未返回邮箱、头像或套餐字段。".to_string())
                    })
                };
        }
    } else if profile_error.is_some()
        && !local_profile_applied
        && !desktop_account_has_real_profile_data(&account)
    {
        account.status_reason = profile_error;
    }
    save_account_and_index(account)
}

pub fn import_desktop_from_local(account_name: Option<&str>) -> Result<ClaudeAccount, String> {
    let was_running = is_claude_desktop_running();
    let source_dir = get_default_claude_desktop_user_data_dir()?;
    quit_claude_desktop_for_profile_write()?;
    let result = import_desktop_profile_snapshot(&source_dir, account_name, "local_desktop");
    if was_running {
        launch_default_claude_desktop();
    }
    result
}

pub fn import_cli_from_local() -> Result<ClaudeAccount, String> {
    let config_dir = get_default_claude_code_config_dir()?;
    let credentials_raw = read_claude_code_credentials(&config_dir);
    if credentials_oauth(&credentials_raw).is_none() {
        return Err(
            "未找到本机 Claude Code 登录信息，请先在 Claude Code 完成 OAuth 登录。".to_string(),
        );
    }

    let config_path = get_claude_code_global_config_path(&config_dir)?;
    let config_raw = read_config_file(&config_path)?
        .ok_or_else(|| format!("未找到本机 Claude Code 配置文件: {}", config_path.display()))?;
    if config_oauth_account(&config_raw).is_none() {
        return Err(
            "本机 Claude Code 配置缺少 oauthAccount，请先在 Claude Code 完成登录。".to_string(),
        );
    }

    upsert_account_from_snapshots(credentials_raw, config_raw)
}

pub fn start_desktop_login() -> Result<ClaudeDesktopLoginStartResponse, String> {
    let _ = cancel_desktop_login(None);
    let login_id = generate_random_url_token(18);
    let user_data_dir = get_desktop_login_root_dir()?.join(&login_id);
    let status_file = user_data_dir.join(CLAUDE_DESKTOP_AUTH_STATUS_FILE);
    let export_file = user_data_dir.join(CLAUDE_DESKTOP_AUTH_EXPORT_FILE);
    remove_path_if_exists(&user_data_dir)?;
    fs::create_dir_all(&user_data_dir)
        .map_err(|e| format!("创建 Claude Desktop 登录 profile 失败: {}", e))?;
    let helper_pid =
        launch_platform_desktop_auth_helper(&user_data_dir, &status_file, &export_file, "auth")?;
    let pending = PendingClaudeDesktopLoginState {
        login_id,
        user_data_dir,
        status_file,
        export_file,
        helper_pid: Some(helper_pid),
        expires_at: now_ts() + CLAUDE_DESKTOP_LOGIN_TIMEOUT_SECONDS,
        cancelled: false,
    };
    set_pending_desktop_login(Some(pending.clone()));
    Ok(to_desktop_login_start_response(&pending))
}

pub fn complete_desktop_login(
    login_id: &str,
    account_name: Option<&str>,
) -> Result<ClaudeAccount, String> {
    let pending = get_pending_desktop_login_for(login_id)?;
    let export = wait_for_desktop_auth_export_logged_in(&pending.user_data_dir)?;
    terminate_desktop_auth_helper(pending.helper_pid);
    rewrite_desktop_cookies_with_exported_plaintext(&pending.user_data_dir, &export)?;
    let account =
        import_desktop_profile_snapshot(&pending.user_data_dir, account_name, "platform_login")?;
    clear_pending_desktop_login_if_matches(login_id);
    let _ = remove_path_if_exists(&pending.user_data_dir);
    Ok(account)
}

pub fn cancel_desktop_login(login_id: Option<&str>) -> Result<(), String> {
    if let Some(login_id) = login_id.and_then(|value| normalize_non_empty(Some(value))) {
        hydrate_pending_desktop_login_if_missing();
        let state = CLAUDE_PENDING_DESKTOP_LOGIN
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().cloned())
            .filter(|state| state.login_id == login_id);
        clear_pending_desktop_login_if_matches(&login_id);
        if let Some(state) = state {
            terminate_desktop_auth_helper(state.helper_pid);
            let _ = remove_path_if_exists(&state.user_data_dir);
        }
        return Ok(());
    }
    hydrate_pending_desktop_login_if_missing();
    if let Some(state) = CLAUDE_PENDING_DESKTOP_LOGIN
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().cloned())
    {
        terminate_desktop_auth_helper(state.helper_pid);
        let _ = remove_path_if_exists(&state.user_data_dir);
    }
    set_pending_desktop_login(None);
    Ok(())
}

fn parse_import_item(value: &Value) -> Result<ClaudeAccount, String> {
    if value
        .get("auth_mode")
        .or_else(|| value.get("authMode"))
        .and_then(|item| item.as_str())
        .map(|mode| mode.eq_ignore_ascii_case("api_key") || mode.eq_ignore_ascii_case("apikey"))
        .unwrap_or(false)
    {
        if let Some(api_key) = value
            .get("api_key")
            .or_else(|| value.get("apiKey"))
            .or_else(|| value.get("anthropicApiKey"))
            .and_then(|item| item.as_str())
        {
            let account_name = value
                .get("email")
                .or_else(|| value.get("accountName"))
                .or_else(|| value.get("name"))
                .and_then(|item| item.as_str());
            let provider_value = value
                .get("apiProvider")
                .or_else(|| value.get("api_provider"))
                .or_else(|| {
                    value
                        .get("claude_credentials_raw")
                        .and_then(|item| item.get("apiProvider"))
                });
            let api_model_catalog = value
                .get("api_model_catalog")
                .or_else(|| value.get("apiModelCatalog"))
                .or_else(|| provider_value.and_then(|provider| provider.get("modelCatalog")))
                .and_then(|item| item.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(ToString::to_string))
                        .collect::<Vec<_>>()
                });
            let api_extra_env = value
                .get("api_extra_env")
                .or_else(|| value.get("apiExtraEnv"))
                .or_else(|| provider_value.and_then(|provider| provider.get("extraEnv")))
                .and_then(|item| item.as_object())
                .map(|object| {
                    object
                        .iter()
                        .filter_map(|(key, value)| {
                            value.as_str().map(|value| (key.clone(), value.to_string()))
                        })
                        .collect::<BTreeMap<_, _>>()
                });
            return import_api_key(
                api_key,
                account_name,
                ClaudeApiKeyProviderConfig {
                    api_base_url: value
                        .get("api_base_url")
                        .or_else(|| value.get("apiBaseUrl"))
                        .or_else(|| provider_value.and_then(|provider| provider.get("baseUrl")))
                        .and_then(|item| item.as_str())
                        .map(ToString::to_string),
                    api_provider_id: value
                        .get("api_provider_id")
                        .or_else(|| value.get("apiProviderId"))
                        .or_else(|| provider_value.and_then(|provider| provider.get("id")))
                        .and_then(|item| item.as_str())
                        .map(ToString::to_string),
                    api_provider_name: value
                        .get("api_provider_name")
                        .or_else(|| value.get("apiProviderName"))
                        .or_else(|| provider_value.and_then(|provider| provider.get("name")))
                        .and_then(|item| item.as_str())
                        .map(ToString::to_string),
                    api_provider_source_tag: value
                        .get("api_provider_source_tag")
                        .or_else(|| value.get("apiProviderSourceTag"))
                        .or_else(|| provider_value.and_then(|provider| provider.get("sourceTag")))
                        .and_then(|item| item.as_str())
                        .map(ToString::to_string),
                    api_provider_website: value
                        .get("api_provider_website")
                        .or_else(|| value.get("apiProviderWebsite"))
                        .or_else(|| provider_value.and_then(|provider| provider.get("website")))
                        .and_then(|item| item.as_str())
                        .map(ToString::to_string),
                    api_provider_api_key_url: value
                        .get("api_provider_api_key_url")
                        .or_else(|| value.get("apiProviderApiKeyUrl"))
                        .or_else(|| provider_value.and_then(|provider| provider.get("apiKeyUrl")))
                        .and_then(|item| item.as_str())
                        .map(ToString::to_string),
                    api_key_field: value
                        .get("api_key_field")
                        .or_else(|| value.get("apiKeyField"))
                        .or_else(|| provider_value.and_then(|provider| provider.get("keyField")))
                        .and_then(|item| item.as_str())
                        .map(ToString::to_string),
                    api_model_catalog,
                    api_extra_env,
                },
            );
        }
    }

    if let Some(id) = value.get("id").and_then(|item| item.as_str()) {
        if value.get("claude_credentials_raw").is_some()
            || value.get("claudeCredentialsRaw").is_some()
        {
            let mut account: ClaudeAccount = serde_json::from_value(value.clone())
                .map_err(|e| format!("解析 Claude 账号失败: {}", e))?;
            if account.id.trim().is_empty() {
                account.id = id.to_string();
            }
            return save_account_and_index(account);
        }
    }

    let credentials_raw = value
        .get("claude_credentials_raw")
        .or_else(|| value.get("claudeCredentialsRaw"))
        .or_else(|| value.get("credentials"))
        .cloned()
        .unwrap_or_else(|| {
            if value.get("claudeAiOauth").is_some() {
                value.clone()
            } else {
                Value::Null
            }
        });
    let config_raw = value
        .get("claude_config_raw")
        .or_else(|| value.get("claudeConfigRaw"))
        .or_else(|| value.get("config"))
        .cloned()
        .unwrap_or_else(|| {
            if value.get("oauthAccount").is_some() {
                value.clone()
            } else {
                Value::Null
            }
        });
    upsert_account_from_snapshots(credentials_raw, config_raw)
}

pub fn import_from_json(json_content: &str) -> Result<Vec<ClaudeAccount>, String> {
    let value: Value =
        serde_json::from_str(json_content).map_err(|e| format!("解析 JSON 失败: {}", e))?;
    if let Some(arr) = value.as_array() {
        return arr.iter().map(parse_import_item).collect();
    }
    if let Some(arr) = value.get("accounts").and_then(|item| item.as_array()) {
        return arr.iter().map(parse_import_item).collect();
    }
    Ok(vec![parse_import_item(&value)?])
}

pub fn start_oauth_login() -> Result<ClaudeOAuthStartResponse, String> {
    let login_id = generate_random_url_token(18);
    let state = generate_random_url_token(32);
    let code_verifier = generate_random_url_token(32);
    let code_challenge = generate_pkce_challenge(&code_verifier);
    let auth_url = build_oauth_authorize_url(&state, &code_challenge)?;
    let pending = PendingClaudeOAuthState {
        login_id,
        state,
        code_verifier,
        auth_url,
        expires_at: now_ts() + CLAUDE_OAUTH_TIMEOUT_SECONDS,
        cancelled: false,
    };
    set_pending_oauth_login(Some(pending.clone()));
    Ok(to_oauth_start_response(&pending))
}

pub fn cancel_oauth_login(login_id: Option<&str>) -> Result<(), String> {
    if let Some(login_id) = login_id.and_then(|value| normalize_non_empty(Some(value))) {
        clear_pending_oauth_login_if_matches(&login_id);
        return Ok(());
    }
    set_pending_oauth_login(None);
    Ok(())
}

async fn exchange_oauth_code_for_tokens(
    state: &PendingClaudeOAuthState,
    code: &str,
) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
    let resp = client
        .post(CLAUDE_OAUTH_TOKEN_URL)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json, text/plain, */*")
        .header(USER_AGENT, "antigravity-cockpit-tools")
        .json(&json!({
            "grant_type": "authorization_code",
            "client_id": CLAUDE_OAUTH_CLIENT_ID,
            "code": code,
            "redirect_uri": CLAUDE_OAUTH_MANUAL_REDIRECT_URL,
            "code_verifier": state.code_verifier,
            "state": state.state,
        }))
        .send()
        .await
        .map_err(|e| format!("交换 Claude OAuth token 失败: {}", e))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("读取 Claude OAuth token 响应失败: {}", e))?;
    if !status.is_success() {
        return Err(format!(
            "交换 Claude OAuth token 失败: HTTP {} {}",
            status, body
        ));
    }
    serde_json::from_str(&body).map_err(|e| format!("解析 Claude OAuth token 响应失败: {}", e))
}

async fn request_oauth_profile(access_token: &str) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
    let resp = client
        .get(CLAUDE_OAUTH_PROFILE_URL)
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", access_token))
                .map_err(|e| format!("构造 Claude profile Authorization 失败: {}", e))?,
        )
        .header(CONTENT_TYPE, "application/json")
        .header(USER_AGENT, "antigravity-cockpit-tools")
        .send()
        .await
        .map_err(|e| format!("请求 Claude OAuth profile 失败: {}", e))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("读取 Claude OAuth profile 响应失败: {}", e))?;
    if !status.is_success() {
        return Err(format!(
            "请求 Claude OAuth profile 失败: HTTP {} {}",
            status, body
        ));
    }
    serde_json::from_str(&body).map_err(|e| format!("解析 Claude OAuth profile 响应失败: {}", e))
}

fn split_scope_string(scope: Option<String>) -> Vec<String> {
    scope
        .map(|value| {
            value
                .split_whitespace()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| {
            CLAUDE_OAUTH_SCOPES
                .iter()
                .map(|item| item.to_string())
                .collect()
        })
}

fn insert_string_if_present(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
    value: Option<String>,
) {
    if let Some(value) = value.and_then(|item| normalize_non_empty(Some(item.as_str()))) {
        object.insert(key.to_string(), Value::String(value));
    }
}

fn insert_bool_if_present(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
    value: Option<bool>,
) {
    if let Some(value) = value {
        object.insert(key.to_string(), Value::Bool(value));
    }
}

fn first_string(candidates: Vec<Option<String>>) -> Option<String> {
    candidates
        .into_iter()
        .flatten()
        .find_map(|value| normalize_non_empty(Some(value.as_str())))
}

fn subscription_type_from_profile(profile: Option<&Value>) -> Option<String> {
    // 对齐官方 oauth/profile 分支，只识别 4 个枚举：claude_max / claude_pro / claude_enterprise / claude_team。
    // 其它取值一律返回 None，避免产出多余原值。
    match read_string_path(profile?, &["organization", "organization_type"])?.as_str() {
        "claude_max" => Some("Max".to_string()),
        "claude_pro" => Some("Pro".to_string()),
        "claude_enterprise" => Some("Enterprise".to_string()),
        "claude_team" => Some("Team".to_string()),
        _ => None,
    }
}

fn build_oauth_snapshots(
    token_response: &Value,
    profile: Option<&Value>,
    email_hint: Option<&str>,
) -> Result<(Value, Value), String> {
    let access_token = read_string_path(token_response, &["access_token"])
        .ok_or_else(|| "Claude OAuth 响应缺少 access_token".to_string())?;
    let refresh_token = read_string_path(token_response, &["refresh_token"]);
    let expires_in = read_i64_value(token_response.get("expires_in")).unwrap_or(3600);
    let scopes = split_scope_string(read_string_path(token_response, &["scope"]));

    let account_uuid = first_string(vec![
        profile.and_then(|value| read_string_path(value, &["account", "uuid"])),
        read_string_path(token_response, &["account", "uuid"]),
    ]);
    let email = first_string(vec![
        profile.and_then(|value| read_string_path(value, &["account", "email"])),
        profile.and_then(|value| read_string_path(value, &["account", "email_address"])),
        read_string_path(token_response, &["account", "email_address"]),
        email_hint.and_then(|value| normalize_non_empty(Some(value))),
    ])
    .ok_or_else(|| "无法从 Claude OAuth 响应识别邮箱，请填写邮箱后重试".to_string())?;
    let organization_uuid = first_string(vec![
        profile.and_then(|value| read_string_path(value, &["organization", "uuid"])),
        read_string_path(token_response, &["organization", "uuid"]),
    ]);
    let organization_name = first_string(vec![
        profile.and_then(|value| read_string_path(value, &["organization", "name"])),
        profile.and_then(|value| read_string_path(value, &["organization", "display_name"])),
        read_string_path(token_response, &["organization", "name"]),
    ]);
    let display_name =
        profile.and_then(|value| read_string_path(value, &["account", "display_name"]));
    let avatar_url = first_string(vec![
        profile.and_then(|value| read_string_path(value, &["account", "avatar_url"])),
        profile.and_then(|value| read_string_path(value, &["account", "avatarUrl"])),
        read_string_path(token_response, &["account", "avatar_url"]),
    ]);
    let account_created_at =
        profile.and_then(|value| read_string_path(value, &["account", "created_at"]));
    let organization_type =
        profile.and_then(|value| read_string_path(value, &["organization", "organization_type"]));
    let billing_type =
        profile.and_then(|value| read_string_path(value, &["organization", "billing_type"]));
    let rate_limit_tier =
        profile.and_then(|value| read_string_path(value, &["organization", "rate_limit_tier"]));
    let subscription_created_at = profile
        .and_then(|value| read_string_path(value, &["organization", "subscription_created_at"]));
    let has_extra_usage_enabled = profile.and_then(|value| {
        read_bool_value(value.get("organization")?.get("has_extra_usage_enabled"))
    });
    let subscription_type = subscription_type_from_profile(profile);

    let mut credentials_oauth = serde_json::Map::new();
    credentials_oauth.insert("accessToken".to_string(), Value::String(access_token));
    if let Some(refresh_token) = refresh_token {
        credentials_oauth.insert("refreshToken".to_string(), Value::String(refresh_token));
    }
    credentials_oauth.insert(
        "expiresAt".to_string(),
        Value::Number(serde_json::Number::from(now_ts_ms() + expires_in * 1000)),
    );
    credentials_oauth.insert(
        "scopes".to_string(),
        Value::Array(scopes.into_iter().map(Value::String).collect()),
    );
    credentials_oauth.insert(
        "subscriptionType".to_string(),
        subscription_type
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    credentials_oauth.insert(
        "rateLimitTier".to_string(),
        rate_limit_tier
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    if let Some(profile) = profile {
        credentials_oauth.insert("profile".to_string(), profile.clone());
    }

    let mut oauth_account = serde_json::Map::new();
    insert_string_if_present(&mut oauth_account, "accountUuid", account_uuid);
    oauth_account.insert("emailAddress".to_string(), Value::String(email));
    insert_string_if_present(&mut oauth_account, "organizationUuid", organization_uuid);
    insert_string_if_present(&mut oauth_account, "organizationName", organization_name);
    insert_string_if_present(&mut oauth_account, "displayName", display_name);
    insert_string_if_present(&mut oauth_account, "avatarUrl", avatar_url);
    insert_bool_if_present(
        &mut oauth_account,
        "hasExtraUsageEnabled",
        has_extra_usage_enabled,
    );
    insert_string_if_present(&mut oauth_account, "billingType", billing_type);
    insert_string_if_present(&mut oauth_account, "organizationType", organization_type);
    insert_string_if_present(&mut oauth_account, "accountCreatedAt", account_created_at);
    insert_string_if_present(
        &mut oauth_account,
        "subscriptionCreatedAt",
        subscription_created_at,
    );
    insert_string_if_present(&mut oauth_account, "subscriptionType", subscription_type);
    insert_string_if_present(&mut oauth_account, "rateLimitTier", rate_limit_tier);

    let credentials = Value::Object(
        [(
            "claudeAiOauth".to_string(),
            Value::Object(credentials_oauth),
        )]
        .into_iter()
        .collect(),
    );
    let config = json!({
        "oauthAccount": Value::Object(oauth_account),
        "hasCompletedOnboarding": true,
    });
    Ok((credentials, config))
}

pub async fn complete_oauth_login(
    login_id: &str,
    callback_or_code: &str,
    email_hint: Option<&str>,
) -> Result<ClaudeAccount, String> {
    let pending = get_pending_oauth_login_for(login_id)?;
    let (code, callback_state) = parse_oauth_callback_input(callback_or_code)?;
    if let Some(callback_state) = callback_state {
        if callback_state != pending.state {
            return Err("Claude OAuth 回调 state 不匹配，请重新开始授权".to_string());
        }
    }
    let token_response = exchange_oauth_code_for_tokens(&pending, &code).await?;
    let access_token = read_string_path(&token_response, &["access_token"])
        .ok_or_else(|| "Claude OAuth 响应缺少 access_token".to_string())?;
    let profile = match request_oauth_profile(&access_token).await {
        Ok(profile) => Some(profile),
        Err(error) => {
            logger::log_warn(&format!(
                "[Claude OAuth] 获取 profile 失败，将尝试使用 token 响应或邮箱兜底: {}",
                error
            ));
            None
        }
    };
    let (credentials, config) =
        build_oauth_snapshots(&token_response, profile.as_ref(), email_hint)?;
    let account = upsert_account_from_snapshots(credentials, config)?;
    clear_pending_oauth_login_if_matches(login_id);
    Ok(account)
}

fn first_string_path_candidates(value: Option<&Value>, paths: &[&[&str]]) -> Option<String> {
    let value = value?;
    paths.iter().find_map(|path| read_string_path(value, path))
}

fn first_f64_path_candidates(value: Option<&Value>, paths: &[&[&str]]) -> Option<f64> {
    let value = value?;
    paths.iter().find_map(|path| {
        let mut current = value;
        for key in *path {
            current = current.get(*key)?;
        }
        read_f64_value(Some(current))
    })
}

fn first_i64_path_candidates(value: Option<&Value>, paths: &[&[&str]]) -> Option<i64> {
    let value = value?;
    paths.iter().find_map(|path| {
        let mut current = value;
        for key in *path {
            current = current.get(*key)?;
        }
        read_i64_value(Some(current))
    })
}

fn first_reset_path_candidates(value: Option<&Value>, paths: &[&[&str]]) -> Option<i64> {
    let value = value?;
    paths.iter().find_map(|path| {
        let mut current = value;
        for key in *path {
            current = current.get(*key)?;
        }
        parse_reset_seconds(Some(current))
    })
}

fn find_string_by_key(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(object) => {
            for key in keys {
                if let Some(found) = object
                    .get(*key)
                    .and_then(|item| normalize_non_empty(item.as_str()))
                {
                    return Some(found);
                }
            }
            object
                .values()
                .find_map(|item| find_string_by_key(item, keys))
        }
        Value::Array(items) => items.iter().find_map(|item| find_string_by_key(item, keys)),
        _ => None,
    }
}

/// 对齐官方 Claude.app `fai` 函数的 4 个枚举。
/// 只识别：free / claude_pro / claude_max / raven（raven 进一步看 isEnterprise 拆为 Team / Enterprise）。
/// 其他一律返回 None，与官方 “拿不到 paidAccountTier 则不显示” 一致。
/// 额外兼容本地 profile 中用于细分 Max 档位的 rate limit tier。
fn normalize_desktop_plan_value(value: Option<String>) -> Option<String> {
    let value = value.and_then(|item| normalize_non_empty(Some(item.as_str())))?;
    let key = value
        .trim()
        .to_ascii_lowercase()
        .replace('-', " ")
        .replace('_', " ");
    let normalized = match key.as_str() {
        "default claude max 20x" | "claude max 20x" | "max 20x" => "Max 20x",
        "default claude max 5x" | "claude max 5x" | "max 5x" => "Max 5x",
        "claude max" | "max" => "Max",
        "claude pro" | "pro" => "Pro",
        "default claude ai" | "free" | "claude free" => "Free",
        // OAuth profile organization_type 路径：claude_enterprise / claude_team
        "claude enterprise" | "enterprise" => "Enterprise",
        "claude team" | "team" => "Team",
        // 其它取值（claude_desktop、desktop、personal、individual、apple_subscription 等）一律不识别。
        _ => return None,
    };
    Some(normalized.to_string())
}

/// 从 capabilities 数组中提取小写字符串（对齐 ["chat", "claude_pro"] 这种结构）。
fn capability_strings(value: Option<&Value>) -> Vec<String> {
    let Some(items) = value.and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.as_str().map(|s| s.trim().to_ascii_lowercase()))
        .filter(|s| !s.is_empty())
        .collect()
}

/// 严格按官方 `fai(A, isEnterprise)` 函数输出：
///   - claude_max → Max
///   - claude_pro → Pro
///   - raven      → isEnterprise ? Enterprise : Team
///   - claude_free / free → Free
fn plan_from_capability_list(caps: &[String], is_enterprise: bool) -> Option<String> {
    if caps.iter().any(|c| c == "claude_max") {
        return Some("Max".to_string());
    }
    if caps.iter().any(|c| c == "claude_pro") {
        return Some("Pro".to_string());
    }
    if caps.iter().any(|c| c == "raven") {
        return Some(if is_enterprise {
            "Enterprise".to_string()
        } else {
            "Team".to_string()
        });
    }
    if caps.iter().any(|c| c == "claude_free" || c == "free") {
        return Some("Free".to_string());
    }
    None
}

/// 是否企业版：对齐官方 oauth/profile 分支，看 organization.organization_type === "claude_enterprise"。
fn is_enterprise_from_profile(profile: &Value) -> bool {
    let Some(endpoints) = profile.get("endpoints") else {
        return false;
    };
    let direct_paths: &[&[&str]] = &[
        &["accountProfile", "organization", "organization_type"],
        &["account", "organization", "organization_type"],
        &[
            "bootstrapAppStart",
            "activeOrganization",
            "organization_type",
        ],
        &[
            "bootstrapAppStart",
            "active_organization",
            "organization_type",
        ],
        &["bootstrapAppStart", "organization", "organization_type"],
    ];
    for path in direct_paths {
        if let Some(value) = read_string_path(endpoints, path) {
            if value.eq_ignore_ascii_case("claude_enterprise") {
                return true;
            }
        }
    }
    let memberships_paths: &[&[&str]] = &[
        &["bootstrapAppStart", "account", "memberships"],
        &["accountProfile", "account", "memberships"],
        &["account", "account", "memberships"],
        &["account", "memberships"],
    ];
    for path in memberships_paths {
        let mut current = endpoints;
        let mut ok = true;
        for key in *path {
            match current.get(*key) {
                Some(next) => current = next,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        let Some(memberships) = current.as_array() else {
            continue;
        };
        for membership in memberships {
            if let Some(org_type) = membership
                .get("organization")
                .and_then(|org| read_string_path(org, &["organization_type"]))
            {
                if org_type.eq_ignore_ascii_case("claude_enterprise") {
                    return true;
                }
            }
        }
    }
    false
}

fn infer_desktop_plan_from_capabilities(profile: &Value) -> Option<String> {
    let endpoints = profile.get("endpoints")?;
    let is_enterprise = is_enterprise_from_profile(profile);

    // 1) accountProfile.organization.capabilities
    let direct_paths: &[&[&str]] = &[
        &["accountProfile", "organization", "capabilities"],
        &["account", "organization", "capabilities"],
        &["bootstrapAppStart", "activeOrganization", "capabilities"],
        &["bootstrapAppStart", "active_organization", "capabilities"],
        &["bootstrapAppStart", "organization", "capabilities"],
    ];
    for path in direct_paths {
        let mut current = endpoints;
        let mut ok = true;
        for key in *path {
            match current.get(*key) {
                Some(next) => current = next,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            let caps = capability_strings(Some(current));
            if let Some(plan) = plan_from_capability_list(&caps, is_enterprise) {
                return Some(plan);
            }
        }
    }

    // 2) bootstrapAppStart.account.memberships[*].organization.capabilities
    let memberships_paths: &[&[&str]] = &[
        &["bootstrapAppStart", "account", "memberships"],
        &["accountProfile", "account", "memberships"],
        &["account", "account", "memberships"],
        &["account", "memberships"],
    ];
    for path in memberships_paths {
        let mut current = endpoints;
        let mut ok = true;
        for key in *path {
            match current.get(*key) {
                Some(next) => current = next,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        let Some(memberships) = current.as_array() else {
            continue;
        };
        let mut all_caps: Vec<String> = Vec::new();
        for membership in memberships {
            let caps = capability_strings(
                membership
                    .get("organization")
                    .and_then(|org| org.get("capabilities")),
            );
            all_caps.extend(caps);
        }
        if let Some(plan) = plan_from_capability_list(&all_caps, is_enterprise) {
            return Some(plan);
        }
    }

    None
}

fn is_desktop_plan_placeholder(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "claude desktop" | "desktop"
    )
}

fn normalize_desktop_usage_percentage(value: f64) -> i32 {
    let scaled = if value > 0.0 && value <= 1.0 {
        value * 100.0
    } else {
        value
    };
    clamp_percentage(Some(scaled))
}

// ============================================================================
// webProfile 存储瘦身
//
// 贬面背景：claude-desktop-auth-helper 会拽回 bootstrapAppStart，包含
// statsig / growthbook feature flags、system_prompts、完整 memberships 等，原始对象
// 常常能到几 MB。以前直接把整个 profile 填到 claude_usage_raw，导致账号文件 +
// 导出 JSON 极大。这里只保留上层识别实际使用的字段。
// ============================================================================

fn slim_organization_object(org: &Value) -> Value {
    let Some(obj) = org.as_object() else {
        return Value::Null;
    };
    let mut slim = serde_json::Map::new();
    for key in [
        "uuid",
        "name",
        "organization_type",
        "rate_limit_tier",
        "capabilities",
    ] {
        if let Some(v) = obj.get(key) {
            slim.insert(key.to_string(), v.clone());
        }
    }
    Value::Object(slim)
}

fn slim_membership_entry(membership: &Value) -> Value {
    let mut slim = serde_json::Map::new();
    if let Some(org) = membership.get("organization") {
        slim.insert("organization".to_string(), slim_organization_object(org));
    }
    Value::Object(slim)
}

fn slim_account_object(value: &Value) -> Option<Value> {
    let obj = value.as_object()?;
    let mut slim = serde_json::Map::new();
    for key in [
        "email_address",
        "email",
        "uuid",
        "full_name",
        "display_name",
    ] {
        if let Some(v) = obj.get(key) {
            slim.insert(key.to_string(), v.clone());
        }
    }
    if let Some(memberships) = obj.get("memberships").and_then(|v| v.as_array()) {
        let trimmed: Vec<Value> = memberships.iter().map(slim_membership_entry).collect();
        slim.insert("memberships".to_string(), Value::Array(trimmed));
    }
    Some(Value::Object(slim))
}

/// accountProfile / account 端点响应瘦身：只保留邮箱、uuid、全名、organization、嵌套 account.memberships。
fn slim_account_profile_payload(value: &Value) -> Option<Value> {
    let obj = value.as_object()?;
    let mut slim = serde_json::Map::new();
    for key in [
        "email_address",
        "email",
        "uuid",
        "full_name",
        "display_name",
    ] {
        if let Some(v) = obj.get(key) {
            slim.insert(key.to_string(), v.clone());
        }
    }
    if let Some(org) = obj.get("organization") {
        slim.insert("organization".to_string(), slim_organization_object(org));
    }
    if let Some(account) = obj.get("account") {
        if let Some(slim_account) = slim_account_object(account) {
            slim.insert("account".to_string(), slim_account);
        }
    }
    Some(Value::Object(slim))
}

/// bootstrapAppStart 瘦身：只保留 active_organization 与 account.memberships。
fn slim_bootstrap_payload(value: &Value) -> Option<Value> {
    let obj = value.as_object()?;
    let mut slim = serde_json::Map::new();
    for org_key in ["activeOrganization", "active_organization", "organization"] {
        if let Some(org) = obj.get(org_key) {
            slim.insert(org_key.to_string(), slim_organization_object(org));
        }
    }
    if let Some(account) = obj.get("account") {
        if let Some(slim_account) = slim_account_object(account) {
            slim.insert("account".to_string(), slim_account);
        }
    }
    if slim.is_empty() {
        None
    } else {
        Some(Value::Object(slim))
    }
}

/// 生成可安全写入 claude_usage_raw / 导出的 webProfile 瘦身副本。
fn slim_web_profile_for_storage(profile: &Value) -> Value {
    let mut slim = serde_json::Map::new();
    for key in ["version", "fetchContext", "fetchedAt"] {
        if let Some(v) = profile.get(key) {
            slim.insert(key.to_string(), v.clone());
        }
    }
    if let Some(errors) = profile.get("errors") {
        slim.insert("errors".to_string(), errors.clone());
    }
    if let Some(endpoints) = profile.get("endpoints").and_then(|v| v.as_object()) {
        let mut slim_endpoints = serde_json::Map::new();
        // 额度与订阅响应体量可控，原样保留（后续字段识别/展示都靠它）。
        for key in [
            "organizationUsage",
            "subscriptionDetails",
            "overageSpendLimit",
        ] {
            if let Some(v) = endpoints.get(key) {
                slim_endpoints.insert(key.to_string(), v.clone());
            }
        }
        if let Some(ap) = endpoints
            .get("accountProfile")
            .and_then(slim_account_profile_payload)
        {
            slim_endpoints.insert("accountProfile".to_string(), ap);
        }
        if let Some(acc) = endpoints
            .get("account")
            .and_then(slim_account_profile_payload)
        {
            slim_endpoints.insert("account".to_string(), acc);
        }
        if let Some(boot) = endpoints
            .get("bootstrapAppStart")
            .and_then(slim_bootstrap_payload)
        {
            slim_endpoints.insert("bootstrapAppStart".to_string(), boot);
        }
        slim.insert("endpoints".to_string(), Value::Object(slim_endpoints));
    }
    Value::Object(slim)
}

fn desktop_web_usage_to_quota(profile: &Value) -> Option<ClaudeQuota> {
    let five_hour = first_f64_path_candidates(
        Some(profile),
        &[
            &["endpoints", "organizationUsage", "five_hour", "utilization"],
            &["endpoints", "organizationUsage", "five_hour", "percentage"],
            &[
                "endpoints",
                "organizationUsage",
                "five_hour",
                "percent_used",
            ],
            &["endpoints", "organizationUsage", "fiveHour", "utilization"],
            &["endpoints", "organizationUsage", "fiveHour", "percentage"],
            &["endpoints", "organizationUsage", "fiveHour", "percentUsed"],
            &[
                "endpoints",
                "organizationUsage",
                "usage",
                "five_hour",
                "utilization",
            ],
            &[
                "endpoints",
                "organizationUsage",
                "usage",
                "fiveHour",
                "utilization",
            ],
            &[
                "endpoints",
                "organizationUsage",
                "limits",
                "five_hour",
                "utilization",
            ],
            &[
                "endpoints",
                "organizationUsage",
                "limits",
                "fiveHour",
                "utilization",
            ],
            &["endpoints", "organizationUsage", "five_hour_percentage"],
            &["endpoints", "organizationUsage", "fiveHourPercentage"],
            &["endpoints", "organizationUsage", "five_hour_utilization"],
            &["endpoints", "organizationUsage", "fiveHourUtilization"],
            &["endpoints", "organizationUsage", "five_hour_percent_used"],
            &["endpoints", "organizationUsage", "fiveHourPercentUsed"],
        ],
    );
    let seven_day = first_f64_path_candidates(
        Some(profile),
        &[
            &["endpoints", "organizationUsage", "seven_day", "utilization"],
            &["endpoints", "organizationUsage", "seven_day", "percentage"],
            &[
                "endpoints",
                "organizationUsage",
                "seven_day",
                "percent_used",
            ],
            &["endpoints", "organizationUsage", "sevenDay", "utilization"],
            &["endpoints", "organizationUsage", "sevenDay", "percentage"],
            &["endpoints", "organizationUsage", "sevenDay", "percentUsed"],
            &[
                "endpoints",
                "organizationUsage",
                "usage",
                "seven_day",
                "utilization",
            ],
            &[
                "endpoints",
                "organizationUsage",
                "usage",
                "sevenDay",
                "utilization",
            ],
            &[
                "endpoints",
                "organizationUsage",
                "limits",
                "seven_day",
                "utilization",
            ],
            &[
                "endpoints",
                "organizationUsage",
                "limits",
                "sevenDay",
                "utilization",
            ],
            &["endpoints", "organizationUsage", "seven_day_percentage"],
            &["endpoints", "organizationUsage", "sevenDayPercentage"],
            &["endpoints", "organizationUsage", "seven_day_utilization"],
            &["endpoints", "organizationUsage", "sevenDayUtilization"],
            &["endpoints", "organizationUsage", "seven_day_percent_used"],
            &["endpoints", "organizationUsage", "sevenDayPercentUsed"],
        ],
    );
    let seven_day_sonnet = first_f64_path_candidates(
        Some(profile),
        &[
            &[
                "endpoints",
                "organizationUsage",
                "seven_day_sonnet",
                "utilization",
            ],
            &[
                "endpoints",
                "organizationUsage",
                "seven_day_sonnet",
                "percentage",
            ],
            &[
                "endpoints",
                "organizationUsage",
                "seven_day_sonnet",
                "percent_used",
            ],
            &[
                "endpoints",
                "organizationUsage",
                "sevenDaySonnet",
                "utilization",
            ],
            &[
                "endpoints",
                "organizationUsage",
                "sevenDaySonnet",
                "percentage",
            ],
            &[
                "endpoints",
                "organizationUsage",
                "sevenDaySonnet",
                "percentUsed",
            ],
            &[
                "endpoints",
                "organizationUsage",
                "seven_day_sonnet_percentage",
            ],
            &["endpoints", "organizationUsage", "sevenDaySonnetPercentage"],
            &[
                "endpoints",
                "organizationUsage",
                "seven_day_sonnet_utilization",
            ],
            &[
                "endpoints",
                "organizationUsage",
                "sevenDaySonnetUtilization",
            ],
        ],
    );
    if five_hour.is_none() && seven_day.is_none() && seven_day_sonnet.is_none() {
        return None;
    }

    let extra_usage = first_f64_path_candidates(
        Some(profile),
        &[
            &[
                "endpoints",
                "organizationUsage",
                "extra_usage",
                "utilization",
            ],
            &[
                "endpoints",
                "organizationUsage",
                "extraUsage",
                "utilization",
            ],
            &["endpoints", "organizationUsage", "extra_usage_percentage"],
            &["endpoints", "organizationUsage", "extraUsagePercentage"],
            &["endpoints", "overageSpendLimit", "utilization"],
            &["endpoints", "overageSpendLimit", "percentage"],
            &["endpoints", "overageSpendLimit", "percent_used"],
            &["endpoints", "overageSpendLimit", "percentUsed"],
        ],
    );
    let extra_enabled = read_bool_value(
        profile
            .get("endpoints")
            .and_then(|value| value.get("organizationUsage"))
            .and_then(|value| value.get("extra_usage"))
            .and_then(|value| value.get("is_enabled")),
    )
    .or_else(|| {
        read_bool_value(
            profile
                .get("endpoints")
                .and_then(|value| value.get("organizationUsage"))
                .and_then(|value| value.get("extraUsage"))
                .and_then(|value| value.get("isEnabled")),
        )
    })
    .or_else(|| {
        read_bool_value(
            profile
                .get("endpoints")
                .and_then(|value| value.get("overageSpendLimit"))
                .and_then(|value| value.get("is_enabled")),
        )
    })
    .unwrap_or(extra_usage.is_some());

    let endpoints = profile.get("endpoints");
    Some(ClaudeQuota {
        five_hour_percentage: five_hour
            .map(normalize_desktop_usage_percentage)
            .unwrap_or(0),
        five_hour_reset_time: first_reset_path_candidates(
            Some(profile),
            &[
                &["endpoints", "organizationUsage", "five_hour", "resets_at"],
                &["endpoints", "organizationUsage", "five_hour", "reset_at"],
                &["endpoints", "organizationUsage", "fiveHour", "resetsAt"],
                &["endpoints", "organizationUsage", "fiveHour", "resetAt"],
                &["endpoints", "organizationUsage", "five_hour_reset_time"],
                &["endpoints", "organizationUsage", "fiveHourResetTime"],
            ],
        ),
        seven_day_percentage: seven_day
            .map(normalize_desktop_usage_percentage)
            .unwrap_or(0),
        seven_day_reset_time: first_reset_path_candidates(
            Some(profile),
            &[
                &["endpoints", "organizationUsage", "seven_day", "resets_at"],
                &["endpoints", "organizationUsage", "seven_day", "reset_at"],
                &["endpoints", "organizationUsage", "sevenDay", "resetsAt"],
                &["endpoints", "organizationUsage", "sevenDay", "resetAt"],
                &["endpoints", "organizationUsage", "seven_day_reset_time"],
                &["endpoints", "organizationUsage", "sevenDayResetTime"],
            ],
        ),
        seven_day_sonnet_percentage: seven_day_sonnet.map(normalize_desktop_usage_percentage),
        seven_day_sonnet_reset_time: first_reset_path_candidates(
            Some(profile),
            &[
                &[
                    "endpoints",
                    "organizationUsage",
                    "seven_day_sonnet",
                    "resets_at",
                ],
                &[
                    "endpoints",
                    "organizationUsage",
                    "seven_day_sonnet",
                    "reset_at",
                ],
                &[
                    "endpoints",
                    "organizationUsage",
                    "sevenDaySonnet",
                    "resetsAt",
                ],
                &[
                    "endpoints",
                    "organizationUsage",
                    "sevenDaySonnet",
                    "resetAt",
                ],
                &[
                    "endpoints",
                    "organizationUsage",
                    "seven_day_sonnet_reset_time",
                ],
                &["endpoints", "organizationUsage", "sevenDaySonnetResetTime"],
            ],
        ),
        extra_usage_percentage: extra_enabled.then(|| {
            extra_usage
                .map(normalize_desktop_usage_percentage)
                .unwrap_or(0)
        }),
        extra_usage_reset_time: first_reset_path_candidates(
            Some(profile),
            &[
                &["endpoints", "organizationUsage", "extra_usage", "resets_at"],
                &["endpoints", "organizationUsage", "extraUsage", "resetsAt"],
                &["endpoints", "overageSpendLimit", "resets_at"],
                &["endpoints", "overageSpendLimit", "resetsAt"],
            ],
        ),
        extra_usage_used_cents: first_i64_path_candidates(
            Some(profile),
            &[
                &[
                    "endpoints",
                    "organizationUsage",
                    "extra_usage",
                    "used_credits",
                ],
                &[
                    "endpoints",
                    "organizationUsage",
                    "extraUsage",
                    "usedCredits",
                ],
                &["endpoints", "overageSpendLimit", "used_credits"],
                &["endpoints", "overageSpendLimit", "usedCredits"],
                &["endpoints", "overageSpendLimit", "used_cents"],
                &["endpoints", "overageSpendLimit", "usedCents"],
            ],
        ),
        extra_usage_limit_cents: first_i64_path_candidates(
            Some(profile),
            &[
                &[
                    "endpoints",
                    "organizationUsage",
                    "extra_usage",
                    "monthly_limit",
                ],
                &[
                    "endpoints",
                    "organizationUsage",
                    "extraUsage",
                    "monthlyLimit",
                ],
                &["endpoints", "overageSpendLimit", "monthly_limit"],
                &["endpoints", "overageSpendLimit", "monthlyLimit"],
                &["endpoints", "overageSpendLimit", "limit_cents"],
                &["endpoints", "overageSpendLimit", "limitCents"],
            ],
        ),
        raw_data: Some(json!({
            "source": "claude_desktop_web",
            "organizationUsage": endpoints.and_then(|value| value.get("organizationUsage")).cloned(),
            "subscriptionDetails": endpoints.and_then(|value| value.get("subscriptionDetails")).cloned(),
            "overageSpendLimit": endpoints.and_then(|value| value.get("overageSpendLimit")).cloned(),
        })),
    })
}

fn desktop_web_profile_summary(profile: &Value) -> Value {
    let email = first_string_path_candidates(
        Some(profile),
        &[
            &["endpoints", "accountProfile", "account", "email"],
            &["endpoints", "accountProfile", "account", "email_address"],
            &["endpoints", "accountProfile", "email"],
            &["endpoints", "account", "account", "email"],
            &["endpoints", "account", "email"],
            &["endpoints", "bootstrapAppStart", "account", "email"],
            &["endpoints", "bootstrapAppStart", "user", "email"],
        ],
    )
    .or_else(|| find_string_by_key(profile, &["email", "email_address", "emailAddress"]));
    let avatar_url = first_string_path_candidates(
        Some(profile),
        &[
            &["endpoints", "accountProfile", "account", "avatar_url"],
            &["endpoints", "accountProfile", "account", "avatarUrl"],
            &["endpoints", "accountProfile", "account", "picture"],
            &["endpoints", "account", "avatar_url"],
            &["endpoints", "account", "picture"],
            &["endpoints", "bootstrapAppStart", "account", "avatar_url"],
        ],
    )
    .or_else(|| {
        find_string_by_key(
            profile,
            &[
                "avatar_url",
                "avatarUrl",
                "profile_image_url",
                "profileImageUrl",
                "picture",
                "picture_url",
                "image_url",
            ],
        )
    });
    let account_uuid = first_string_path_candidates(
        Some(profile),
        &[
            &["endpoints", "accountProfile", "account", "uuid"],
            &["endpoints", "account", "uuid"],
            &["endpoints", "account", "account", "uuid"],
            &["endpoints", "bootstrapAppStart", "account", "uuid"],
        ],
    )
    .or_else(|| find_string_by_key(profile, &["account_uuid", "accountUuid"]));
    let organization_uuid = first_string_path_candidates(
        Some(profile),
        &[
            &["endpoints", "accountProfile", "organization", "uuid"],
            &["endpoints", "account", "organization", "uuid"],
            &[
                "endpoints",
                "bootstrapAppStart",
                "activeOrganization",
                "uuid",
            ],
            &[
                "endpoints",
                "bootstrapAppStart",
                "active_organization",
                "uuid",
            ],
            &["endpoints", "bootstrapAppStart", "organization", "uuid"],
        ],
    )
    .or_else(|| find_string_by_key(profile, &["organization_uuid", "organizationUuid"]));
    let organization_name = first_string_path_candidates(
        Some(profile),
        &[
            &["endpoints", "accountProfile", "organization", "name"],
            &[
                "endpoints",
                "accountProfile",
                "organization",
                "display_name",
            ],
            &["endpoints", "account", "organization", "name"],
            &[
                "endpoints",
                "bootstrapAppStart",
                "activeOrganization",
                "name",
            ],
            &[
                "endpoints",
                "bootstrapAppStart",
                "active_organization",
                "name",
            ],
            &["endpoints", "bootstrapAppStart", "organization", "name"],
        ],
    )
    .or_else(|| find_string_by_key(profile, &["organization_name", "organizationName"]));
    let raw_plan = first_string_path_candidates(
        Some(profile),
        &[
            &["endpoints", "subscriptionDetails", "plan_type"],
            &["endpoints", "subscriptionDetails", "planType"],
            &["endpoints", "subscriptionDetails", "plan"],
            &["endpoints", "subscriptionDetails", "tier"],
            &["endpoints", "subscriptionDetails", "subscription_type"],
            &["endpoints", "subscriptionDetails", "subscriptionType"],
            &[
                "endpoints",
                "subscriptionDetails",
                "subscription",
                "plan_type",
            ],
            &[
                "endpoints",
                "subscriptionDetails",
                "subscription",
                "planType",
            ],
            &["endpoints", "subscriptionDetails", "subscription", "plan"],
            &["endpoints", "subscriptionDetails", "subscription", "tier"],
            &["endpoints", "organizationUsage", "plan_type"],
            &["endpoints", "organizationUsage", "planType"],
            &["endpoints", "organizationUsage", "subscription_type"],
            &["endpoints", "organizationUsage", "subscriptionType"],
            &[
                "endpoints",
                "accountProfile",
                "organization",
                "rate_limit_tier",
            ],
            &[
                "endpoints",
                "accountProfile",
                "organization",
                "organization_type",
            ],
            &[
                "endpoints",
                "accountProfile",
                "organization",
                "billing_type",
            ],
            &["endpoints", "account", "organization", "rate_limit_tier"],
            &["endpoints", "account", "organization", "organization_type"],
            &[
                "endpoints",
                "bootstrapAppStart",
                "activeOrganization",
                "rate_limit_tier",
            ],
            &[
                "endpoints",
                "bootstrapAppStart",
                "activeOrganization",
                "organization_type",
            ],
            &[
                "endpoints",
                "bootstrapAppStart",
                "active_organization",
                "rate_limit_tier",
            ],
            &[
                "endpoints",
                "bootstrapAppStart",
                "active_organization",
                "organization_type",
            ],
        ],
    )
    .or_else(|| {
        find_string_by_key(
            profile,
            &[
                "rate_limit_tier",
                "rateLimitTier",
                "subscription_type",
                "subscriptionType",
                "billing_type",
                "billingType",
                "organization_type",
                "organizationType",
                "plan_type",
                "planType",
                "plan_name",
                "planName",
                "subscription_tier",
                "subscriptionTier",
                "plan",
                "tier",
            ],
        )
    });
    // 严格对齐官方：仅 capabilities 识别、OAuth profile organization_type 识别。
    // 拿不到时返回 None，与官方 "没值则不显示" 一致。
    let plan_type = infer_desktop_plan_from_capabilities(profile)
        .or_else(|| normalize_desktop_plan_value(raw_plan.clone()));
    json!({
        "fetchedAt": read_string_path(profile, &["fetchedAt"]),
        "email": email,
        "avatarUrl": avatar_url,
        "accountUuid": account_uuid,
        "organizationUuid": organization_uuid,
        "organizationName": organization_name,
        "planType": plan_type,
        "rawPlan": raw_plan,
        "errors": profile.get("errors").cloned(),
    })
}

fn shorten_profile_error(raw: &str) -> String {
    let trimmed = raw.trim();
    let mut value = String::new();
    for ch in trimmed.chars().take(180) {
        value.push(ch);
    }
    if trimmed.chars().count() > 180 {
        value.push_str("...");
    }
    value
}

fn desktop_web_profile_error_message(profile: &Value) -> Option<String> {
    let errors = profile.get("errors")?.as_object()?;
    let first_error = errors
        .values()
        .filter_map(|value| normalize_non_empty(value.as_str()))
        .next()?;
    if first_error.contains("HTTP 403")
        || first_error.contains("Just a moment")
        || first_error.to_ascii_lowercase().contains("cloudflare")
    {
        return Some(
            "Claude Web 接口被 Cloudflare 校验拦截，暂时无法读取账号资料、订阅或额度；切号不受影响。"
                .to_string(),
        );
    }
    Some(format!(
        "Claude Web 资料接口失败: {}",
        shorten_profile_error(&first_error)
    ))
}

fn desktop_web_usage_error_message(profile: &Value) -> Option<String> {
    let error = profile
        .get("errors")
        .and_then(|value| value.as_object())
        .and_then(|errors| errors.get("organizationUsage"))
        .and_then(|value| normalize_non_empty(value.as_str()))?;
    if error.contains("missing lastActiveOrg") {
        return Some("Claude Desktop 账号缺少组织信息，暂时无法刷新额度。".to_string());
    }
    if error.contains("HTTP 403")
        || error.contains("Just a moment")
        || error.to_ascii_lowercase().contains("cloudflare")
    {
        return Some(
            "Claude Web usage 接口被 Cloudflare 校验拦截，暂时无法刷新额度；已保留旧缓存。"
                .to_string(),
        );
    }
    Some(format!(
        "Claude Desktop 额度刷新失败: {}",
        shorten_profile_error(&error)
    ))
}

fn desktop_account_has_real_profile_data(account: &ClaudeAccount) -> bool {
    account
        .email
        .split_once('@')
        .map(|(_, domain)| domain.contains('.'))
        .unwrap_or(false)
        || account.account_uuid.is_some()
        || account.avatar_url.is_some()
        || account
            .plan_type
            .as_deref()
            .and_then(|value| normalize_non_empty(Some(value)))
            .map(|value| !value.eq_ignore_ascii_case("Claude Desktop"))
            .unwrap_or(false)
        || account
            .organization_name
            .as_deref()
            .and_then(|value| normalize_non_empty(Some(value)))
            .map(|value| !value.eq_ignore_ascii_case("Claude Desktop"))
            .unwrap_or(false)
}

fn apply_desktop_web_profile(account: &mut ClaudeAccount, profile: &Value) -> bool {
    let summary = desktop_web_profile_summary(profile);
    let mut applied = false;
    let quota = desktop_web_usage_to_quota(profile);
    if let Some(quota) = quota {
        account.quota = Some(quota);
        // 仅存瘦身后的 webProfile，避免 bootstrapAppStart 中的 statsig / feature flags
        // / system_prompts 等数 MB 包体颍到账号文件与导出 JSON。
        account.claude_usage_raw = Some(slim_web_profile_for_storage(profile));
        account.usage_updated_at = Some(now_ts_ms());
        applied = true;
    } else {
        // 额度未识别时输出诊断信息，便于定位是接口失败还是字段结构不识别。
        let usage_node = profile
            .get("endpoints")
            .and_then(|v| v.get("organizationUsage"));
        let usage_keys: Vec<String> = usage_node
            .and_then(|v| v.as_object())
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        let usage_err = profile
            .get("errors")
            .and_then(|v| v.get("organizationUsage"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        logger::log_warn(&format!(
            "[Claude Desktop] organizationUsage 未识别: account_id={}, usage_present={}, usage_keys={:?}, usage_error={:?}",
            account.id,
            usage_node.is_some(),
            usage_keys,
            usage_err
        ));
    }
    if let Some(email) = read_string_path(&summary, &["email"]) {
        account.email = email;
        applied = true;
    }
    if let Some(account_uuid) = read_string_path(&summary, &["accountUuid"]) {
        account.account_uuid = Some(account_uuid);
        applied = true;
    }
    if let Some(organization_uuid) = read_string_path(&summary, &["organizationUuid"]) {
        account.organization_uuid = Some(organization_uuid);
        applied = true;
    }
    if let Some(organization_name) = read_string_path(&summary, &["organizationName"]) {
        account.organization_name = Some(organization_name);
        applied = true;
    }
    if let Some(plan_type) = read_string_path(&summary, &["planType"]) {
        account.plan_type = Some(plan_type);
        applied = true;
    } else if account
        .plan_type
        .as_deref()
        .map(is_desktop_plan_placeholder)
        .unwrap_or(false)
    {
        account.plan_type = None;
        applied = true;
    }
    if let Some(avatar_url) = read_string_path(&summary, &["avatarUrl"]) {
        account.avatar_url = Some(avatar_url);
        applied = true;
    }
    if applied {
        account.profile_updated_at = Some(now_ts_ms());
    } else if !desktop_account_has_real_profile_data(account) {
        account.profile_updated_at = None;
    }
    if let Some(config) = account.claude_config_raw.as_mut() {
        if !config.is_object() {
            *config = json!({});
        }
        if let Some(object) = config.as_object_mut() {
            let desktop_profile = object
                .entry("desktopProfile".to_string())
                .or_insert_with(|| json!({}));
            if !desktop_profile.is_object() {
                *desktop_profile = json!({});
            }
            if let Some(desktop_object) = desktop_profile.as_object_mut() {
                desktop_object.insert("webProfileSummary".to_string(), summary);
            }
        }
    }
    applied
}

pub fn export_accounts(account_ids: &[String]) -> Result<String, String> {
    let accounts: Vec<ClaudeAccount> = account_ids
        .iter()
        .filter_map(|id| load_account_file(id))
        .collect();
    serde_json::to_string_pretty(&accounts).map_err(|e| format!("序列化导出 JSON 失败: {}", e))
}

pub fn read_config_file(path: &Path) -> Result<Option<Value>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path).map_err(|e| {
        format!(
            "读取 Claude config 失败: path={}, error={}",
            path.display(),
            e
        )
    })?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str::<Value>(&content)
        .map(Some)
        .map_err(|e| format!("解析 Claude config 失败: {}", e))
}

fn write_config_file(path: &Path, config: &Value) -> Result<(), String> {
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("序列化 Claude config 失败: {}", e))?;
    atomic_write::write_string_atomic(path, &content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn normalize_env_key(value: &str) -> Option<String> {
    let key = value.trim().to_ascii_uppercase();
    if key.is_empty() {
        return None;
    }
    let mut chars = key.chars();
    match chars.next() {
        Some(ch) if ch == '_' || ch.is_ascii_uppercase() => {}
        _ => return None,
    }
    if chars.all(|ch| ch == '_' || ch.is_ascii_uppercase() || ch.is_ascii_digit()) {
        Some(key)
    } else {
        None
    }
}

fn managed_env_store_key(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn read_settings_managed_env_keys() -> BTreeMap<String, Vec<String>> {
    let Ok(path) = get_claude_code_settings_managed_env_keys_path() else {
        return BTreeMap::new();
    };
    let Ok(Some(value)) = read_config_file(&path) else {
        return BTreeMap::new();
    };
    let mut result = BTreeMap::new();
    let Some(object) = value.as_object() else {
        return result;
    };
    for (path, keys) in object {
        let key_list = keys
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .filter_map(normalize_env_key)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !key_list.is_empty() {
            result.insert(path.clone(), key_list);
        }
    }
    result
}

fn write_settings_managed_env_keys(map: &BTreeMap<String, Vec<String>>) -> Result<(), String> {
    let path = get_claude_code_settings_managed_env_keys_path()?;
    let value = serde_json::to_value(map)
        .map_err(|e| format!("序列化 Claude CLI settings 管理键失败: {}", e))?;
    write_config_file(&path, &value)
}

fn record_settings_managed_env_keys(
    settings_path: &Path,
    keys: BTreeSet<String>,
) -> Result<(), String> {
    let mut map = read_settings_managed_env_keys();
    let map_key = managed_env_store_key(settings_path);
    if keys.is_empty() {
        map.remove(&map_key);
    } else {
        map.insert(map_key, keys.into_iter().collect());
    }
    write_settings_managed_env_keys(&map)
}

fn managed_env_keys_for_settings(settings_path: &Path) -> BTreeSet<String> {
    let mut keys = CLAUDE_CODE_API_ENV_KEYS
        .iter()
        .filter_map(|key| normalize_env_key(key))
        .collect::<BTreeSet<_>>();
    let map = read_settings_managed_env_keys();
    if let Some(recorded) = map.get(&managed_env_store_key(settings_path)) {
        keys.extend(recorded.iter().filter_map(|key| normalize_env_key(key)));
    }
    keys
}

fn clear_api_key_env_from_claude_code_settings(config_dir: &Path) -> Result<(), String> {
    let settings_path = get_claude_code_settings_path(config_dir);
    let recorded_keys = read_settings_managed_env_keys();
    let has_recorded_keys = recorded_keys.contains_key(&managed_env_store_key(&settings_path));
    if !settings_path.exists() {
        if has_recorded_keys {
            record_settings_managed_env_keys(&settings_path, BTreeSet::new())?;
        }
        return Ok(());
    }

    let keys = managed_env_keys_for_settings(&settings_path);
    if keys.is_empty() {
        return Ok(());
    }

    let mut settings = read_config_file(&settings_path)?.unwrap_or_else(|| json!({}));
    if !settings.is_object() {
        settings = json!({});
    }

    if let Some(env_object) = settings
        .get_mut("env")
        .and_then(|value| value.as_object_mut())
    {
        for key in &keys {
            env_object.remove(key);
        }
    }

    write_config_file(&settings_path, &settings)?;
    record_settings_managed_env_keys(&settings_path, BTreeSet::new())
}

pub(crate) fn build_api_key_cli_env_map(
    account: &ClaudeAccount,
) -> Result<BTreeMap<String, String>, String> {
    let api_key = account
        .api_key
        .as_deref()
        .and_then(|value| normalize_non_empty(Some(value)))
        .ok_or_else(|| "Claude API Key 账号缺少 API Key".to_string())?;
    let api_base_url = account
        .api_base_url
        .as_deref()
        .and_then(|value| normalize_non_empty(Some(value)));
    let key_field =
        normalize_api_key_field(account.api_key_field.as_deref(), api_base_url.as_deref());
    let mut env = BTreeMap::new();
    if let Some(extra_env) = account.api_extra_env.as_ref() {
        for (key, value) in extra_env {
            let Some(key) = normalize_env_key(key) else {
                continue;
            };
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            if matches!(
                key.as_str(),
                "ANTHROPIC_API_KEY" | "ANTHROPIC_AUTH_TOKEN" | "ANTHROPIC_BASE_URL"
            ) {
                continue;
            }
            env.insert(key, value.to_string());
        }
    }
    if let Some(api_base_url) = api_base_url {
        env.insert("ANTHROPIC_BASE_URL".to_string(), api_base_url);
    }
    env.insert(key_field, api_key);
    Ok(env)
}

fn inject_api_key_to_claude_code_settings(
    account: &ClaudeAccount,
    config_dir: Option<&Path>,
) -> Result<(), String> {
    let config_dir = get_effective_claude_code_config_dir(config_dir)?;
    let settings_path = get_claude_code_settings_path(&config_dir);
    let env = build_api_key_cli_env_map(account)?;
    let managed_keys = env.keys().cloned().collect::<BTreeSet<_>>();

    fs::create_dir_all(&config_dir).map_err(|e| format!("创建 Claude Code 配置目录失败: {}", e))?;
    let mut settings = read_config_file(&settings_path)?.unwrap_or_else(|| json!({}));
    if !settings.is_object() {
        settings = json!({});
    }

    let keys_to_clear = managed_env_keys_for_settings(&settings_path);
    let object = settings
        .as_object_mut()
        .ok_or_else(|| "Claude settings.json 结构非法".to_string())?;
    let env_value = object.entry("env".to_string()).or_insert_with(|| json!({}));
    if !env_value.is_object() {
        *env_value = json!({});
    }
    let env_object = env_value
        .as_object_mut()
        .ok_or_else(|| "Claude settings.json env 结构非法".to_string())?;
    for key in keys_to_clear {
        env_object.remove(&key);
    }
    for (key, value) in env {
        env_object.insert(key, Value::String(value));
    }

    write_config_file(&settings_path, &settings)?;
    record_settings_managed_env_keys(&settings_path, managed_keys)
}

#[cfg(target_os = "macos")]
fn claude_code_keychain_service_name() -> String {
    let hash_suffix = std::env::var("CLAUDE_CONFIG_DIR")
        .ok()
        .and_then(|value| normalize_non_empty(Some(&value)))
        .map(|value| {
            let digest = Sha256::digest(value.as_bytes());
            let hex = hex_encode(&digest);
            format!("-{}", &hex[..8])
        })
        .unwrap_or_default();
    format!(
        "{}{}{}",
        CLAUDE_CODE_KEYCHAIN_SERVICE_PREFIX, CLAUDE_CODE_KEYCHAIN_CREDENTIALS_SUFFIX, hash_suffix
    )
}

#[cfg(target_os = "macos")]
fn claude_code_keychain_account_name() -> String {
    std::env::var("USER")
        .ok()
        .and_then(|value| normalize_non_empty(Some(&value)))
        .or_else(|| {
            std::env::var("LOGNAME")
                .ok()
                .and_then(|value| normalize_non_empty(Some(&value)))
        })
        .unwrap_or_else(|| "claude-code-user".to_string())
}

#[cfg(target_os = "macos")]
fn read_claude_code_keychain_credentials() -> Option<Value> {
    let service = claude_code_keychain_service_name();
    let account = claude_code_keychain_account_name();
    let output = std::process::Command::new("security")
        .args([
            "find-generic-password",
            "-a",
            account.as_str(),
            "-w",
            "-s",
            service.as_str(),
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(text.trim()).ok()
}

#[cfg(target_os = "macos")]
fn write_claude_code_keychain_credentials(credentials: &Value) -> Result<(), String> {
    let service = claude_code_keychain_service_name();
    let account = claude_code_keychain_account_name();
    let content = serde_json::to_string(credentials)
        .map_err(|e| format!("序列化 Claude Code Keychain credentials 失败: {}", e))?;
    let hex_content = hex_encode(content.as_bytes());
    let output = std::process::Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-a",
            account.as_str(),
            "-s",
            service.as_str(),
            "-X",
            hex_content.as_str(),
        ])
        .output()
        .map_err(|e| format!("调用 macOS Keychain 失败: {}", e))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = stderr.trim();
    Err(format!(
        "写入 macOS Keychain 失败: {}",
        if message.is_empty() {
            "unknown error"
        } else {
            message
        }
    ))
}

#[cfg(target_os = "macos")]
fn delete_claude_code_keychain_credentials() {
    let service = claude_code_keychain_service_name();
    let account = claude_code_keychain_account_name();
    let _ = std::process::Command::new("security")
        .args([
            "delete-generic-password",
            "-a",
            account.as_str(),
            "-s",
            service.as_str(),
        ])
        .output();
}

fn read_plaintext_claude_code_credentials(config_dir: &Path) -> Option<Value> {
    read_config_file(&get_claude_code_credentials_path(config_dir))
        .ok()
        .flatten()
}

fn read_claude_code_credentials(config_dir: &Path) -> Value {
    #[cfg(target_os = "macos")]
    if let Some(value) = read_claude_code_keychain_credentials() {
        return value;
    }
    read_plaintext_claude_code_credentials(config_dir).unwrap_or_else(|| json!({}))
}

fn write_plaintext_claude_code_credentials(
    config_dir: &Path,
    credentials: &Value,
) -> Result<(), String> {
    write_config_file(&get_claude_code_credentials_path(config_dir), credentials)
}

fn write_claude_code_credentials(config_dir: &Path, credentials: &Value) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        match write_claude_code_keychain_credentials(credentials) {
            Ok(()) => {
                let _ = remove_path_if_exists(&get_claude_code_credentials_path(config_dir));
                return Ok(());
            }
            Err(error) => {
                logger::log_warn(&format!(
                    "[Claude Code] Keychain 写入失败，回退到 .credentials.json: {}",
                    error
                ));
                write_plaintext_claude_code_credentials(config_dir, credentials)?;
                delete_claude_code_keychain_credentials();
                return Ok(());
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        write_plaintext_claude_code_credentials(config_dir, credentials)
    }
}

fn merge_claude_code_oauth_config(mut target: Value, snapshot: &Value) -> Value {
    if !target.is_object() {
        target = json!({});
    }
    if let Some(target_object) = target.as_object_mut() {
        if let Some(oauth_account) = snapshot.get("oauthAccount").cloned() {
            target_object.insert("oauthAccount".to_string(), oauth_account);
        }
        target_object.insert("hasCompletedOnboarding".to_string(), Value::Bool(true));
    }
    target
}

fn inject_oauth_account_to_claude_code(
    account: &ClaudeAccount,
    config_dir: Option<&Path>,
) -> Result<(), String> {
    let config_dir = get_effective_claude_code_config_dir(config_dir)?;
    let credentials_snapshot = account
        .claude_credentials_raw
        .as_ref()
        .ok_or_else(|| "Claude OAuth 账号缺少 credentials 快照".to_string())?;
    let oauth_credentials = credentials_snapshot
        .get("claudeAiOauth")
        .cloned()
        .ok_or_else(|| "Claude OAuth 账号 credentials 缺少 claudeAiOauth".to_string())?;
    let config_snapshot = account
        .claude_config_raw
        .as_ref()
        .ok_or_else(|| "Claude OAuth 账号缺少 config 快照".to_string())?;
    if config_snapshot.get("oauthAccount").is_none() {
        return Err("Claude OAuth 账号 config 缺少 oauthAccount".to_string());
    }

    let mut credentials = read_claude_code_credentials(&config_dir);
    if !credentials.is_object() {
        credentials = json!({});
    }
    if let Some(object) = credentials.as_object_mut() {
        object.insert("claudeAiOauth".to_string(), oauth_credentials);
    }
    write_claude_code_credentials(&config_dir, &credentials)?;

    let global_config_path = get_claude_code_global_config_path(&config_dir)?;
    let target_config = read_config_file(&global_config_path)?.unwrap_or_else(|| json!({}));
    let merged_config = merge_claude_code_oauth_config(target_config, config_snapshot);
    write_config_file(&global_config_path, &merged_config)?;
    Ok(())
}

pub fn inject_to_claude_config(account_id: &str, config_dir: Option<&Path>) -> Result<(), String> {
    let account = load_account(account_id).ok_or_else(|| "Claude 账号不存在".to_string())?;
    if account.auth_mode == ClaudeAuthMode::DesktopOAuth {
        if config_dir.is_some() {
            return Err(
                "Claude Desktop 登录态不能写入旧配置目录，请使用 Claude Desktop 实例。".to_string(),
            );
        }
        let snapshot_dir = account
            .desktop_profile_dir
            .as_deref()
            .and_then(|value| normalize_non_empty(Some(value)))
            .map(PathBuf::from)
            .ok_or_else(|| "Claude Desktop 账号缺少 profile 快照".to_string())?;
        let target_dir = get_default_claude_desktop_user_data_dir()?;
        quit_claude_desktop_for_profile_write()?;
        let _backup_dir = backup_current_desktop_profile(&target_dir)?;
        restore_desktop_profile_snapshot(&snapshot_dir, &target_dir)?;

        let mut updated = account.clone();
        updated.last_used = now_ts_ms();
        save_account_and_index(updated)?;
        launch_default_claude_desktop();
        return Ok(());
    }
    if account.auth_mode == ClaudeAuthMode::ApiKey {
        inject_api_key_to_claude_code_settings(&account, config_dir)?;
        let mut updated = account.clone();
        updated.last_used = now_ts_ms();
        save_account_and_index(updated)?;
        return Ok(());
    }
    let config_dir_path = get_effective_claude_code_config_dir(config_dir)?;
    clear_api_key_env_from_claude_code_settings(&config_dir_path)?;
    inject_oauth_account_to_claude_code(&account, config_dir)?;

    let mut updated = account.clone();
    updated.last_used = now_ts_ms();
    save_account_and_index(updated)?;
    Ok(())
}

pub fn inject_to_claude(account_id: &str) -> Result<(), String> {
    inject_to_claude_config(account_id, None)
}

pub fn resolve_current_account_for_platform(
    platform: &str,
    accounts: &[ClaudeAccount],
) -> Option<ClaudeAccount> {
    let current_id = crate::modules::provider_current_state::resolve_existing_current_account_id(
        platform,
        accounts.iter().map(|item| item.id.as_str()),
    );
    if let Some(current_id) = current_id {
        if let Some(account) = accounts.iter().find(|item| item.id == current_id) {
            return Some(account.clone());
        }
    }
    None
}

pub fn remove_account(account_id: &str) -> Result<(), String> {
    remove_accounts(&[account_id.to_string()])
}

pub fn remove_accounts(account_ids: &[String]) -> Result<(), String> {
    let _lock = CLAUDE_ACCOUNT_INDEX_LOCK
        .lock()
        .map_err(|_| "无法获取 Claude 账号锁")?;
    let mut index = load_index()?;
    for account_id in account_ids {
        if let Some(account) = load_account_file(account_id) {
            if account.auth_mode == ClaudeAuthMode::DesktopOAuth {
                if let Some(snapshot_dir) = account
                    .desktop_profile_dir
                    .as_deref()
                    .and_then(|value| normalize_non_empty(Some(value)))
                {
                    let snapshot_path = PathBuf::from(snapshot_dir);
                    if snapshot_path.exists() {
                        fs::remove_dir_all(&snapshot_path).map_err(|e| {
                            format!(
                                "删除 Claude Desktop 快照失败: path={}, error={}",
                                snapshot_path.display(),
                                e
                            )
                        })?;
                    }
                }
            }
        }
        let path = account_file_path(account_id)?;
        if path.exists() {
            fs::remove_file(&path).map_err(|e| {
                format!("删除 Claude 账号失败: path={}, error={}", path.display(), e)
            })?;
        }
    }
    index
        .accounts
        .retain(|item| !account_ids.iter().any(|id| id == &item.id));
    save_index(&index)?;
    for platform in ["claude", "claude_cli"] {
        let _ = crate::modules::provider_current_state::resolve_existing_current_account_id(
            platform,
            index.accounts.iter().map(|item| item.id.as_str()),
        );
    }
    Ok(())
}

pub fn update_account_tags(account_id: &str, tags: Vec<String>) -> Result<ClaudeAccount, String> {
    let _lock = CLAUDE_ACCOUNT_INDEX_LOCK
        .lock()
        .map_err(|_| "无法获取 Claude 账号锁")?;
    let mut account = load_account(account_id).ok_or_else(|| "Claude 账号不存在".to_string())?;
    account.tags = Some(
        tags.into_iter()
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect(),
    );
    save_account_and_index(account)
}

pub fn update_account_plan(
    account_id: &str,
    plan_type: Option<&str>,
) -> Result<ClaudeAccount, String> {
    let _lock = CLAUDE_ACCOUNT_INDEX_LOCK
        .lock()
        .map_err(|_| "无法获取 Claude 账号锁")?;
    let mut account = load_account(account_id).ok_or_else(|| "Claude 账号不存在".to_string())?;
    account.plan_type = plan_type
        .and_then(|value| normalize_non_empty(Some(value)))
        .map(|value| value.to_string());
    save_account_and_index(account)
}

pub fn update_account_note(account_id: &str, note: Option<&str>) -> Result<ClaudeAccount, String> {
    let _lock = CLAUDE_ACCOUNT_INDEX_LOCK
        .lock()
        .map_err(|_| "无法获取 Claude 账号锁")?;
    let mut account = load_account(account_id).ok_or_else(|| "Claude 账号不存在".to_string())?;
    account.account_note = note
        .and_then(|value| normalize_non_empty(Some(value)))
        .map(|value| value.to_string());
    save_account_and_index(account)
}

fn usage_to_quota(raw: &Value) -> ClaudeQuota {
    let five_hour = raw.get("five_hour");
    let seven_day = raw.get("seven_day");
    let seven_day_sonnet = raw
        .get("seven_day_sonnet")
        .or_else(|| raw.get("seven_day_sonnet_4"))
        .or_else(|| raw.get("seven_day_model"));
    let extra_usage = raw.get("extra_usage");

    let extra_enabled = extra_usage
        .and_then(|item| item.get("is_enabled"))
        .and_then(|item| item.as_bool())
        .unwrap_or(false);
    let extra_usage_percentage = extra_enabled.then(|| {
        clamp_percentage(
            extra_usage
                .and_then(|item| item.get("utilization"))
                .and_then(|item| item.as_f64()),
        )
    });

    ClaudeQuota {
        five_hour_percentage: clamp_percentage(
            five_hour
                .and_then(|item| item.get("utilization"))
                .and_then(|item| item.as_f64()),
        ),
        five_hour_reset_time: parse_reset_seconds(five_hour.and_then(|item| item.get("resets_at"))),
        seven_day_percentage: clamp_percentage(
            seven_day
                .and_then(|item| item.get("utilization"))
                .and_then(|item| item.as_f64()),
        ),
        seven_day_reset_time: parse_reset_seconds(seven_day.and_then(|item| item.get("resets_at"))),
        seven_day_sonnet_percentage: seven_day_sonnet
            .map(|item| clamp_percentage(item.get("utilization").and_then(|value| value.as_f64()))),
        seven_day_sonnet_reset_time: parse_reset_seconds(
            seven_day_sonnet.and_then(|item| item.get("resets_at")),
        ),
        extra_usage_percentage,
        extra_usage_reset_time: parse_reset_seconds(
            extra_usage.and_then(|item| item.get("resets_at")),
        ),
        extra_usage_used_cents: read_i64_value(
            extra_usage.and_then(|item| item.get("used_credits")),
        ),
        extra_usage_limit_cents: read_i64_value(
            extra_usage.and_then(|item| item.get("monthly_limit")),
        ),
        raw_data: Some(raw.clone()),
    }
}

async fn refresh_oauth_credentials(credentials: &Value) -> Result<Option<Value>, String> {
    let Some(refresh_token) = credentials_refresh_token(credentials) else {
        return Ok(None);
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
    let resp = client
        .post(CLAUDE_OAUTH_TOKEN_URL)
        .header(CONTENT_TYPE, "application/json")
        .header(USER_AGENT, "antigravity-cockpit-tools")
        .json(&json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": CLAUDE_OAUTH_CLIENT_ID,
        }))
        .send()
        .await
        .map_err(|e| format!("刷新 Claude OAuth token 失败: {}", e))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("读取 Claude OAuth 响应失败: {}", e))?;
    if !status.is_success() {
        return Err(format!(
            "刷新 Claude OAuth token 失败: HTTP {} {}",
            status, body
        ));
    }
    let payload: Value =
        serde_json::from_str(&body).map_err(|e| format!("解析 Claude OAuth 响应失败: {}", e))?;
    let mut next = credentials.clone();
    let oauth = next
        .get_mut("claudeAiOauth")
        .and_then(|item| item.as_object_mut())
        .ok_or_else(|| "Claude credentials 缺少 claudeAiOauth 字段".to_string())?;
    if let Some(access_token) = read_string_path(&payload, &["access_token"]) {
        oauth.insert("accessToken".to_string(), Value::String(access_token));
    }
    if let Some(refresh_token) = read_string_path(&payload, &["refresh_token"]) {
        oauth.insert("refreshToken".to_string(), Value::String(refresh_token));
    }
    if let Some(expires_in) = read_i64_value(payload.get("expires_in")) {
        oauth.insert(
            "expiresAt".to_string(),
            Value::Number(serde_json::Number::from(now_ts_ms() + expires_in * 1000)),
        );
    }
    if let Some(scope) = read_string_path(&payload, &["scope"]) {
        oauth.insert(
            "scopes".to_string(),
            Value::Array(
                scope
                    .split_whitespace()
                    .map(|item| Value::String(item.to_string()))
                    .collect(),
            ),
        );
    }
    Ok(Some(next))
}

async fn request_usage(access_token: &str) -> Result<Value, String> {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", access_token))
            .map_err(|e| format!("构造 Claude usage Authorization 失败: {}", e))?,
    );
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static(CLAUDE_OAUTH_BETA_HEADER),
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("antigravity-cockpit-tools"),
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
    let resp = client
        .get(CLAUDE_OAUTH_USAGE_URL)
        .headers(headers)
        .send()
        .await
        .map_err(|e| format!("请求 Claude usage 失败: {}", e))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("读取 Claude usage 响应失败: {}", e))?;
    if !status.is_success() {
        return Err(format!("请求 Claude usage 失败: HTTP {} {}", status, body));
    }
    serde_json::from_str(&body).map_err(|e| format!("解析 Claude usage 响应失败: {}", e))
}

pub async fn refresh_account_quota(account_id: &str) -> Result<ClaudeAccount, String> {
    let mut account = load_account(account_id).ok_or_else(|| "Claude 账号不存在".to_string())?;
    if account.auth_mode == ClaudeAuthMode::ApiKey {
        account.quota = None;
        account.quota_error = Some(ClaudeQuotaErrorInfo {
            code: Some("unsupported_auth_mode".to_string()),
            message:
                "Claude API Key 账号不支持 Claude 订阅配额刷新，请在 Anthropic Console 查看用量。"
                    .to_string(),
            timestamp: now_ts(),
        });
        account.usage_updated_at = Some(now_ts_ms());
        return save_account_and_index(account);
    }
    if account.auth_mode == ClaudeAuthMode::DesktopOAuth {
        let snapshot_dir = account
            .desktop_profile_dir
            .as_deref()
            .and_then(|value| normalize_non_empty(Some(value)))
            .map(PathBuf::from)
            .ok_or_else(|| "Claude Desktop 账号缺少 profile 快照".to_string())?;
        let local_profile_applied = apply_desktop_local_profile(&mut account, &snapshot_dir);
        match probe_desktop_web_profile(&snapshot_dir) {
            Ok(web_profile) => {
                let web_quota_available = desktop_web_usage_to_quota(&web_profile).is_some();
                let usage_error = desktop_web_usage_error_message(&web_profile);
                let profile_applied = apply_desktop_web_profile(&mut account, &web_profile);
                if profile_applied
                    || local_profile_applied
                    || desktop_account_has_real_profile_data(&account)
                {
                    account.status = None;
                    account.status_reason = None;
                    if web_quota_available {
                        account.quota_error = None;
                    } else if let Some(message) = usage_error {
                        account.quota_error = Some(ClaudeQuotaErrorInfo {
                            code: Some("desktop_usage_refresh_failed".to_string()),
                            message,
                            timestamp: now_ts(),
                        });
                    } else {
                        account.quota_error = None;
                    }
                } else {
                    let message =
                        desktop_web_profile_error_message(&web_profile).unwrap_or_else(|| {
                            "Claude Desktop 资料接口未返回邮箱、头像或套餐字段。".to_string()
                        });
                    account.quota_error = Some(ClaudeQuotaErrorInfo {
                        code: Some("desktop_profile_failed".to_string()),
                        message: message.clone(),
                        timestamp: now_ts(),
                    });
                    account.status_reason = Some(message);
                }
            }
            Err(error) => {
                logger::log_warn(&format!(
                    "[Claude Desktop] 刷新账号资料失败: account_id={}, error={}",
                    account_id, error
                ));
                let message = format!("Claude Desktop 资料刷新失败: {}", error);
                if local_profile_applied || desktop_account_has_real_profile_data(&account) {
                    account.quota_error = Some(ClaudeQuotaErrorInfo {
                        code: Some("desktop_usage_refresh_failed".to_string()),
                        message,
                        timestamp: now_ts(),
                    });
                    account.status = None;
                    account.status_reason = None;
                } else {
                    account.quota_error = Some(ClaudeQuotaErrorInfo {
                        code: Some("desktop_profile_failed".to_string()),
                        message: message.clone(),
                        timestamp: now_ts(),
                    });
                    account.status_reason = Some(message);
                }
            }
        }
        return save_account_and_index(account);
    }

    let mut credentials = account
        .claude_credentials_raw
        .clone()
        .ok_or_else(|| "Claude 账号缺少 credentials 快照".to_string())?;

    if token_is_expired(&credentials) {
        match refresh_oauth_credentials(&credentials).await {
            Ok(Some(refreshed)) => {
                credentials = refreshed;
                account.claude_credentials_raw = Some(credentials.clone());
            }
            Ok(None) => {}
            Err(error) => {
                account.quota_error = Some(ClaudeQuotaErrorInfo {
                    code: Some("refresh_failed".to_string()),
                    message: error,
                    timestamp: now_ts(),
                });
                account.usage_updated_at = Some(now_ts_ms());
                return save_account_and_index(account);
            }
        }
    }

    let Some(access_token) = credentials_access_token(&credentials) else {
        account.quota_error = Some(ClaudeQuotaErrorInfo {
            code: Some("missing_access_token".to_string()),
            message: "Claude 账号缺少 accessToken".to_string(),
            timestamp: now_ts(),
        });
        account.usage_updated_at = Some(now_ts_ms());
        return save_account_and_index(account);
    };

    match request_usage(&access_token).await {
        Ok(usage) => {
            account.quota = Some(usage_to_quota(&usage));
            account.claude_usage_raw = Some(usage);
            account.usage_updated_at = Some(now_ts_ms());
            account.quota_error = None;
            account.status = None;
            account.status_reason = None;
        }
        Err(error) => {
            logger::log_warn(&format!(
                "[Claude Quota] 刷新失败: account_id={}, error={}",
                account_id, error
            ));
            account.quota_error = Some(ClaudeQuotaErrorInfo {
                code: Some("usage_failed".to_string()),
                message: error,
                timestamp: now_ts(),
            });
            account.usage_updated_at = Some(now_ts_ms());
        }
    }
    save_account_and_index(account)
}

pub async fn refresh_all_quotas() -> Result<Vec<(String, Result<ClaudeAccount, String>)>, String> {
    let accounts = list_accounts_checked()?;
    let mut results = Vec::with_capacity(accounts.len());
    for account in accounts {
        let id = account.id.clone();
        results.push((id.clone(), refresh_account_quota(&id).await));
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_oauth_authorize_url_as_callback_input() {
        let error = parse_oauth_callback_input(
            "https://claude.com/cai/oauth/authorize?code=true&client_id=test-client",
        )
        .expect_err("authorize entry URL should not be accepted as callback code");

        assert!(error.contains("授权入口链接"));
    }

    #[test]
    fn parses_oauth_callback_url_with_state() {
        let (code, state) = parse_oauth_callback_input(
            "https://platform.claude.com/oauth/code/callback?code=actual-code&state=state-1",
        )
        .expect("callback URL should parse");

        assert_eq!(code, "actual-code");
        assert_eq!(state.as_deref(), Some("state-1"));
    }

    #[test]
    fn slims_claude_code_config_snapshot_to_switch_required_fields() {
        let full_config = serde_json::json!({
            "oauthAccount": {
                "emailAddress": "alice@testmail.dev",
                "accountUuid": "b55de31d-da47-4433-9a73-bbba05affeeb"
            },
            "email": "alice@testmail.dev",
            "hasCompletedOnboarding": true,
            "cachedGrowthBookFeatures": {
                "tengu_amber_lattice": {
                    "plugins": ["security-guidance", "code-review"]
                }
            },
            "cachedDynamicConfigs": {
                "tengu-top-of-feed-tip": {
                    "color": "warning",
                    "tip": "large cached payload"
                }
            }
        });

        let slimmed = slim_claude_code_config_snapshot(&full_config);

        assert!(slimmed.get("oauthAccount").is_some());
        assert_eq!(
            read_string_path(&slimmed, &["oauthAccount", "emailAddress"]).as_deref(),
            Some("alice@testmail.dev")
        );
        assert_eq!(
            read_string_path(&slimmed, &["email"]).as_deref(),
            Some("alice@testmail.dev")
        );
        assert_eq!(
            read_bool_path(&slimmed, &["hasCompletedOnboarding"]),
            Some(true)
        );
        assert!(slimmed.get("cachedGrowthBookFeatures").is_none());
        assert!(slimmed.get("cachedDynamicConfigs").is_none());
    }

    #[test]
    fn slims_only_claude_cli_oauth_account_snapshots() {
        let config = serde_json::json!({
            "oauthAccount": {
                "emailAddress": "alice@testmail.dev"
            },
            "cachedGrowthBookFeatures": {
                "large": true
            }
        });
        let mut account = test_desktop_account(
            "claude_desktop",
            "alice@testmail.dev",
            None,
            Some("/tmp/snapshot"),
            10,
            20,
        );
        account.claude_config_raw = Some(config.clone());
        assert!(!slim_claude_account_snapshots(&mut account));
        assert_eq!(account.claude_config_raw.as_ref(), Some(&config));

        account.auth_mode = ClaudeAuthMode::OAuth;
        assert!(slim_claude_account_snapshots(&mut account));
        let slimmed = account.claude_config_raw.as_ref().expect("slimmed config");
        assert!(slimmed.get("oauthAccount").is_some());
        assert!(slimmed.get("cachedGrowthBookFeatures").is_none());
    }

    #[test]
    fn derives_oauth_plan_from_subscription_type_before_billing_source() {
        let credentials = serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "sk-ant-oat01-test",
                "refreshToken": "sk-ant-ort01-test",
                "subscriptionType": "Pro",
                "profile": {
                    "account": {
                        "has_claude_pro": true,
                        "has_claude_max": false
                    },
                    "organization": {
                        "organization_type": "claude_pro",
                        "billing_type": "apple_subscription"
                    }
                }
            }
        });
        let config = serde_json::json!({
            "oauthAccount": {
                "emailAddress": "alice@testmail.dev",
                "accountUuid": "b55de31d-da47-4433-9a73-bbba05affeeb",
                "organizationUuid": "d6faab9e-25dc-4d42-bce1-08f2dfe21bf6",
                "billingType": "apple_subscription",
                "organizationType": "claude_pro",
                "subscriptionType": "Pro"
            }
        });

        let account = derive_account_from_snapshots(credentials, config, None)
            .expect("account should be derived");

        assert_eq!(account.plan_type.as_deref(), Some("Pro"));
    }

    #[test]
    fn normalizes_existing_oauth_plan_from_billing_source_to_subscription() {
        let credentials = serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "sk-ant-oat01-test",
                "subscriptionType": "Pro"
            }
        });
        let config = serde_json::json!({
            "oauthAccount": {
                "emailAddress": "alice@testmail.dev",
                "billingType": "apple_subscription",
                "subscriptionType": "Pro"
            }
        });
        let mut account = derive_account_from_snapshots(credentials, config, None)
            .expect("account should be derived");
        account.plan_type = Some("apple_subscription".to_string());

        assert!(normalize_account_plan_from_snapshots(&mut account));
        assert_eq!(account.plan_type.as_deref(), Some("Pro"));
    }

    #[test]
    fn extracts_desktop_local_profile_from_indexeddb_blob_text() {
        let blob = br#"
            datao" accounto" tagged_id" user_abc"
            uuid"$b55de31d-da47-4433-9a73-bbba05affeeb"
            email_address" alice@testmail.dev"
            full_name" Alice Chen"
            display_name" Alice"
            membershipsA o" organizationo" idI"
            uuid"$d6faab9e-25dc-4d42-bce1-08f2dfe21bf6"
            name" Alice Workspace"
            settings
        "#;

        let profile = extract_desktop_local_profile_from_bytes(Path::new("IndexedDB/blob/1"), blob)
            .expect("profile should be extracted");

        assert_eq!(profile.email.as_deref(), Some("alice@testmail.dev"));
        assert_eq!(
            profile.account_uuid.as_deref(),
            Some("b55de31d-da47-4433-9a73-bbba05affeeb")
        );
        assert_eq!(profile.display_name.as_deref(), Some("Alice"));
        assert_eq!(profile.full_name.as_deref(), Some("Alice Chen"));
        assert_eq!(
            profile.organization_uuid.as_deref(),
            Some("d6faab9e-25dc-4d42-bce1-08f2dfe21bf6")
        );
        assert_eq!(
            profile.organization_name.as_deref(),
            Some("Alice Workspace")
        );
    }

    #[test]
    fn extracts_desktop_subscription_and_usage_from_web_profile() {
        let profile = serde_json::json!({
            "fetchedAt": "2026-06-13T12:00:00Z",
            "endpoints": {
                "accountProfile": {
                    "account": {
                        "email_address": "alice@testmail.dev",
                        "uuid": "b55de31d-da47-4433-9a73-bbba05affeeb"
                    }
                },
                "subscriptionDetails": {
                    "plan_type": "claude_max_20x"
                },
                "organizationUsage": {
                    "five_hour": {
                        "utilization": 42,
                        "resets_at": "2026-06-13T17:00:00Z"
                    },
                    "sevenDay": {
                        "utilization": 0.88,
                        "resetsAt": 1781366400
                    },
                    "seven_day_sonnet": {
                        "utilization": 12,
                        "resets_at": "2026-06-14T09:00:00Z"
                    }
                }
            }
        });

        let summary = desktop_web_profile_summary(&profile);
        assert_eq!(
            read_string_path(&summary, &["email"]).as_deref(),
            Some("alice@testmail.dev")
        );
        assert_eq!(
            read_string_path(&summary, &["planType"]).as_deref(),
            Some("Max 20x")
        );

        let quota = desktop_web_usage_to_quota(&profile).expect("usage should produce quota");
        assert_eq!(quota.five_hour_percentage, 42);
        assert_eq!(quota.seven_day_percentage, 88);
        assert_eq!(quota.seven_day_sonnet_percentage, Some(12));
        assert!(quota.five_hour_reset_time.is_some());
        assert!(quota.seven_day_sonnet_reset_time.is_some());
    }

    #[test]
    fn maps_default_claude_rate_limit_tier_to_free_plan() {
        let profile = serde_json::json!({
            "endpoints": {
                "account": {
                    "email_address": "alice@testmail.dev",
                    "memberships": [
                        {
                            "organization": {
                                "rate_limit_tier": "default_claude_ai",
                                "rate_limit_upsell": "upgrade_to_pro"
                            }
                        }
                    ]
                }
            }
        });

        let summary = desktop_web_profile_summary(&profile);
        assert_eq!(
            read_string_path(&summary, &["planType"]).as_deref(),
            Some("Free")
        );
        assert_eq!(
            read_string_path(&summary, &["rawPlan"]).as_deref(),
            Some("default_claude_ai")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn decrypts_chromium_v10_cookie_with_host_digest_prefix() {
        let encrypted = test_hex_to_bytes(
            "763130cba8d8b3b813f784aae46dea9258b58b3d19f5f789dc4778df01527afd73e93eaa0590f58c4d6b38d78e1aa843ee5a3cebf07ae55d7ce19bb941b6b37c668fc5",
        );
        let value = decrypt_chromium_v10_cookie(".claude.ai", &encrypted, "test-password")
            .expect("cookie should decrypt");
        assert_eq!(value, "session-test-value");
    }

    #[cfg(target_os = "macos")]
    fn test_hex_to_bytes(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks(2)
            .map(|chunk| {
                let text = std::str::from_utf8(chunk).expect("valid hex");
                u8::from_str_radix(text, 16).expect("valid hex byte")
            })
            .collect()
    }

    fn test_desktop_account(
        id: &str,
        email: &str,
        account_uuid: Option<&str>,
        snapshot_dir: Option<&str>,
        created_at: i64,
        last_used: i64,
    ) -> ClaudeAccount {
        ClaudeAccount {
            id: id.to_string(),
            email: email.to_string(),
            auth_mode: ClaudeAuthMode::DesktopOAuth,
            account_uuid: account_uuid.map(ToString::to_string),
            organization_uuid: None,
            organization_name: None,
            plan_type: None,
            avatar_url: None,
            profile_updated_at: None,
            quota: None,
            quota_error: None,
            usage_updated_at: None,
            status: None,
            status_reason: None,
            api_key: None,
            api_base_url: None,
            api_provider_id: None,
            api_provider_name: None,
            api_provider_source_tag: None,
            api_provider_website: None,
            api_provider_api_key_url: None,
            api_key_field: None,
            api_model_catalog: None,
            api_extra_env: None,
            desktop_profile_dir: snapshot_dir.map(ToString::to_string),
            desktop_profile_imported_at: Some(last_used),
            claude_credentials_raw: None,
            claude_config_raw: None,
            claude_usage_raw: None,
            tags: None,
            account_note: None,
            created_at,
            last_used,
        }
    }

    #[test]
    fn merges_same_desktop_identity_without_touching_non_desktop_accounts() {
        let mut base = test_desktop_account(
            "claude_desktop_old",
            "Claude Desktop",
            Some("B55DE31D-DA47-4433-9A73-BBBA05AFFEEB"),
            Some("/tmp/old-snapshot"),
            10,
            20,
        );
        base.tags = Some(vec!["work".to_string()]);
        base.plan_type = Some("Claude Desktop".to_string());

        let mut incoming = test_desktop_account(
            "claude_desktop_new",
            "alice@testmail.dev",
            Some("b55de31d-da47-4433-9a73-bbba05affeeb"),
            Some("/tmp/new-snapshot"),
            30,
            40,
        );
        incoming.organization_uuid = Some("org-1".to_string());
        incoming.organization_name = Some("Alice Workspace".to_string());
        incoming.plan_type = Some("Max 20x".to_string());
        incoming.avatar_url = Some("https://example.test/avatar.png".to_string());
        incoming.tags = Some(vec!["work".to_string(), "max".to_string()]);

        assert!(desktop_accounts_same_identity(&base, &incoming));

        let mut oauth_account = incoming.clone();
        oauth_account.auth_mode = ClaudeAuthMode::OAuth;
        assert!(!desktop_accounts_same_identity(&base, &oauth_account));

        let merged = merge_desktop_account_fields(&base, &incoming);
        assert_eq!(merged.id, base.id);
        assert_eq!(merged.email, "alice@testmail.dev");
        assert_eq!(
            merged.account_uuid.as_deref(),
            Some("b55de31d-da47-4433-9a73-bbba05affeeb")
        );
        assert_eq!(merged.organization_uuid.as_deref(), Some("org-1"));
        assert_eq!(merged.organization_name.as_deref(), Some("Alice Workspace"));
        assert_eq!(merged.plan_type.as_deref(), Some("Max 20x"));
        assert_eq!(
            merged.avatar_url.as_deref(),
            Some("https://example.test/avatar.png")
        );
        assert_eq!(
            merged.desktop_profile_dir.as_deref(),
            Some("/tmp/new-snapshot")
        );
        assert_eq!(merged.created_at, 10);
        assert_eq!(merged.last_used, 40);
        assert_eq!(
            merged.tags,
            Some(vec!["max".to_string(), "work".to_string()])
        );
    }
}
