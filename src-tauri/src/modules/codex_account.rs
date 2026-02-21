use crate::models::codex::{
    CodexAccount, CodexAccountIndex, CodexAccountSummary, CodexAuthFile, CodexAuthTokens,
    CodexJwtPayload, CodexTokens,
};
use crate::modules::{codex_oauth, logger};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static CODEX_QUOTA_ALERT_LAST_SENT: std::sync::LazyLock<Mutex<HashMap<String, i64>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));
const CODEX_QUOTA_ALERT_COOLDOWN_SECONDS: i64 = 300;

/// 获取 Codex 数据目录
pub fn get_codex_home() -> PathBuf {
    dirs::home_dir().expect("无法获取用户主目录").join(".codex")
}

/// 获取官方 auth.json 路径
pub fn get_auth_json_path() -> PathBuf {
    get_codex_home().join("auth.json")
}

/// 获取我们的多账号存储路径
fn get_accounts_storage_path() -> PathBuf {
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().expect("无法获取用户目录"))
        .join("com.antigravity.cockpit-tools");
    fs::create_dir_all(&data_dir).ok();
    data_dir.join("codex_accounts.json")
}

/// 获取账号详情存储目录
fn get_accounts_dir() -> PathBuf {
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().expect("无法获取用户目录"))
        .join("com.antigravity.cockpit-tools")
        .join("codex_accounts");
    fs::create_dir_all(&data_dir).ok();
    data_dir
}

/// 解析 JWT Token 的 payload
pub fn decode_jwt_payload(token: &str) -> Result<CodexJwtPayload, String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return Err("无效的 JWT Token 格式".to_string());
    }

    let payload_b64 = parts[1];
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|e| format!("Base64 解码失败: {}", e))?;

    let payload: CodexJwtPayload =
        serde_json::from_slice(&payload_bytes).map_err(|e| format!("JSON 解析失败: {}", e))?;

    Ok(payload)
}

fn decode_jwt_payload_value(token: &str) -> Option<serde_json::Value> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    let payload_bytes = URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
    let payload_str = String::from_utf8(payload_bytes).ok()?;
    serde_json::from_str(&payload_str).ok()
}

fn normalize_optional_value(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_optional_ref(value: Option<&str>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub fn extract_chatgpt_account_id_from_access_token(access_token: &str) -> Option<String> {
    let payload = decode_jwt_payload_value(access_token)?;
    let auth_data = payload.get("https://api.openai.com/auth")?;
    normalize_optional_ref(auth_data.get("chatgpt_account_id").and_then(|v| v.as_str()))
}

pub fn extract_chatgpt_organization_id_from_access_token(access_token: &str) -> Option<String> {
    let payload = decode_jwt_payload_value(access_token)?;
    let auth_data = payload.get("https://api.openai.com/auth")?;
    const ORG_KEYS: [&str; 4] = [
        "organization_id",
        "chatgpt_organization_id",
        "chatgpt_org_id",
        "org_id",
    ];
    for key in ORG_KEYS {
        if let Some(value) = normalize_optional_ref(auth_data.get(key).and_then(|v| v.as_str())) {
            return Some(value);
        }
    }
    None
}

fn build_account_storage_id(
    email: &str,
    account_id: Option<&str>,
    organization_id: Option<&str>,
) -> String {
    let mut seed = email.trim().to_string();
    if let Some(id) = normalize_optional_ref(account_id) {
        seed.push('|');
        seed.push_str(&id);
    }
    if let Some(org) = normalize_optional_ref(organization_id) {
        seed.push('|');
        seed.push_str(&org);
    }
    format!("codex_{:x}", md5::compute(seed.as_bytes()))
}

fn find_existing_account_id(
    index: &CodexAccountIndex,
    email: &str,
    account_id: Option<&str>,
    organization_id: Option<&str>,
) -> Option<String> {
    let expected_account_id = normalize_optional_ref(account_id);
    let expected_org_id = normalize_optional_ref(organization_id);
    let mut first_email_match: Option<String> = None;
    let mut email_match_count = 0usize;
    let mut account_id_match_without_org: Option<String> = None;
    let mut legacy_email_only_candidate: Option<String> = None;

    for summary in &index.accounts {
        if !summary.email.eq_ignore_ascii_case(email) {
            continue;
        }
        email_match_count += 1;
        if first_email_match.is_none() {
            first_email_match = Some(summary.id.clone());
        }

        let Some(account) = load_account(&summary.id) else {
            continue;
        };

        let current_account_id = normalize_optional_ref(account.account_id.as_deref());
        let current_org_id = normalize_optional_ref(account.organization_id.as_deref());

        let is_exact_match =
            current_account_id == expected_account_id && current_org_id == expected_org_id;
        if is_exact_match {
            return Some(summary.id.clone());
        }

        if expected_account_id.is_some()
            && current_account_id == expected_account_id
            && current_org_id.is_none()
            && account_id_match_without_org.is_none()
        {
            account_id_match_without_org = Some(summary.id.clone());
        }

        if (expected_account_id.is_some() || expected_org_id.is_some())
            && current_account_id.is_none()
            && current_org_id.is_none()
            && legacy_email_only_candidate.is_none()
        {
            legacy_email_only_candidate = Some(summary.id.clone());
        }
    }

    if expected_account_id.is_some() || expected_org_id.is_some() {
        return account_id_match_without_org.or(legacy_email_only_candidate);
    }

    if email_match_count == 1 {
        return first_email_match;
    }

    None
}

/// 从 id_token 提取用户信息
pub fn extract_user_info(
    id_token: &str,
) -> Result<
    (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ),
    String,
> {
    let payload = decode_jwt_payload(id_token)?;

    let email = payload.email.ok_or("id_token 中缺少 email")?;
    let user_id = payload
        .auth_data
        .as_ref()
        .and_then(|d| d.chatgpt_user_id.clone());
    let plan_type = payload
        .auth_data
        .as_ref()
        .and_then(|d| d.chatgpt_plan_type.clone());
    let account_id = payload
        .auth_data
        .as_ref()
        .and_then(|d| d.account_id.clone());
    let organization_id = payload
        .auth_data
        .as_ref()
        .and_then(|d| d.organization_id.clone());

    Ok((email, user_id, plan_type, account_id, organization_id))
}

/// 读取账号索引
pub fn load_account_index() -> CodexAccountIndex {
    let path = get_accounts_storage_path();
    if !path.exists() {
        return CodexAccountIndex::new();
    }

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| CodexAccountIndex::new()),
        Err(_) => CodexAccountIndex::new(),
    }
}

/// 保存账号索引
pub fn save_account_index(index: &CodexAccountIndex) -> Result<(), String> {
    let path = get_accounts_storage_path();
    let content = serde_json::to_string_pretty(index).map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("写入文件失败: {}", e))?;
    Ok(())
}

/// 读取单个账号详情
pub fn load_account(account_id: &str) -> Option<CodexAccount> {
    let path = get_accounts_dir().join(format!("{}.json", account_id));
    if !path.exists() {
        return None;
    }

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).ok(),
        Err(_) => None,
    }
}

/// 保存单个账号详情
pub fn save_account(account: &CodexAccount) -> Result<(), String> {
    let path = get_accounts_dir().join(format!("{}.json", &account.id));
    let content =
        serde_json::to_string_pretty(account).map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("写入文件失败: {}", e))?;
    Ok(())
}

/// 删除单个账号
pub fn delete_account_file(account_id: &str) -> Result<(), String> {
    let path = get_accounts_dir().join(format!("{}.json", account_id));
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("删除文件失败: {}", e))?;
    }
    Ok(())
}

/// 列出所有账号
pub fn list_accounts() -> Vec<CodexAccount> {
    let index = load_account_index();
    index
        .accounts
        .iter()
        .filter_map(|summary| load_account(&summary.id))
        .collect()
}

/// 添加或更新账号
pub fn upsert_account(tokens: CodexTokens) -> Result<CodexAccount, String> {
    upsert_account_with_hints(tokens, None, None)
}

fn upsert_account_with_hints(
    tokens: CodexTokens,
    account_id_hint: Option<String>,
    organization_id_hint: Option<String>,
) -> Result<CodexAccount, String> {
    let (email, user_id, plan_type, id_token_account_id, id_token_org_id) =
        extract_user_info(&tokens.id_token)?;
    let account_id = normalize_optional_value(
        extract_chatgpt_account_id_from_access_token(&tokens.access_token)
            .or(id_token_account_id)
            .or(account_id_hint),
    );
    let organization_id = normalize_optional_value(
        extract_chatgpt_organization_id_from_access_token(&tokens.access_token)
            .or(id_token_org_id)
            .or(organization_id_hint),
    );

    let mut index = load_account_index();
    let generated_id =
        build_account_storage_id(&email, account_id.as_deref(), organization_id.as_deref());

    // 优先按 email + account_id + organization_id 匹配已有账号；兼容旧数据时回退到 email-only 记录
    let existing_id = find_existing_account_id(
        &index,
        &email,
        account_id.as_deref(),
        organization_id.as_deref(),
    )
    .unwrap_or_else(|| generated_id.clone());
    let existing = index.accounts.iter().position(|a| a.id == existing_id);

    let account = if let Some(pos) = existing {
        // 更新现有账号
        let existing_id = index.accounts[pos].id.clone();
        let mut acc = load_account(&existing_id)
            .unwrap_or_else(|| CodexAccount::new(existing_id, email.clone(), tokens.clone()));
        acc.tokens = tokens;
        acc.user_id = user_id;
        acc.plan_type = plan_type.clone();
        acc.account_id = account_id.clone();
        acc.organization_id = organization_id.clone();
        acc.update_last_used();
        acc
    } else {
        // 创建新账号
        let mut acc = CodexAccount::new(existing_id.clone(), email.clone(), tokens);
        acc.user_id = user_id;
        acc.plan_type = plan_type.clone();
        acc.account_id = account_id.clone();
        acc.organization_id = organization_id.clone();

        index.accounts.retain(|item| item.id != existing_id);
        index.accounts.push(CodexAccountSummary {
            id: existing_id.clone(),
            email: email.clone(),
            plan_type: plan_type.clone(),
            created_at: acc.created_at,
            last_used: acc.last_used,
        });
        acc
    };

    // 保存账号详情
    save_account(&account)?;

    // 更新索引中的摘要信息
    if let Some(summary) = index.accounts.iter_mut().find(|a| a.id == account.id) {
        summary.email = account.email.clone();
        summary.plan_type = account.plan_type.clone();
        summary.last_used = account.last_used;
    } else {
        index.accounts.push(CodexAccountSummary {
            id: account.id.clone(),
            email: account.email.clone(),
            plan_type: account.plan_type.clone(),
            created_at: account.created_at,
            last_used: account.last_used,
        });
    }

    save_account_index(&index)?;

    logger::log_info(&format!(
        "Codex 账号已保存: email={}, account_id={:?}, organization_id={:?}",
        email, account_id, organization_id
    ));

    Ok(account)
}

/// 删除账号
pub fn remove_account(account_id: &str) -> Result<(), String> {
    let mut index = load_account_index();

    // 从索引中移除
    index.accounts.retain(|a| a.id != account_id);

    // 如果删除的是当前账号，清除 current_account_id
    if index.current_account_id.as_deref() == Some(account_id) {
        index.current_account_id = None;
    }

    save_account_index(&index)?;
    delete_account_file(account_id)?;

    Ok(())
}

/// 批量删除账号
pub fn remove_accounts(account_ids: &[String]) -> Result<(), String> {
    for id in account_ids {
        remove_account(id)?;
    }
    Ok(())
}

/// 获取当前激活的账号（基于 auth.json）
pub fn get_current_account() -> Option<CodexAccount> {
    let auth_path = get_auth_json_path();
    if !auth_path.exists() {
        return None;
    }

    let content = fs::read_to_string(&auth_path).ok()?;
    let auth_file: CodexAuthFile = serde_json::from_str(&content).ok()?;

    // 从 id_token 提取 email + 租户信息，优先精确匹配同邮箱下的账号
    let (email, _, _, id_token_account_id, id_token_org_id) =
        extract_user_info(&auth_file.tokens.id_token).ok()?;
    let current_account_id = normalize_optional_value(
        auth_file
            .tokens
            .account_id
            .clone()
            .or_else(|| {
                extract_chatgpt_account_id_from_access_token(&auth_file.tokens.access_token)
            })
            .or(id_token_account_id),
    );
    let current_organization_id = normalize_optional_value(
        extract_chatgpt_organization_id_from_access_token(&auth_file.tokens.access_token)
            .or(id_token_org_id),
    );

    // 在我们的账号列表中查找
    let accounts = list_accounts();
    if let Some(account_id) = current_account_id.as_deref() {
        if let Some(account) = accounts.iter().find(|account| {
            account.email.eq_ignore_ascii_case(&email)
                && normalize_optional_ref(account.account_id.as_deref())
                    == Some(account_id.to_string())
                && (current_organization_id.is_none()
                    || normalize_optional_ref(account.organization_id.as_deref())
                        == current_organization_id.clone())
        }) {
            return Some(account.clone());
        }
    }

    if let Some(organization_id) = current_organization_id.as_deref() {
        if let Some(account) = accounts.iter().find(|account| {
            account.email.eq_ignore_ascii_case(&email)
                && normalize_optional_ref(account.organization_id.as_deref())
                    == Some(organization_id.to_string())
        }) {
            return Some(account.clone());
        }
    }

    accounts
        .into_iter()
        .find(|account| account.email.eq_ignore_ascii_case(&email))
}

fn build_auth_file(account: &CodexAccount) -> CodexAuthFile {
    CodexAuthFile {
        openai_api_key: Some(serde_json::Value::Null),
        tokens: CodexAuthTokens {
            id_token: account.tokens.id_token.clone(),
            access_token: account.tokens.access_token.clone(),
            refresh_token: account.tokens.refresh_token.clone(),
            account_id: account.account_id.clone(),
        },
        last_refresh: Some(serde_json::Value::String(
            chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.6fZ")
                .to_string(),
        )),
    }
}

pub fn write_auth_file_to_dir(base_dir: &Path, account: &CodexAccount) -> Result<(), String> {
    let auth_path = base_dir.join("auth.json");
    if let Some(parent) = auth_path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let auth_file = build_auth_file(account);
    let content =
        serde_json::to_string_pretty(&auth_file).map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(&auth_path, content).map_err(|e| format!("写入 auth.json 失败: {}", e))?;
    Ok(())
}

/// 准备账号注入：如有必要刷新 Token 并写回存储
pub async fn prepare_account_for_injection(account_id: &str) -> Result<CodexAccount, String> {
    let mut account =
        load_account(account_id).ok_or_else(|| format!("账号不存在: {}", account_id))?;
    if codex_oauth::is_token_expired(&account.tokens.access_token) {
        logger::log_info(&format!("账号 {} 的 Token 已过期，尝试刷新", account.email));
        if let Some(ref refresh_token) = account.tokens.refresh_token {
            match codex_oauth::refresh_access_token(refresh_token).await {
                Ok(new_tokens) => {
                    logger::log_info(&format!("账号 {} 的 Token 刷新成功", account.email));
                    account.tokens = new_tokens;
                    save_account(&account)?;
                }
                Err(e) => {
                    logger::log_error(&format!("账号 {} Token 刷新失败: {}", account.email, e));
                    return Err(format!("Token 已过期且刷新失败: {}", e));
                }
            }
        } else {
            return Err("Token 已过期且无 refresh_token，请重新登录".to_string());
        }
    }
    Ok(account)
}

/// 切换账号（写入 auth.json）
pub fn switch_account(account_id: &str) -> Result<CodexAccount, String> {
    let account = load_account(account_id).ok_or_else(|| format!("账号不存在: {}", account_id))?;
    write_auth_file_to_dir(&get_codex_home(), &account)?;

    // 更新索引中的 current_account_id
    let mut index = load_account_index();
    index.current_account_id = Some(account_id.to_string());
    save_account_index(&index)?;

    // 更新账号的 last_used
    let mut updated_account = account.clone();
    updated_account.update_last_used();
    save_account(&updated_account)?;

    logger::log_info(&format!("已切换到 Codex 账号: {}", account.email));

    Ok(updated_account)
}

/// 从本地 auth.json 导入账号
pub fn import_from_local() -> Result<CodexAccount, String> {
    let auth_path = get_auth_json_path();
    if !auth_path.exists() {
        return Err("未找到 ~/.codex/auth.json 文件".to_string());
    }

    let content =
        fs::read_to_string(&auth_path).map_err(|e| format!("读取 auth.json 失败: {}", e))?;

    let auth_file: CodexAuthFile =
        serde_json::from_str(&content).map_err(|e| format!("解析 auth.json 失败: {}", e))?;

    let CodexAuthFile { tokens, .. } = auth_file;
    let account_id_hint = tokens.account_id.clone();
    let tokens = CodexTokens {
        id_token: tokens.id_token,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
    };

    upsert_account_with_hints(tokens, account_id_hint, None)
}

/// 从 JSON 字符串导入账号
pub fn import_from_json(json_content: &str) -> Result<Vec<CodexAccount>, String> {
    // 尝试解析为 auth.json 格式
    if let Ok(auth_file) = serde_json::from_str::<CodexAuthFile>(json_content) {
        let account_id_hint = auth_file.tokens.account_id.clone();
        let tokens = CodexTokens {
            id_token: auth_file.tokens.id_token,
            access_token: auth_file.tokens.access_token,
            refresh_token: auth_file.tokens.refresh_token,
        };
        let account = upsert_account_with_hints(tokens, account_id_hint, None)?;
        return Ok(vec![account]);
    }

    // 尝试解析为账号数组
    if let Ok(accounts) = serde_json::from_str::<Vec<CodexAccount>>(json_content) {
        let mut result = Vec::new();
        for acc in accounts {
            let imported = upsert_account(acc.tokens)?;
            result.push(imported);
        }
        return Ok(result);
    }

    Err("无法解析 JSON 内容".to_string())
}

/// 导出账号为 JSON
pub fn export_accounts(account_ids: &[String]) -> Result<String, String> {
    let accounts: Vec<CodexAccount> = account_ids
        .iter()
        .filter_map(|id| load_account(id))
        .collect();

    serde_json::to_string_pretty(&accounts).map_err(|e| format!("序列化失败: {}", e))
}

pub fn update_account_tags(account_id: &str, tags: Vec<String>) -> Result<CodexAccount, String> {
    let mut account =
        load_account(account_id).ok_or_else(|| format!("账号不存在: {}", account_id))?;

    account.tags = Some(tags);
    save_account(&account)?;

    Ok(account)
}

fn normalize_quota_alert_threshold(raw: i32) -> i32 {
    raw.clamp(0, 100)
}

fn format_codex_quota_metric_label(window_minutes: Option<i64>, fallback: &str) -> String {
    const HOUR_MINUTES: i64 = 60;
    const DAY_MINUTES: i64 = 24 * HOUR_MINUTES;
    const WEEK_MINUTES: i64 = 7 * DAY_MINUTES;

    let Some(minutes) = window_minutes.filter(|value| *value > 0) else {
        return fallback.to_string();
    };

    if minutes >= WEEK_MINUTES - 1 {
        let weeks = (minutes + WEEK_MINUTES - 1) / WEEK_MINUTES;
        return if weeks <= 1 {
            "Weekly".to_string()
        } else {
            format!("{} Week", weeks)
        };
    }

    if minutes >= DAY_MINUTES - 1 {
        let days = (minutes + DAY_MINUTES - 1) / DAY_MINUTES;
        return format!("{}d", days);
    }

    if minutes >= HOUR_MINUTES {
        let hours = (minutes + HOUR_MINUTES - 1) / HOUR_MINUTES;
        return format!("{}h", hours);
    }

    format!("{}m", minutes)
}

fn extract_quota_metrics(account: &CodexAccount) -> Vec<(String, i32)> {
    let Some(quota) = account.quota.as_ref() else {
        return Vec::new();
    };

    let has_presence =
        quota.hourly_window_present.is_some() || quota.weekly_window_present.is_some();
    let mut metrics = Vec::new();

    if !has_presence || quota.hourly_window_present.unwrap_or(false) {
        metrics.push((
            format_codex_quota_metric_label(quota.hourly_window_minutes, "5h"),
            quota.hourly_percentage.clamp(0, 100),
        ));
    }

    if !has_presence || quota.weekly_window_present.unwrap_or(false) {
        metrics.push((
            format_codex_quota_metric_label(quota.weekly_window_minutes, "Weekly"),
            quota.weekly_percentage.clamp(0, 100),
        ));
    }

    if metrics.is_empty() {
        metrics.push((
            format_codex_quota_metric_label(quota.hourly_window_minutes, "5h"),
            quota.hourly_percentage.clamp(0, 100),
        ));
    }

    metrics
}

fn average_quota_percentage(metrics: &[(String, i32)]) -> f64 {
    if metrics.is_empty() {
        return 0.0;
    }
    let sum: i32 = metrics.iter().map(|(_, pct)| *pct).sum();
    sum as f64 / metrics.len() as f64
}

fn build_quota_alert_cooldown_key(account_id: &str, threshold: i32) -> String {
    format!("codex:{}:{}", account_id, threshold)
}

fn should_emit_quota_alert(cooldown_key: &str, now: i64) -> bool {
    let Ok(mut state) = CODEX_QUOTA_ALERT_LAST_SENT.lock() else {
        return true;
    };

    if let Some(last_sent) = state.get(cooldown_key) {
        if now - *last_sent < CODEX_QUOTA_ALERT_COOLDOWN_SECONDS {
            return false;
        }
    }

    state.insert(cooldown_key.to_string(), now);
    true
}

fn clear_quota_alert_cooldown(account_id: &str, threshold: i32) {
    if let Ok(mut state) = CODEX_QUOTA_ALERT_LAST_SENT.lock() {
        state.remove(&build_quota_alert_cooldown_key(account_id, threshold));
    }
}

fn resolve_current_account_id(accounts: &[CodexAccount]) -> Option<String> {
    if let Some(account) = get_current_account() {
        return Some(account.id);
    }

    if let Ok(settings) = crate::modules::codex_instance::load_default_settings() {
        if let Some(bind_id) = settings.bind_account_id {
            let trimmed = bind_id.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    accounts
        .iter()
        .max_by_key(|account| account.last_used)
        .map(|account| account.id.clone())
}

fn pick_quota_alert_recommendation(
    accounts: &[CodexAccount],
    current_id: &str,
) -> Option<CodexAccount> {
    let mut candidates: Vec<CodexAccount> = accounts
        .iter()
        .filter(|account| account.id != current_id)
        .filter(|account| !extract_quota_metrics(account).is_empty())
        .cloned()
        .collect();

    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by(|a, b| {
        let avg_a = average_quota_percentage(&extract_quota_metrics(a));
        let avg_b = average_quota_percentage(&extract_quota_metrics(b));
        avg_b
            .partial_cmp(&avg_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.last_used.cmp(&b.last_used))
    });

    candidates.into_iter().next()
}

pub fn run_quota_alert_if_needed(
) -> Result<Option<crate::modules::account::QuotaAlertPayload>, String> {
    let cfg = crate::modules::config::get_user_config();
    if !cfg.codex_quota_alert_enabled {
        return Ok(None);
    }

    let threshold = normalize_quota_alert_threshold(cfg.codex_quota_alert_threshold);
    let accounts = list_accounts();
    let current_id = match resolve_current_account_id(&accounts) {
        Some(id) => id,
        None => return Ok(None),
    };

    let current = match accounts.iter().find(|account| account.id == current_id) {
        Some(account) => account,
        None => return Ok(None),
    };

    let metrics = extract_quota_metrics(current);
    let low_models: Vec<(String, i32)> = metrics
        .into_iter()
        .filter(|(_, pct)| *pct <= threshold)
        .collect();

    if low_models.is_empty() {
        clear_quota_alert_cooldown(&current_id, threshold);
        return Ok(None);
    }

    let now = chrono::Utc::now().timestamp();
    let cooldown_key = build_quota_alert_cooldown_key(&current_id, threshold);
    if !should_emit_quota_alert(&cooldown_key, now) {
        return Ok(None);
    }

    let recommendation = pick_quota_alert_recommendation(&accounts, &current_id);
    let lowest_percentage = low_models.iter().map(|(_, pct)| *pct).min().unwrap_or(0);
    let payload = crate::modules::account::QuotaAlertPayload {
        platform: "codex".to_string(),
        current_account_id: current_id,
        current_email: current.email.clone(),
        threshold,
        lowest_percentage,
        low_models: low_models.into_iter().map(|(name, _)| name).collect(),
        recommended_account_id: recommendation.as_ref().map(|account| account.id.clone()),
        recommended_email: recommendation.as_ref().map(|account| account.email.clone()),
        triggered_at: now,
    };

    crate::modules::account::dispatch_quota_alert(&payload);
    Ok(Some(payload))
}
