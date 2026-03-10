use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;
use tauri::Emitter;

use crate::models::codebuddy::{
    CodebuddyAccount, CodebuddyAccountIndex, CodebuddyOAuthCompletePayload, CodebuddyQuotaBinding,
    CodebuddyQuotaRequestHeaders,
};
use crate::modules::{account, codebuddy_oauth, logger};

const ACCOUNTS_INDEX_FILE: &str = "codebuddy_accounts.json";
const ACCOUNTS_DIR: &str = "codebuddy_accounts";
const CODEBUDDY_QUOTA_ALERT_COOLDOWN_SECONDS: i64 = 10 * 60;
const CODEBUDDY_QUOTA_BINDING_ACCOUNT_MISMATCH_CODE: &str =
    "CODEBUDDY_QUOTA_BINDING_ACCOUNT_MISMATCH";
const CODEBUDDY_SECRET_EXTENSION_ID: &str = "tencent-cloud.coding-copilot";
const CODEBUDDY_SECRET_KEY: &str = "planning-genie.new.accessToken";

lazy_static::lazy_static! {
    static ref CODEBUDDY_ACCOUNT_INDEX_LOCK: Mutex<()> = Mutex::new(());
    static ref CODEBUDDY_QUOTA_ALERT_LAST_SENT: Mutex<HashMap<String, i64>> = Mutex::new(HashMap::new());
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

fn mark_quota_query_failure(account: &mut CodebuddyAccount, reason: &str) {
    let trimmed = reason.trim();
    if trimmed.is_empty() {
        account.quota_query_last_error = Some("unknown error".to_string());
    } else {
        account.quota_query_last_error = Some(trimmed.to_string());
    }
    account.quota_query_last_error_at = Some(chrono::Utc::now().timestamp_millis());
}

fn get_data_dir() -> Result<PathBuf, String> {
    account::get_data_dir()
}

fn get_accounts_dir() -> Result<PathBuf, String> {
    let base = get_data_dir()?;
    let dir = base.join(ACCOUNTS_DIR);
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| format!("创建 CodeBuddy 账号目录失败: {}", e))?;
    }
    Ok(dir)
}

fn get_accounts_index_path() -> Result<PathBuf, String> {
    Ok(get_data_dir()?.join(ACCOUNTS_INDEX_FILE))
}

pub fn accounts_index_path_string() -> Result<String, String> {
    Ok(get_accounts_index_path()?.to_string_lossy().to_string())
}

fn normalize_account_id(account_id: &str) -> Result<String, String> {
    let trimmed = account_id.trim();
    if trimmed.is_empty() {
        return Err("账号 ID 不能为空".to_string());
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err("账号 ID 非法，包含路径字符".to_string());
    }
    let valid = trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.');
    if !valid {
        return Err("账号 ID 非法，仅允许字母/数字/._-".to_string());
    }
    Ok(trimmed.to_string())
}

fn resolve_account_file_path(account_id: &str) -> Result<PathBuf, String> {
    let normalized = normalize_account_id(account_id)?;
    Ok(get_accounts_dir()?.join(format!("{}.json", normalized)))
}

pub fn load_account(account_id: &str) -> Option<CodebuddyAccount> {
    let account_path = resolve_account_file_path(account_id).ok()?;
    if !account_path.exists() {
        return None;
    }
    let content = fs::read_to_string(account_path).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_account_file(account: &CodebuddyAccount) -> Result<(), String> {
    let path = resolve_account_file_path(account.id.as_str())?;
    let content =
        serde_json::to_string_pretty(account).map_err(|e| format!("序列化账号失败: {}", e))?;
    fs::write(path, content).map_err(|e| format!("保存账号失败: {}", e))
}

fn delete_account_file(account_id: &str) -> Result<(), String> {
    let path = resolve_account_file_path(account_id)?;
    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("删除账号文件失败: {}", e))?;
    }
    Ok(())
}

fn load_account_index() -> CodebuddyAccountIndex {
    let path = match get_accounts_index_path() {
        Ok(p) => p,
        Err(_) => return CodebuddyAccountIndex::new(),
    };
    if !path.exists() {
        return CodebuddyAccountIndex::new();
    }
    match fs::read_to_string(path) {
        Ok(content) => {
            serde_json::from_str(&content).unwrap_or_else(|_| CodebuddyAccountIndex::new())
        }
        Err(_) => CodebuddyAccountIndex::new(),
    }
}

fn save_account_index(index: &CodebuddyAccountIndex) -> Result<(), String> {
    let path = get_accounts_index_path()?;
    let content =
        serde_json::to_string_pretty(index).map_err(|e| format!("序列化账号索引失败: {}", e))?;
    fs::write(path, content).map_err(|e| format!("写入账号索引失败: {}", e))
}

fn refresh_summary(index: &mut CodebuddyAccountIndex, account: &CodebuddyAccount) {
    if let Some(summary) = index.accounts.iter_mut().find(|item| item.id == account.id) {
        *summary = account.summary();
        return;
    }
    index.accounts.push(account.summary());
}

fn upsert_account_record(account: CodebuddyAccount) -> Result<CodebuddyAccount, String> {
    let _lock = CODEBUDDY_ACCOUNT_INDEX_LOCK
        .lock()
        .map_err(|_| "获取 CodeBuddy 账号锁失败".to_string())?;
    let mut index = load_account_index();
    save_account_file(&account)?;
    refresh_summary(&mut index, &account);
    save_account_index(&index)?;
    Ok(account)
}

fn normalize_non_empty(value: Option<&str>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_identity(value: Option<&str>) -> Option<String> {
    normalize_non_empty(value).map(|v| v.to_lowercase())
}

fn normalize_email_identity(value: Option<&str>) -> Option<String> {
    normalize_non_empty(value).and_then(|raw| {
        let lowered = raw.to_lowercase();
        if lowered.contains('@') {
            Some(lowered)
        } else {
            None
        }
    })
}

fn split_shell_like_args(raw: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in raw.chars() {
        if escaped {
            if !matches!(ch, '\n' | '\r') {
                current.push(ch);
            }
            escaped = false;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '\'' | '"' => quote = Some(ch),
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    args
}

#[derive(Debug, Clone)]
struct ParsedCodebuddyQuotaCurlRequest {
    request_url: String,
    request_method: String,
    request_headers_for_replay: Vec<(String, String)>,
    request_headers_snapshot: Option<CodebuddyQuotaRequestHeaders>,
    request_body: Option<String>,
    normalized_cookie_header: String,
    user_agent: Option<String>,
    product_code: Option<String>,
    status: Option<Vec<i32>>,
    package_end_time_range_begin: Option<String>,
    package_end_time_range_end: Option<String>,
    page_number: Option<i32>,
    page_size: Option<i32>,
}

fn normalize_windows_curl_command(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '^' {
            out.push(ch);
            continue;
        }
        match chars.peek().copied() {
            Some('\r') => {
                chars.next();
                if matches!(chars.peek(), Some('\n')) {
                    chars.next();
                }
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            Some('\n') => {
                chars.next();
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            Some(next) => {
                out.push(next);
                chars.next();
            }
            None => {}
        }
    }
    out
}

fn canonicalize_curl_command(raw: &str) -> String {
    normalize_windows_curl_command(raw)
        .replace("\\\r\n", " ")
        .replace("\\\n", " ")
        .replace("\\\r", " ")
}

fn is_curl_command(raw: &str) -> bool {
    let canonical = canonicalize_curl_command(raw);
    let first = canonical
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    first == "curl" || first == "curl.exe"
}

fn parse_header_kv(raw: &str) -> Option<(String, String)> {
    let mut parts = raw.splitn(2, ':');
    let key = parts.next()?.trim();
    let value = parts.next()?.trim();
    if key.is_empty() || value.is_empty() {
        return None;
    }
    Some((key.to_string(), value.to_string()))
}

fn pick_header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .rev()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, value)| normalize_non_empty(Some(value)))
}

fn parse_body_string_field(data: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        data.get(key).and_then(|value| match value {
            Value::String(s) => normalize_non_empty(Some(s)),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
    })
}

fn parse_body_i32_field(data: &Value, keys: &[&str]) -> Option<i32> {
    keys.iter().find_map(|key| {
        data.get(key).and_then(|value| match value {
            Value::Number(n) => n.as_i64().and_then(|v| i32::try_from(v).ok()),
            Value::String(s) => s.trim().parse::<i32>().ok(),
            _ => None,
        })
    })
}

fn parse_body_status_field(data: &Value, keys: &[&str]) -> Option<Vec<i32>> {
    keys.iter().find_map(|key| {
        data.get(key).and_then(|value| {
            let arr = value.as_array()?;
            let mut parsed = Vec::new();
            for item in arr {
                let parsed_item = match item {
                    Value::Number(n) => n.as_i64().and_then(|v| i32::try_from(v).ok()),
                    Value::String(s) => s.trim().parse::<i32>().ok(),
                    _ => None,
                };
                if let Some(v) = parsed_item {
                    parsed.push(v);
                }
            }
            if parsed.is_empty() {
                None
            } else {
                Some(parsed)
            }
        })
    })
}

fn parse_codebuddy_quota_curl_request(raw: &str) -> Result<ParsedCodebuddyQuotaCurlRequest, String> {
    if !is_curl_command(raw) {
        return Err(
            "仅支持完整 cURL 命令。请在浏览器 Network 中对 get-user-resource 使用“Copy as cURL”后原样粘贴。"
                .to_string(),
        );
    }

    let canonical = canonicalize_curl_command(raw);
    let tokens = split_shell_like_args(&canonical);
    if tokens.is_empty() {
        return Err("cURL 命令为空".to_string());
    }

    let first = tokens
        .first()
        .map(|v| v.to_ascii_lowercase())
        .unwrap_or_default();
    if first != "curl" && first != "curl.exe" {
        return Err("请输入以 curl 开头的完整命令".to_string());
    }

    let mut url: Option<String> = None;
    let mut method: Option<String> = None;
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut cookie_from_arg: Option<String> = None;
    let mut data_parts: Vec<String> = Vec::new();
    let mut idx = 1usize;

    while idx < tokens.len() {
        let token = tokens[idx].trim().to_string();
        let lower = token.to_ascii_lowercase();

        if lower == "-x" || lower == "--request" {
            let value = tokens
                .get(idx + 1)
                .and_then(|v| normalize_non_empty(Some(v)))
                .ok_or_else(|| "cURL 参数 --request 缺少值".to_string())?;
            method = Some(value.to_ascii_uppercase());
            idx += 2;
            continue;
        }

        if let Some(value) = token.strip_prefix("--request=") {
            let parsed = normalize_non_empty(Some(value))
                .ok_or_else(|| "cURL 参数 --request 缺少值".to_string())?;
            method = Some(parsed.to_ascii_uppercase());
            idx += 1;
            continue;
        }

        if lower == "-h" || lower == "--header" {
            let header_raw = tokens
                .get(idx + 1)
                .and_then(|v| normalize_non_empty(Some(v)))
                .ok_or_else(|| "cURL 参数 --header 缺少值".to_string())?;
            if let Some((k, v)) = parse_header_kv(&header_raw) {
                headers.push((k, v));
            }
            idx += 2;
            continue;
        }

        if let Some(value) = token.strip_prefix("--header=") {
            if let Some((k, v)) = parse_header_kv(value) {
                headers.push((k, v));
            }
            idx += 1;
            continue;
        }

        if lower == "-b" || lower == "--cookie" {
            let cookie = tokens
                .get(idx + 1)
                .and_then(|v| normalize_non_empty(Some(v)))
                .ok_or_else(|| "cURL 参数 --cookie 缺少值".to_string())?;
            cookie_from_arg = Some(cookie);
            idx += 2;
            continue;
        }

        if let Some(value) = token.strip_prefix("--cookie=") {
            if let Some(cookie) = normalize_non_empty(Some(value)) {
                cookie_from_arg = Some(cookie);
            }
            idx += 1;
            continue;
        }

        let is_data_flag = matches!(
            lower.as_str(),
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-urlencode"
        );
        if is_data_flag {
            let data = tokens
                .get(idx + 1)
                .and_then(|v| normalize_non_empty(Some(v)))
                .ok_or_else(|| format!("cURL 参数 {} 缺少值", token))?;
            data_parts.push(data);
            idx += 2;
            continue;
        }

        let data_prefixes = [
            "--data=",
            "--data-raw=",
            "--data-binary=",
            "--data-urlencode=",
        ];
        if let Some(prefix) = data_prefixes
            .iter()
            .find(|prefix| token.starts_with(**prefix))
        {
            let value = token.strip_prefix(prefix).unwrap_or("");
            if let Some(data) = normalize_non_empty(Some(value)) {
                data_parts.push(data);
            }
            idx += 1;
            continue;
        }

        if lower == "--url" {
            let parsed = tokens
                .get(idx + 1)
                .and_then(|v| normalize_non_empty(Some(v)))
                .ok_or_else(|| "cURL 参数 --url 缺少值".to_string())?;
            url = Some(parsed);
            idx += 2;
            continue;
        }

        if let Some(value) = token.strip_prefix("--url=") {
            let parsed =
                normalize_non_empty(Some(value)).ok_or_else(|| "cURL 参数 --url 缺少值".to_string())?;
            url = Some(parsed);
            idx += 1;
            continue;
        }

        if token.starts_with('-') {
            idx += 1;
            continue;
        }

        if url.is_none() {
            url = normalize_non_empty(Some(&token));
        }
        idx += 1;
    }

    let request_url = url.ok_or_else(|| "cURL 命令中未找到请求 URL".to_string())?;
    let mut request_headers_for_replay = headers.clone();
    let cookie_from_header = pick_header_value(&request_headers_for_replay, "Cookie");
    let cookie_raw = cookie_from_arg.or(cookie_from_header).ok_or_else(|| {
        "cURL 命令缺少 Cookie（需包含 session 与 session_2）".to_string()
    })?;
    let normalized_cookie_header = normalize_session_cookie_header(&cookie_raw)?;

    if pick_header_value(&request_headers_for_replay, "Cookie").is_none() {
        request_headers_for_replay.push(("Cookie".to_string(), cookie_raw));
    }

    let request_method = method.unwrap_or_else(|| {
        if data_parts.is_empty() {
            "GET".to_string()
        } else {
            "POST".to_string()
        }
    });
    let request_body = if data_parts.is_empty() {
        None
    } else {
        Some(data_parts.join("&"))
    };

    let request_headers_snapshot = {
        let snapshot = CodebuddyQuotaRequestHeaders {
            accept: pick_header_value(&request_headers_for_replay, "Accept"),
            accept_language: pick_header_value(&request_headers_for_replay, "Accept-Language"),
            content_type: pick_header_value(&request_headers_for_replay, "Content-Type"),
            origin: pick_header_value(&request_headers_for_replay, "Origin"),
            referer: pick_header_value(&request_headers_for_replay, "Referer"),
            user_agent: pick_header_value(&request_headers_for_replay, "User-Agent"),
            sec_fetch_site: pick_header_value(&request_headers_for_replay, "Sec-Fetch-Site"),
            sec_fetch_mode: pick_header_value(&request_headers_for_replay, "Sec-Fetch-Mode"),
            sec_fetch_dest: pick_header_value(&request_headers_for_replay, "Sec-Fetch-Dest"),
        };
        if snapshot.is_empty() {
            None
        } else {
            Some(snapshot)
        }
    };
    let user_agent = request_headers_snapshot
        .as_ref()
        .and_then(|headers| headers.user_agent.clone());

    let mut product_code = None;
    let mut status = None;
    let mut package_end_time_range_begin = None;
    let mut package_end_time_range_end = None;
    let mut page_number = None;
    let mut page_size = None;
    if let Some(body) = request_body.as_deref() {
        if let Ok(json) = serde_json::from_str::<Value>(body) {
            product_code = parse_body_string_field(&json, &["ProductCode", "productCode", "product_code"]);
            status = parse_body_status_field(&json, &["Status", "status"]);
            package_end_time_range_begin = parse_body_string_field(
                &json,
                &["PackageEndTimeRangeBegin", "packageEndTimeRangeBegin", "package_end_time_range_begin"],
            );
            package_end_time_range_end = parse_body_string_field(
                &json,
                &["PackageEndTimeRangeEnd", "packageEndTimeRangeEnd", "package_end_time_range_end"],
            );
            page_number = parse_body_i32_field(&json, &["PageNumber", "pageNumber", "page_number"]);
            page_size = parse_body_i32_field(&json, &["PageSize", "pageSize", "page_size"]);
        }
    }

    Ok(ParsedCodebuddyQuotaCurlRequest {
        request_url,
        request_method,
        request_headers_for_replay,
        request_headers_snapshot,
        request_body,
        normalized_cookie_header,
        user_agent,
        product_code,
        status,
        package_end_time_range_begin,
        package_end_time_range_end,
        page_number,
        page_size,
    })
}

fn extract_cookie_from_header_arg(raw: &str) -> Option<String> {
    let mut parts = raw.splitn(2, ':');
    let key = parts.next()?.trim();
    if !key.eq_ignore_ascii_case("cookie") {
        return None;
    }
    parts
        .next()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn extract_cookie_blob(raw: &str) -> String {
    let trimmed = raw.trim();
    if !is_curl_command(trimmed) {
        return trimmed.to_string();
    }

    let canonical = canonicalize_curl_command(trimmed);
    let tokens = split_shell_like_args(&canonical);
    if tokens.is_empty() {
        return trimmed.to_string();
    }

    for idx in 0..tokens.len() {
        let token = tokens[idx].trim();
        if token.eq_ignore_ascii_case("-b") || token.eq_ignore_ascii_case("--cookie") {
            if let Some(next) = tokens
                .get(idx + 1)
                .and_then(|s| normalize_non_empty(Some(s)))
            {
                return next;
            }
            continue;
        }
        if let Some(value) = token.strip_prefix("--cookie=") {
            if let Some(cookie) = normalize_non_empty(Some(value)) {
                return cookie;
            }
            continue;
        }
        if token.eq_ignore_ascii_case("-H") || token.eq_ignore_ascii_case("--header") {
            if let Some(next) = tokens
                .get(idx + 1)
                .and_then(|s| extract_cookie_from_header_arg(s))
            {
                return next;
            }
            continue;
        }
        if let Some(value) = token.strip_prefix("--header=") {
            if let Some(cookie) = extract_cookie_from_header_arg(value) {
                return cookie;
            }
        }
    }

    trimmed.to_string()
}

fn normalize_session_cookie_header(raw: &str) -> Result<String, String> {
    let cookie_blob = extract_cookie_blob(raw);
    if cookie_blob.is_empty() {
        return Err("Cookie Header 不能为空".to_string());
    }
    let mut seen = HashSet::new();
    let mut merged = Vec::new();
    let mut has_session = false;
    let mut has_session_2 = false;

    for segment in cookie_blob.split(';') {
        let part = segment.trim();
        if part.is_empty() {
            continue;
        }
        let mut pieces = part.splitn(2, '=');
        let key = pieces.next().unwrap_or("").trim();
        let value = pieces.next().unwrap_or("").trim();
        if key.is_empty() || value.is_empty() {
            continue;
        }
        let lowered = key.to_lowercase();
        if seen.insert(lowered) {
            merged.push(format!("{}={}", key, value));
            let name = key.to_lowercase();
            if name == "session" {
                has_session = true;
            } else if name == "session_2" {
                has_session_2 = true;
            }
        }
    }

    if merged.is_empty() {
        return Err("Cookie Header 不能为空".to_string());
    }
    if !has_session || !has_session_2 {
        return Err("Cookie Header 缺少 session/session_2，无法查询配额".to_string());
    }

    Ok(merged.join("; "))
}

fn normalize_status_list(status: Option<Vec<i32>>) -> Vec<i32> {
    let mut list = status.unwrap_or_else(|| vec![0, 3]);
    list.retain(|v| *v >= 0);
    if list.is_empty() {
        return vec![0, 3];
    }
    list.sort_unstable();
    list.dedup();
    list
}

fn build_default_time_range() -> (String, String) {
    let now = chrono::Local::now();
    let begin = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let end = (now + chrono::Duration::days(365 * 100))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    (begin, end)
}

fn value_to_non_empty_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn extract_profile_uin(profile_raw: Option<&Value>) -> Option<String> {
    profile_raw.and_then(|profile| {
        profile
            .get("uin")
            .and_then(value_to_non_empty_string)
            .or_else(|| {
                profile
                    .get("data")
                    .and_then(|v| v.get("uin"))
                    .and_then(value_to_non_empty_string)
            })
    })
}

fn extract_user_resource_accounts(user_resource: &Value) -> Vec<&Value> {
    user_resource
        .get("data")
        .and_then(|v| v.get("Response"))
        .and_then(|v| v.get("Data"))
        .and_then(|v| v.get("Accounts"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().collect())
        .unwrap_or_default()
}

fn collect_user_resource_uins(user_resource: &Value) -> HashSet<String> {
    let mut result = HashSet::new();
    for account in extract_user_resource_accounts(user_resource) {
        if let Some(uin) = account.get("Uin").and_then(value_to_non_empty_string) {
            result.insert(uin);
        }
    }
    result
}

fn collect_user_resource_payer_uins(user_resource: &Value) -> HashSet<String> {
    let mut result = HashSet::new();
    for account in extract_user_resource_accounts(user_resource) {
        if let Some(attributes) = account.get("AccountAttributes").and_then(|v| v.as_array()) {
            for attribute in attributes {
                let key = attribute
                    .get("Key")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_lowercase());
                if key.as_deref() != Some("payeruin") {
                    continue;
                }
                if let Some(value) = attribute.get("Value").and_then(value_to_non_empty_string) {
                    result.insert(value);
                }
            }
        }
    }
    result
}

fn join_uin_set(values: &HashSet<String>) -> String {
    let mut items: Vec<String> = values.iter().cloned().collect();
    items.sort_unstable();
    items.into_iter().take(3).collect::<Vec<_>>().join(",")
}

fn account_matches_payload_identity(
    existing_uid: Option<&String>,
    existing_email: Option<&String>,
    incoming_uid: Option<&String>,
    incoming_email: Option<&String>,
) -> bool {
    if let (Some(existing), Some(incoming)) = (existing_uid, incoming_uid) {
        if existing == incoming {
            return true;
        }
    }
    if let (Some(existing), Some(incoming)) = (existing_email, incoming_email) {
        if existing == incoming {
            if let (Some(eu), Some(iu)) = (existing_uid, incoming_uid) {
                if eu != iu {
                    return false;
                }
            }
            return true;
        }
    }
    false
}

fn accounts_are_duplicates(left: &CodebuddyAccount, right: &CodebuddyAccount) -> bool {
    let left_uid = normalize_identity(left.uid.as_deref());
    let right_uid = normalize_identity(right.uid.as_deref());
    let left_email = normalize_email_identity(Some(left.email.as_str()));
    let right_email = normalize_email_identity(Some(right.email.as_str()));

    let uid_conflict = matches!(
        (left_uid.as_ref(), right_uid.as_ref()),
        (Some(l), Some(r)) if l != r
    );
    let email_conflict = matches!(
        (left_email.as_ref(), right_email.as_ref()),
        (Some(l), Some(r)) if l != r
    );
    if uid_conflict || email_conflict {
        return false;
    }

    let uid_match = matches!(
        (left_uid.as_ref(), right_uid.as_ref()),
        (Some(l), Some(r)) if l == r
    );
    let email_match = matches!(
        (left_email.as_ref(), right_email.as_ref()),
        (Some(l), Some(r)) if l == r
    );

    uid_match || email_match
}

fn merge_string_list(
    primary: Option<Vec<String>>,
    secondary: Option<Vec<String>>,
) -> Option<Vec<String>> {
    let mut merged = Vec::new();
    let mut seen = HashSet::new();
    for source in [primary, secondary] {
        if let Some(values) = source {
            for value in values {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let key = trimmed.to_lowercase();
                if seen.insert(key) {
                    merged.push(trimmed.to_string());
                }
            }
        }
    }
    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

fn fill_if_none<T: Clone>(target: &mut Option<T>, source: &Option<T>) {
    if target.is_none() {
        *target = source.clone();
    }
}

fn merge_duplicate_account(primary: &mut CodebuddyAccount, dup: &CodebuddyAccount) {
    if primary.email.trim().is_empty() && !dup.email.trim().is_empty() {
        primary.email = dup.email.clone();
    }
    if primary.access_token.trim().is_empty() && !dup.access_token.trim().is_empty() {
        primary.access_token = dup.access_token.clone();
    }
    fill_if_none(&mut primary.uid, &dup.uid);
    fill_if_none(&mut primary.nickname, &dup.nickname);
    fill_if_none(&mut primary.enterprise_id, &dup.enterprise_id);
    fill_if_none(&mut primary.enterprise_name, &dup.enterprise_name);
    fill_if_none(&mut primary.refresh_token, &dup.refresh_token);
    fill_if_none(&mut primary.token_type, &dup.token_type);
    fill_if_none(&mut primary.expires_at, &dup.expires_at);
    fill_if_none(&mut primary.domain, &dup.domain);
    fill_if_none(&mut primary.plan_type, &dup.plan_type);
    fill_if_none(&mut primary.dosage_notify_code, &dup.dosage_notify_code);
    fill_if_none(&mut primary.payment_type, &dup.payment_type);
    fill_if_none(&mut primary.quota_raw, &dup.quota_raw);
    fill_if_none(&mut primary.auth_raw, &dup.auth_raw);
    fill_if_none(&mut primary.profile_raw, &dup.profile_raw);
    fill_if_none(&mut primary.usage_raw, &dup.usage_raw);
    fill_if_none(&mut primary.quota_binding, &dup.quota_binding);
    fill_if_none(&mut primary.status, &dup.status);
    fill_if_none(
        &mut primary.quota_query_last_error,
        &dup.quota_query_last_error,
    );
    fill_if_none(
        &mut primary.quota_query_last_error_at,
        &dup.quota_query_last_error_at,
    );
    primary.tags = merge_string_list(primary.tags.clone(), dup.tags.clone());
    primary.created_at = primary.created_at.min(dup.created_at);
    primary.last_used = primary.last_used.max(dup.last_used);
}

fn choose_primary_account_index(group: &[usize], accounts: &[CodebuddyAccount]) -> usize {
    group
        .iter()
        .copied()
        .max_by(|l, r| {
            accounts[*l]
                .last_used
                .cmp(&accounts[*r].last_used)
                .then_with(|| accounts[*r].created_at.cmp(&accounts[*l].created_at))
        })
        .unwrap_or(group[0])
}

fn normalize_account_index(index: &mut CodebuddyAccountIndex) -> Vec<CodebuddyAccount> {
    let mut loaded = Vec::new();
    let mut seen = HashSet::new();
    for summary in &index.accounts {
        if !seen.insert(summary.id.clone()) {
            continue;
        }
        if let Some(account) = load_account(&summary.id) {
            loaded.push(account);
        }
    }
    if loaded.len() <= 1 {
        index.accounts = loaded.iter().map(|a| a.summary()).collect();
        return loaded;
    }

    let mut parents: Vec<usize> = (0..loaded.len()).collect();
    fn find(parents: &mut [usize], idx: usize) -> usize {
        let p = parents[idx];
        if p == idx {
            return idx;
        }
        let root = find(parents, p);
        parents[idx] = root;
        root
    }
    fn union(parents: &mut [usize], l: usize, r: usize) {
        let lr = find(parents, l);
        let rr = find(parents, r);
        if lr != rr {
            parents[rr] = lr;
        }
    }

    let total = loaded.len();
    for l in 0..total {
        for r in (l + 1)..total {
            if accounts_are_duplicates(&loaded[l], &loaded[r]) {
                union(&mut parents, l, r);
            }
        }
    }

    let mut grouped: HashMap<usize, Vec<usize>> = HashMap::new();
    for idx in 0..total {
        let root = find(&mut parents, idx);
        grouped.entry(root).or_default().push(idx);
    }

    let mut processed = HashSet::new();
    let mut normalized = Vec::new();
    let mut removed_ids = Vec::new();
    for idx in 0..total {
        let root = find(&mut parents, idx);
        if !processed.insert(root) {
            continue;
        }
        let Some(group) = grouped.get(&root) else {
            continue;
        };
        if group.len() == 1 {
            normalized.push(loaded[group[0]].clone());
            continue;
        }
        let primary_idx = choose_primary_account_index(group, &loaded);
        let mut primary = loaded[primary_idx].clone();
        for member in group {
            if *member == primary_idx {
                continue;
            }
            merge_duplicate_account(&mut primary, &loaded[*member]);
            removed_ids.push(loaded[*member].id.clone());
        }
        normalized.push(primary);
    }

    if !removed_ids.is_empty() {
        for acc in &normalized {
            let _ = save_account_file(acc);
        }
        for id in &removed_ids {
            let _ = delete_account_file(id);
        }
        logger::log_warn(&format!(
            "[CodeBuddy Account] 检测到重复账号并已合并: removed_ids={}",
            removed_ids.join(",")
        ));
    }

    index.accounts = normalized.iter().map(|a| a.summary()).collect();
    normalized
}

pub fn list_accounts() -> Vec<CodebuddyAccount> {
    let mut index = load_account_index();
    let accounts = normalize_account_index(&mut index);
    if let Err(err) = save_account_index(&index) {
        logger::log_warn(&format!("[CodeBuddy Account] 保存账号索引失败: {}", err));
    }
    accounts
}

fn apply_payload(account: &mut CodebuddyAccount, payload: CodebuddyOAuthCompletePayload) {
    let incoming_email = payload.email.trim().to_string();
    if !incoming_email.is_empty() {
        account.email = incoming_email;
    }
    account.uid = payload.uid;
    account.nickname = payload.nickname;
    account.enterprise_id = payload.enterprise_id;
    account.enterprise_name = payload.enterprise_name;
    account.access_token = payload.access_token;
    account.refresh_token = payload.refresh_token;
    account.token_type = payload.token_type;
    account.expires_at = payload.expires_at;
    account.domain = payload.domain;
    if payload.plan_type.is_some() {
        account.plan_type = payload.plan_type;
    }
    if payload.dosage_notify_code.is_some() {
        account.dosage_notify_code = payload.dosage_notify_code;
    }
    if payload.dosage_notify_zh.is_some() {
        account.dosage_notify_zh = payload.dosage_notify_zh;
    }
    if payload.dosage_notify_en.is_some() {
        account.dosage_notify_en = payload.dosage_notify_en;
    }
    if payload.payment_type.is_some() {
        account.payment_type = payload.payment_type;
    }
    if payload.quota_raw.is_some() {
        account.quota_raw = payload.quota_raw;
    }
    account.auth_raw = payload.auth_raw;
    if payload.profile_raw.is_some() {
        account.profile_raw = payload.profile_raw;
    }
    if payload.usage_raw.is_some() {
        account.usage_raw = payload.usage_raw;
    }
    if payload.quota_binding.is_some() {
        account.quota_binding = payload.quota_binding;
    }
    account.status = payload.status;
    account.status_reason = payload.status_reason;
    account.last_used = now_ts();
}

pub fn upsert_account(payload: CodebuddyOAuthCompletePayload) -> Result<CodebuddyAccount, String> {
    let _lock = CODEBUDDY_ACCOUNT_INDEX_LOCK
        .lock()
        .map_err(|_| "获取 CodeBuddy 账号锁失败".to_string())?;
    let now = now_ts();
    let mut index = load_account_index();

    let incoming_uid = normalize_identity(payload.uid.as_deref());
    let incoming_email = normalize_email_identity(Some(payload.email.as_str()));

    let identity_seed = incoming_uid
        .clone()
        .or_else(|| incoming_email.clone())
        .unwrap_or_else(|| "codebuddy_user".to_string())
        .to_lowercase();
    let generated_id = format!("codebuddy_{:x}", md5::compute(identity_seed.as_bytes()));

    let account_id = index
        .accounts
        .iter()
        .filter_map(|item| load_account(&item.id))
        .find(|account| {
            let existing_uid = normalize_identity(account.uid.as_deref());
            let existing_email = normalize_email_identity(Some(account.email.as_str()));
            account_matches_payload_identity(
                existing_uid.as_ref(),
                existing_email.as_ref(),
                incoming_uid.as_ref(),
                incoming_email.as_ref(),
            )
        })
        .map(|a| a.id)
        .unwrap_or(generated_id);

    let existing = load_account(&account_id);
    let tags = existing.as_ref().and_then(|a| a.tags.clone());
    let created_at = existing.as_ref().map(|a| a.created_at).unwrap_or(now);

    let mut account = existing.unwrap_or(CodebuddyAccount {
        id: account_id.clone(),
        email: payload.email.clone(),
        uid: payload.uid.clone(),
        nickname: payload.nickname.clone(),
        enterprise_id: payload.enterprise_id.clone(),
        enterprise_name: payload.enterprise_name.clone(),
        tags,
        access_token: payload.access_token.clone(),
        refresh_token: payload.refresh_token.clone(),
        token_type: payload.token_type.clone(),
        expires_at: payload.expires_at,
        domain: payload.domain.clone(),
        plan_type: payload.plan_type.clone(),
        dosage_notify_code: payload.dosage_notify_code.clone(),
        dosage_notify_zh: payload.dosage_notify_zh.clone(),
        dosage_notify_en: payload.dosage_notify_en.clone(),
        payment_type: payload.payment_type.clone(),
        quota_raw: payload.quota_raw.clone(),
        auth_raw: payload.auth_raw.clone(),
        profile_raw: payload.profile_raw.clone(),
        usage_raw: payload.usage_raw.clone(),
        quota_binding: None,
        status: payload.status.clone(),
        status_reason: payload.status_reason.clone(),
        quota_query_last_error: None,
        quota_query_last_error_at: None,
        created_at,
        last_used: now,
    });

    apply_payload(&mut account, payload);
    account.id = account_id;
    account.created_at = created_at;
    account.last_used = now;

    save_account_file(&account)?;
    refresh_summary(&mut index, &account);
    save_account_index(&index)?;

    logger::log_info(&format!(
        "CodeBuddy 账号已保存: id={}, email={}",
        account.id, account.email
    ));
    Ok(account)
}

async fn refresh_account_token_with_mode(
    account_id: &str,
    require_user_resource: bool,
) -> Result<CodebuddyAccount, String> {
    let started_at = Instant::now();
    let mut account = load_account(account_id).ok_or_else(|| "账号不存在".to_string())?;
    logger::log_info(&format!(
        "[CodeBuddy Refresh] 开始刷新账号: id={}, email={}, strict_user_resource={}",
        account.id, account.email, require_user_resource
    ));

    let payload = if require_user_resource {
        codebuddy_oauth::refresh_payload_for_account_strict(&account).await?
    } else {
        codebuddy_oauth::refresh_payload_for_account(&account).await?
    };
    let tags = account.tags.clone();
    let created_at = account.created_at;
    apply_payload(&mut account, payload);
    account.tags = tags;
    account.created_at = created_at;
    account.last_used = now_ts();

    let updated = account.clone();
    upsert_account_record(account)?;
    logger::log_info(&format!(
        "[CodeBuddy Refresh] 刷新完成: id={}, email={}, elapsed={}ms",
        updated.id,
        updated.email,
        started_at.elapsed().as_millis()
    ));
    Ok(updated)
}

pub async fn refresh_account_token(account_id: &str) -> Result<CodebuddyAccount, String> {
    refresh_account_token_with_mode(account_id, false).await
}

pub async fn refresh_account_token_strict(account_id: &str) -> Result<CodebuddyAccount, String> {
    refresh_account_token_with_mode(account_id, true).await
}

pub async fn refresh_all_tokens() -> Result<Vec<(String, Result<CodebuddyAccount, String>)>, String>
{
    use futures::future::join_all;
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    const MAX_CONCURRENT: usize = 5;
    let accounts = list_accounts();
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let tasks: Vec<_> = accounts
        .into_iter()
        .map(|account| {
            let id = account.id;
            let semaphore = semaphore.clone();
            async move {
                let _permit = semaphore
                    .acquire_owned()
                    .await
                    .map_err(|e| format!("获取并发许可失败: {}", e))?;
                let result = refresh_account_token(&id).await;
                Ok::<(String, Result<CodebuddyAccount, String>), String>((id, result))
            }
        })
        .collect();

    let mut results = Vec::with_capacity(tasks.len());
    for task in join_all(tasks).await {
        match task {
            Ok(item) => results.push(item),
            Err(err) => return Err(err),
        }
    }
    Ok(results)
}

pub async fn query_quota_with_binding(
    account_id: &str,
    cookie_header: &str,
    product_code: Option<String>,
    status: Option<Vec<i32>>,
    package_end_time_range_begin: Option<String>,
    package_end_time_range_end: Option<String>,
    page_number: Option<i32>,
    page_size: Option<i32>,
    request_headers: Option<CodebuddyQuotaRequestHeaders>,
) -> Result<CodebuddyAccount, String> {
    let mut account = load_account(account_id).ok_or_else(|| "账号不存在".to_string())?;
    let is_manual_curl_replay = request_headers.is_none();
    let inherited_request_headers = account
        .quota_binding
        .as_ref()
        .and_then(|binding| binding.request_headers.clone());
    let mut replay_request_headers = request_headers.or(inherited_request_headers);
    let mut replay_user_agent = replay_request_headers
        .as_ref()
        .and_then(|headers| headers.user_agent.clone())
        .or_else(|| {
            account
                .quota_binding
                .as_ref()
                .and_then(|binding| normalize_non_empty(binding.user_agent.as_deref()))
        });
    let normalized_cookie_header: String;
    let mut resolved_product_code =
        normalize_non_empty(product_code.as_deref()).unwrap_or("p_tcaca".to_string());
    let mut resolved_status = normalize_status_list(status);

    let (default_begin, default_end) = build_default_time_range();
    let mut resolved_package_end_time_range_begin =
        normalize_non_empty(package_end_time_range_begin.as_deref()).unwrap_or(default_begin);
    let mut resolved_package_end_time_range_end =
        normalize_non_empty(package_end_time_range_end.as_deref()).unwrap_or(default_end);
    let mut resolved_page_number = page_number.unwrap_or(1).clamp(1, 10_000);
    let mut resolved_page_size = page_size.unwrap_or(100).clamp(1, 200);

    let user_resource = if is_manual_curl_replay {
        let parsed_curl = match parse_codebuddy_quota_curl_request(cookie_header) {
            Ok(parsed) => parsed,
            Err(err) => {
                mark_quota_query_failure(&mut account, &err);
                account.last_used = now_ts();
                let _ = upsert_account_record(account.clone());
                return Err(err);
            }
        };

        normalized_cookie_header = parsed_curl.normalized_cookie_header.clone();
        replay_request_headers = parsed_curl.request_headers_snapshot.clone();
        replay_user_agent = parsed_curl
            .user_agent
            .clone()
            .or_else(|| replay_user_agent.clone());

        if let Some(v) = parsed_curl.product_code {
            resolved_product_code = v;
        }
        resolved_status = normalize_status_list(parsed_curl.status.or(Some(resolved_status)));
        if let Some(v) = parsed_curl.package_end_time_range_begin {
            resolved_package_end_time_range_begin = v;
        }
        if let Some(v) = parsed_curl.package_end_time_range_end {
            resolved_package_end_time_range_end = v;
        }
        if let Some(v) = parsed_curl.page_number {
            resolved_page_number = v.clamp(1, 10_000);
        }
        if let Some(v) = parsed_curl.page_size {
            resolved_page_size = v.clamp(1, 200);
        }

        match codebuddy_oauth::fetch_user_resource_with_raw_request(
            &parsed_curl.request_url,
            &parsed_curl.request_method,
            &parsed_curl.request_headers_for_replay,
            parsed_curl.request_body.as_deref(),
        )
        .await
        {
            Ok(value) => value,
            Err(err) => {
                mark_quota_query_failure(&mut account, &err);
                account.last_used = now_ts();
                let _ = upsert_account_record(account.clone());
                return Err(err);
            }
        }
    } else {
        normalized_cookie_header = match normalize_session_cookie_header(cookie_header) {
            Ok(value) => value,
            Err(err) => {
                mark_quota_query_failure(&mut account, &err);
                account.last_used = now_ts();
                let _ = upsert_account_record(account.clone());
                return Err(err);
            }
        };

        match codebuddy_oauth::fetch_user_resource_with_cookie(
            &normalized_cookie_header,
            &resolved_product_code,
            &resolved_status,
            &resolved_package_end_time_range_begin,
            &resolved_package_end_time_range_end,
            resolved_page_number,
            resolved_page_size,
            replay_request_headers.as_ref(),
            replay_user_agent.as_deref(),
        )
        .await
        {
            Ok(value) => value,
            Err(err) => {
                mark_quota_query_failure(&mut account, &err);
                account.last_used = now_ts();
                let _ = upsert_account_record(account.clone());
                return Err(err);
            }
        }
    };

    if let Some(expected_uin) = extract_profile_uin(account.profile_raw.as_ref()) {
        let resource_uins = collect_user_resource_uins(&user_resource);
        let payer_uins = collect_user_resource_payer_uins(&user_resource);
        let has_identity_data = !resource_uins.is_empty() || !payer_uins.is_empty();
        let matches_uin =
            resource_uins.contains(&expected_uin) || payer_uins.contains(&expected_uin);

        if has_identity_data && !matches_uin {
            let err = format!(
                "{}|expected_uin={}|resource_uin={}|payer_uin={}",
                CODEBUDDY_QUOTA_BINDING_ACCOUNT_MISMATCH_CODE,
                expected_uin,
                join_uin_set(&resource_uins),
                join_uin_set(&payer_uins)
            );
            mark_quota_query_failure(&mut account, &err);
            account.last_used = now_ts();
            let _ = upsert_account_record(account.clone());
            return Err(err);
        }
    }

    let dosage = match codebuddy_oauth::fetch_dosage_notify(
        &account.access_token,
        account.uid.as_deref(),
        account.enterprise_id.as_deref(),
        account.domain.as_deref(),
    )
    .await
    {
        Ok(payload) => Some(payload),
        Err(_) => None,
    };

    let payment = match codebuddy_oauth::fetch_payment_type(
        &account.access_token,
        account.uid.as_deref(),
        account.enterprise_id.as_deref(),
        account.domain.as_deref(),
    )
    .await
    {
        Ok(payload) => Some(payload),
        Err(_) => None,
    };

    let mut quota_raw_obj = account
        .quota_raw
        .as_ref()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    quota_raw_obj.insert("userResource".to_string(), user_resource.clone());
    if let Some(payload) = &dosage {
        quota_raw_obj.insert("dosage".to_string(), payload.clone());
    }
    if let Some(payload) = &payment {
        quota_raw_obj.insert("payment".to_string(), payload.clone());
    }

    let dosage_data = dosage.as_ref().and_then(|v| v.get("data"));
    if let Some(code) = dosage_data
        .and_then(|d| d.get("dosageNotifyCode"))
        .map(|v| match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => v.to_string(),
        })
    {
        account.dosage_notify_code = Some(code);
    }
    if let Some(msg_zh) = dosage_data
        .and_then(|d| d.get("dosageNotifyZh"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        account.dosage_notify_zh = Some(msg_zh.to_string());
    }
    if let Some(msg_en) = dosage_data
        .and_then(|d| d.get("dosageNotifyEn"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        account.dosage_notify_en = Some(msg_en.to_string());
    }

    let payment_data = payment.as_ref().and_then(|v| v.get("data"));
    if let Some(payment_type) = payment_data.and_then(|d| {
        d.as_str().map(|s| s.to_string()).or_else(|| {
            d.get("paymentType")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
    }) {
        account.payment_type = Some(payment_type);
    }

    account.quota_raw = Some(Value::Object(quota_raw_obj));
    account.usage_raw = Some(user_resource);
    account.quota_binding = Some(CodebuddyQuotaBinding {
        cookie_header: normalized_cookie_header,
        product_code: resolved_product_code,
        status: resolved_status,
        package_end_time_range_begin: resolved_package_end_time_range_begin,
        package_end_time_range_end: resolved_package_end_time_range_end,
        page_number: resolved_page_number,
        page_size: resolved_page_size,
        updated_at: chrono::Utc::now().timestamp_millis(),
        user_agent: replay_user_agent,
        request_headers: replay_request_headers,
        source: Some(if is_manual_curl_replay {
            "manual_curl".to_string()
        } else {
            "manual".to_string()
        }),
    });
    account.quota_query_last_error = None;
    account.quota_query_last_error_at = None;
    account.last_used = now_ts();

    let updated = account.clone();
    upsert_account_record(account)?;
    Ok(updated)
}

pub fn clear_quota_binding(account_id: &str) -> Result<CodebuddyAccount, String> {
    let mut account = load_account(account_id).ok_or_else(|| "账号不存在".to_string())?;
    account.quota_binding = None;
    account.quota_query_last_error = None;
    account.quota_query_last_error_at = None;
    let updated = account.clone();
    upsert_account_record(account)?;
    Ok(updated)
}

fn apply_quota_api_responses(
    account: &mut CodebuddyAccount,
    user_resource: Value,
    dosage: Option<Value>,
    payment: Option<Value>,
) {
    let mut quota_raw_obj = account
        .quota_raw
        .as_ref()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    quota_raw_obj.insert("userResource".to_string(), user_resource.clone());
    if let Some(ref payload) = dosage {
        quota_raw_obj.insert("dosage".to_string(), payload.clone());
    }
    if let Some(ref payload) = payment {
        quota_raw_obj.insert("payment".to_string(), payload.clone());
    }

    let dosage_data = dosage.as_ref().and_then(|v| v.get("data"));
    if let Some(code) = dosage_data
        .and_then(|d| d.get("dosageNotifyCode"))
        .map(|v| match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => v.to_string(),
        })
    {
        account.dosage_notify_code = Some(code);
    }
    if let Some(msg_zh) = dosage_data
        .and_then(|d| d.get("dosageNotifyZh"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        account.dosage_notify_zh = Some(msg_zh.to_string());
    }
    if let Some(msg_en) = dosage_data
        .and_then(|d| d.get("dosageNotifyEn"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        account.dosage_notify_en = Some(msg_en.to_string());
    }

    let payment_data = payment.as_ref().and_then(|v| v.get("data"));
    if let Some(payment_type) = payment_data.and_then(|d| {
        d.as_str().map(|s| s.to_string()).or_else(|| {
            d.get("paymentType")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
    }) {
        account.payment_type = Some(payment_type);
    }

    account.quota_raw = Some(Value::Object(quota_raw_obj));
    account.usage_raw = Some(user_resource);
}

fn read_binding_request_header(binding: &Value, header_name: &str) -> Option<String> {
    binding
        .get("requestHeaders")
        .and_then(|h| h.as_object())
        .and_then(|headers| {
            headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(header_name))
                .and_then(|(_, value)| value.as_str())
        })
        .and_then(|value| normalize_non_empty(Some(value)))
}

fn build_quota_request_headers(binding: &Value) -> Option<CodebuddyQuotaRequestHeaders> {
    let headers = CodebuddyQuotaRequestHeaders {
        accept: read_binding_request_header(binding, "Accept"),
        accept_language: read_binding_request_header(binding, "Accept-Language"),
        content_type: read_binding_request_header(binding, "Content-Type"),
        origin: read_binding_request_header(binding, "Origin"),
        referer: read_binding_request_header(binding, "Referer").or_else(|| {
            binding
                .get("requestReferrer")
                .and_then(|v| v.as_str())
                .and_then(|v| normalize_non_empty(Some(v)))
        }),
        user_agent: read_binding_request_header(binding, "User-Agent"),
        sec_fetch_site: read_binding_request_header(binding, "Sec-Fetch-Site"),
        sec_fetch_mode: read_binding_request_header(binding, "Sec-Fetch-Mode"),
        sec_fetch_dest: read_binding_request_header(binding, "Sec-Fetch-Dest"),
    };

    if headers.is_empty() {
        None
    } else {
        Some(headers)
    }
}

fn build_quota_binding_from_webview_payload(data: &Value) -> Option<CodebuddyQuotaBinding> {
    let binding = data.get("binding")?;
    let raw_cookie = binding
        .get("cookieHeader")
        .or_else(|| binding.get("cookie_header"))
        .and_then(|v| v.as_str())?;
    let cookie_header = normalize_session_cookie_header(raw_cookie).ok()?;

    let product_code = normalize_non_empty(
        binding
            .get("productCode")
            .or_else(|| binding.get("product_code"))
            .and_then(|v| v.as_str()),
    )
    .unwrap_or_else(|| "p_tcaca".to_string());

    let status = binding.get("status").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|item| item.as_i64())
            .filter_map(|n| i32::try_from(n).ok())
            .collect::<Vec<i32>>()
    });
    let status = normalize_status_list(status);

    let (default_begin, default_end) = build_default_time_range();
    let package_end_time_range_begin = normalize_non_empty(
        binding
            .get("packageEndTimeRangeBegin")
            .or_else(|| binding.get("package_end_time_range_begin"))
            .and_then(|v| v.as_str()),
    )
    .unwrap_or(default_begin);
    let package_end_time_range_end = normalize_non_empty(
        binding
            .get("packageEndTimeRangeEnd")
            .or_else(|| binding.get("package_end_time_range_end"))
            .and_then(|v| v.as_str()),
    )
    .unwrap_or(default_end);

    let page_number = binding
        .get("pageNumber")
        .or_else(|| binding.get("page_number"))
        .and_then(|v| v.as_i64())
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(1)
        .clamp(1, 10_000);
    let page_size = binding
        .get("pageSize")
        .or_else(|| binding.get("page_size"))
        .and_then(|v| v.as_i64())
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(100)
        .clamp(1, 200);

    let request_headers = build_quota_request_headers(binding);
    let user_agent = request_headers
        .as_ref()
        .and_then(|headers| headers.user_agent.clone());

    Some(CodebuddyQuotaBinding {
        cookie_header,
        product_code,
        status,
        package_end_time_range_begin,
        package_end_time_range_end,
        page_number,
        page_size,
        updated_at: chrono::Utc::now().timestamp_millis(),
        user_agent,
        request_headers,
        source: Some("webview".to_string()),
    })
}

pub async fn apply_webview_quota_result(
    account_id: &str,
    raw_json: &str,
) -> Result<CodebuddyAccount, String> {
    let data: Value =
        serde_json::from_str(raw_json).map_err(|e| format!("JSON parse error: {}", e))?;
    let webview_binding = build_quota_binding_from_webview_payload(&data);

    // 新流程：WebView 只采集 Cookie 与请求参数，真实配额请求由后端发起。
    if data.get("userResource").is_none() {
        let binding = webview_binding
            .ok_or_else(|| "WebView 结果缺少有效 binding（需要 session/session_2）".to_string())?;
        let mut updated = query_quota_with_binding(
            account_id,
            &binding.cookie_header,
            Some(binding.product_code.clone()),
            Some(binding.status.clone()),
            Some(binding.package_end_time_range_begin.clone()),
            Some(binding.package_end_time_range_end.clone()),
            Some(binding.page_number),
            Some(binding.page_size),
            binding.request_headers.clone(),
        )
        .await?;
        updated.quota_binding = Some(binding);
        updated.last_used = now_ts();
        let updated = upsert_account_record(updated)?;
        logger::log_info("[CodeBuddy] WebView 仅采集 Cookie，已由后端完成真实配额请求");
        return Ok(updated);
    }

    let user_resource = data
        .get("userResource")
        .cloned()
        .ok_or("Missing userResource in WebView result")?;
    let dosage = data.get("dosage").cloned().filter(|v| !v.is_null());
    let payment = data.get("payment").cloned().filter(|v| !v.is_null());

    let mut account = load_account(account_id).ok_or_else(|| "账号不存在".to_string())?;

    if let Some(expected_uin) = extract_profile_uin(account.profile_raw.as_ref()) {
        let resource_uins = collect_user_resource_uins(&user_resource);
        let payer_uins = collect_user_resource_payer_uins(&user_resource);
        let has_identity_data = !resource_uins.is_empty() || !payer_uins.is_empty();
        let matches_uin =
            resource_uins.contains(&expected_uin) || payer_uins.contains(&expected_uin);

        if has_identity_data && !matches_uin {
            return Err(format!(
                "WebView 登录账号与当前账号不一致（当前 UIN：{}，WebView UIN：{}）",
                expected_uin,
                join_uin_set(&resource_uins)
            ));
        }
    }

    apply_quota_api_responses(&mut account, user_resource, dosage, payment);
    if let Some(binding) = webview_binding {
        account.quota_binding = Some(binding);
        logger::log_info("[CodeBuddy] WebView 配额结果已自动绑定查询参数");
    }
    account.quota_query_last_error = None;
    account.quota_query_last_error_at = None;
    account.last_used = now_ts();

    let updated = account.clone();
    upsert_account_record(account)?;
    Ok(updated)
}

pub fn remove_account(account_id: &str) -> Result<(), String> {
    let _lock = CODEBUDDY_ACCOUNT_INDEX_LOCK
        .lock()
        .map_err(|_| "获取 CodeBuddy 账号锁失败".to_string())?;
    let mut index = load_account_index();
    index.accounts.retain(|item| item.id != account_id);
    save_account_index(&index)?;
    delete_account_file(account_id)?;
    Ok(())
}

pub fn remove_accounts(account_ids: &[String]) -> Result<(), String> {
    for id in account_ids {
        remove_account(id)?;
    }
    Ok(())
}

pub fn update_account_tags(
    account_id: &str,
    tags: Vec<String>,
) -> Result<CodebuddyAccount, String> {
    let mut account = load_account(account_id).ok_or_else(|| "账号不存在".to_string())?;
    account.tags = Some(tags);
    account.last_used = now_ts();
    let updated = account.clone();
    upsert_account_record(account)?;
    Ok(updated)
}

pub fn import_from_json(json_content: &str) -> Result<Vec<CodebuddyAccount>, String> {
    if let Ok(account) = serde_json::from_str::<CodebuddyAccount>(json_content) {
        let saved = upsert_account_record(account)?;
        return Ok(vec![saved]);
    }

    if let Ok(accounts) = serde_json::from_str::<Vec<CodebuddyAccount>>(json_content) {
        let mut result = Vec::new();
        for account in accounts {
            let saved = upsert_account_record(account)?;
            result.push(saved);
        }
        return Ok(result);
    }

    if let Ok(value) = serde_json::from_str::<Value>(json_content) {
        return import_from_json_value(value);
    }

    Err("无法解析 CodeBuddy JSON 导入内容".to_string())
}

fn import_from_json_value(value: Value) -> Result<Vec<CodebuddyAccount>, String> {
    match value {
        Value::Array(items) => {
            if items.is_empty() {
                return Err("导入数组为空".to_string());
            }
            let mut results = Vec::new();
            for (idx, item) in items.into_iter().enumerate() {
                let payload = payload_from_import_value(item)
                    .map_err(|e| format!("第 {} 条记录解析失败: {}", idx + 1, e))?;
                let account = upsert_account_record_from_payload(payload)?;
                results.push(account);
            }
            Ok(results)
        }
        Value::Object(mut obj) => {
            let object_value = Value::Object(obj.clone());
            if let Ok(payload) = payload_from_import_value(object_value) {
                let account = upsert_account_record_from_payload(payload)?;
                return Ok(vec![account]);
            }

            if let Some(accounts) = obj
                .remove("accounts")
                .or_else(|| obj.remove("items"))
                .and_then(|raw| raw.as_array().cloned())
            {
                if accounts.is_empty() {
                    return Err("导入数组为空".to_string());
                }
                let mut results = Vec::new();
                for (idx, item) in accounts.into_iter().enumerate() {
                    let payload = payload_from_import_value(item)
                        .map_err(|e| format!("第 {} 条记录解析失败: {}", idx + 1, e))?;
                    let account = upsert_account_record_from_payload(payload)?;
                    results.push(account);
                }
                return Ok(results);
            }

            Err("无法解析 CodeBuddy 导入对象".to_string())
        }
        _ => Err("CodeBuddy 导入 JSON 必须是对象或数组".to_string()),
    }
}

fn upsert_account_record_from_payload(
    payload: CodebuddyOAuthCompletePayload,
) -> Result<CodebuddyAccount, String> {
    // Release lock pattern: upsert_account already takes lock internally
    drop(
        CODEBUDDY_ACCOUNT_INDEX_LOCK
            .lock()
            .map_err(|_| "获取锁失败".to_string())?,
    );
    let now = now_ts();
    let incoming_uid = normalize_identity(payload.uid.as_deref());
    let incoming_email = normalize_email_identity(Some(payload.email.as_str()));
    let identity_seed = incoming_uid
        .or_else(|| incoming_email)
        .unwrap_or_else(|| "codebuddy_user".to_string());
    let generated_id = format!("codebuddy_{:x}", md5::compute(identity_seed.as_bytes()));

    let account = CodebuddyAccount {
        id: generated_id,
        email: payload.email,
        uid: payload.uid,
        nickname: payload.nickname,
        enterprise_id: payload.enterprise_id,
        enterprise_name: payload.enterprise_name,
        tags: None,
        access_token: payload.access_token,
        refresh_token: payload.refresh_token,
        token_type: payload.token_type,
        expires_at: payload.expires_at,
        domain: payload.domain,
        plan_type: payload.plan_type,
        dosage_notify_code: payload.dosage_notify_code,
        dosage_notify_zh: payload.dosage_notify_zh,
        dosage_notify_en: payload.dosage_notify_en,
        payment_type: payload.payment_type,
        quota_raw: payload.quota_raw,
        auth_raw: payload.auth_raw,
        profile_raw: payload.profile_raw,
        usage_raw: payload.usage_raw,
        quota_binding: None,
        status: payload.status,
        status_reason: payload.status_reason,
        quota_query_last_error: None,
        quota_query_last_error_at: None,
        created_at: now,
        last_used: now,
    };
    upsert_account_record(account)
}

fn payload_from_import_value(raw: Value) -> Result<CodebuddyOAuthCompletePayload, String> {
    let obj = raw
        .as_object()
        .ok_or_else(|| "导入条目必须是对象".to_string())?;

    let access_token = obj
        .get("access_token")
        .or_else(|| obj.get("accessToken"))
        .or_else(|| obj.get("token"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if access_token.is_empty() {
        return Err("缺少 access_token".to_string());
    }

    let email = obj
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let uid = obj
        .get("uid")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let nickname = obj
        .get("nickname")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let enterprise_id = obj
        .get("enterprise_id")
        .or_else(|| obj.get("enterpriseId"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let enterprise_name = obj
        .get("enterprise_name")
        .or_else(|| obj.get("enterpriseName"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let refresh_token = obj
        .get("refresh_token")
        .or_else(|| obj.get("refreshToken"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let domain = obj
        .get("domain")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(CodebuddyOAuthCompletePayload {
        email,
        uid,
        nickname,
        enterprise_id,
        enterprise_name,
        access_token,
        refresh_token,
        token_type: Some("Bearer".to_string()),
        expires_at: None,
        domain,
        plan_type: None,
        dosage_notify_code: None,
        dosage_notify_zh: None,
        dosage_notify_en: None,
        payment_type: None,
        quota_raw: None,
        auth_raw: obj.get("auth_raw").cloned(),
        profile_raw: obj.get("profile_raw").cloned(),
        usage_raw: obj.get("usage_raw").cloned(),
        quota_binding: None,
        status: Some("normal".to_string()),
        status_reason: None,
    })
}

pub fn export_accounts(account_ids: &[String]) -> Result<String, String> {
    let accounts: Vec<CodebuddyAccount> = account_ids
        .iter()
        .filter_map(|id| load_account(id))
        .collect();
    serde_json::to_string_pretty(&accounts).map_err(|e| format!("导出失败: {}", e))
}

pub fn get_default_codebuddy_data_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;

    #[cfg(target_os = "macos")]
    {
        Some(home.join("Library/Application Support/CodeBuddy"))
    }

    #[cfg(target_os = "windows")]
    {
        dirs::data_dir().map(|d| d.join("CodeBuddy"))
    }

    #[cfg(target_os = "linux")]
    {
        dirs::config_dir().map(|d| d.join("CodeBuddy"))
    }
}

pub fn get_default_codebuddy_state_db_path() -> Option<PathBuf> {
    get_default_codebuddy_data_dir()
        .map(|d| d.join("User").join("globalStorage").join("state.vscdb"))
}

fn parse_local_access_token(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Array(arr) => arr.iter().find_map(parse_local_access_token),
        Value::Object(obj) => {
            let direct = obj
                .get("token")
                .or_else(|| obj.get("access_token"))
                .or_else(|| obj.get("accessToken"))
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if let Some(token) = direct {
                return Some(token);
            }

            let auth_token = obj
                .get("auth")
                .and_then(|v| v.as_object())
                .and_then(|auth| {
                    auth.get("accessToken")
                        .or_else(|| auth.get("access_token"))
                        .and_then(|v| v.as_str())
                })
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if let Some(token) = auth_token {
                return Some(token);
            }

            let encoded = obj
                .get("session")
                .or_else(|| obj.get("data"))
                .and_then(parse_local_access_token);
            if encoded.is_some() {
                return encoded;
            }

            None
        }
        _ => None,
    }
}

fn normalize_local_codebuddy_token(token: &str) -> Option<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((_, suffix)) = trimmed.split_once('+') {
        let suffix = suffix.trim();
        if !suffix.is_empty() {
            return Some(suffix.to_string());
        }
    }
    Some(trimmed.to_string())
}

fn extract_local_codebuddy_token_parts(token: &str) -> Option<(Option<String>, String)> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((prefix, suffix)) = trimmed.split_once('+') {
        let uid = prefix.trim();
        let token_value = suffix.trim();
        if token_value.is_empty() {
            return None;
        }
        let uid_opt = if uid.is_empty() {
            None
        } else {
            Some(uid.to_string())
        };
        return Some((uid_opt, token_value.to_string()));
    }
    Some((None, trimmed.to_string()))
}

fn json_object_string_field(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        let value = obj
            .get(*key)
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        if let Some(found) = value {
            return Some(found.to_string());
        }
    }
    None
}

fn json_object_i64_field(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<i64> {
    for key in keys {
        let Some(raw) = obj.get(*key) else {
            continue;
        };
        if let Some(v) = raw.as_i64() {
            return Some(v);
        }
        if let Some(v) = raw.as_u64() {
            if let Ok(parsed) = i64::try_from(v) {
                return Some(parsed);
            }
        }
        if let Some(v) = raw.as_str() {
            if let Ok(parsed) = v.trim().parse::<i64>() {
                return Some(parsed);
            }
        }
    }
    None
}

fn build_local_import_payload(
    access_token: String,
    parsed_json: Option<Value>,
    uid_from_token: Option<String>,
) -> CodebuddyOAuthCompletePayload {
    let root_obj = parsed_json.as_ref().and_then(|v| v.as_object());
    let account_obj = root_obj.and_then(|obj| obj.get("account").and_then(|v| v.as_object()));
    let auth_obj = root_obj.and_then(|obj| obj.get("auth").and_then(|v| v.as_object()));

    let uid = root_obj
        .and_then(|obj| json_object_string_field(obj, &["uid"]))
        .or_else(|| account_obj.and_then(|obj| json_object_string_field(obj, &["uid", "id"])))
        .or(uid_from_token);

    let nickname = root_obj
        .and_then(|obj| json_object_string_field(obj, &["nickname", "name"]))
        .or_else(|| {
            account_obj.and_then(|obj| json_object_string_field(obj, &["nickname", "label"]))
        });

    let email = root_obj
        .and_then(|obj| json_object_string_field(obj, &["email"]))
        .or_else(|| account_obj.and_then(|obj| json_object_string_field(obj, &["email"])))
        .or_else(|| auth_obj.and_then(|obj| json_object_string_field(obj, &["email"])))
        .or_else(|| nickname.clone())
        .or_else(|| uid.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let enterprise_id = root_obj
        .and_then(|obj| json_object_string_field(obj, &["enterpriseId", "enterprise_id"]))
        .or_else(|| {
            account_obj
                .and_then(|obj| json_object_string_field(obj, &["enterpriseId", "enterprise_id"]))
        });
    let enterprise_name = root_obj
        .and_then(|obj| json_object_string_field(obj, &["enterpriseName", "enterprise_name"]))
        .or_else(|| {
            account_obj.and_then(|obj| {
                json_object_string_field(obj, &["enterpriseName", "enterprise_name"])
            })
        });

    let refresh_token = root_obj
        .and_then(|obj| json_object_string_field(obj, &["refreshToken", "refresh_token"]))
        .or_else(|| {
            auth_obj
                .and_then(|obj| json_object_string_field(obj, &["refreshToken", "refresh_token"]))
        });
    let token_type = root_obj
        .and_then(|obj| json_object_string_field(obj, &["tokenType", "token_type"]))
        .or_else(|| {
            auth_obj.and_then(|obj| json_object_string_field(obj, &["tokenType", "token_type"]))
        })
        .or_else(|| Some("Bearer".to_string()));
    let domain = root_obj
        .and_then(|obj| json_object_string_field(obj, &["domain"]))
        .or_else(|| auth_obj.and_then(|obj| json_object_string_field(obj, &["domain"])));
    let expires_at = root_obj
        .and_then(|obj| json_object_i64_field(obj, &["expiresAt", "expires_at"]))
        .or_else(|| {
            auth_obj.and_then(|obj| json_object_i64_field(obj, &["expiresAt", "expires_at"]))
        });

    CodebuddyOAuthCompletePayload {
        email,
        uid,
        nickname,
        enterprise_id,
        enterprise_name,
        access_token,
        refresh_token,
        token_type,
        expires_at,
        domain,
        plan_type: None,
        dosage_notify_code: None,
        dosage_notify_zh: None,
        dosage_notify_en: None,
        payment_type: None,
        quota_raw: None,
        auth_raw: parsed_json.clone(),
        profile_raw: account_obj.map(|obj| Value::Object(obj.clone())),
        usage_raw: None,
        quota_binding: None,
        status: Some("normal".to_string()),
        status_reason: None,
    }
}

pub fn import_payload_from_local() -> Result<Option<CodebuddyOAuthCompletePayload>, String> {
    let data_root = match get_default_codebuddy_data_dir() {
        Some(path) => path,
        None => return Ok(None),
    };

    let state_db = match get_default_codebuddy_state_db_path() {
        Some(path) => path,
        None => return Ok(None),
    };
    if !state_db.exists() {
        return Ok(None);
    }

    let raw_secret = crate::modules::vscode_inject::read_codebuddy_secret_storage_value(
        CODEBUDDY_SECRET_EXTENSION_ID,
        CODEBUDDY_SECRET_KEY,
        Some(data_root.to_string_lossy().as_ref()),
    )?;

    let Some(secret) = raw_secret else {
        return Ok(None);
    };

    let parsed_json = serde_json::from_str::<Value>(&secret).ok();
    let token_candidate = parsed_json
        .as_ref()
        .and_then(parse_local_access_token)
        .or_else(|| {
            let raw = secret.trim();
            if raw.is_empty() {
                None
            } else {
                Some(raw.to_string())
            }
        });

    let Some(raw_token) = token_candidate else {
        return Err("本地 CodeBuddy 登录信息解析失败：未找到 access token".to_string());
    };

    let Some((uid_from_token, normalized_token)) = extract_local_codebuddy_token_parts(&raw_token)
    else {
        return Err("本地 CodeBuddy 登录信息解析失败：access token 无效".to_string());
    };
    let Some(access_token) = normalize_local_codebuddy_token(&normalized_token) else {
        return Err("本地 CodeBuddy 登录信息解析失败：access token 为空".to_string());
    };

    let payload = build_local_import_payload(access_token, parsed_json, uid_from_token);
    Ok(Some(payload))
}

pub fn import_from_local() -> Result<Option<CodebuddyAccount>, String> {
    let payload = match import_payload_from_local()? {
        Some(payload) => payload,
        None => return Ok(None),
    };
    let account = upsert_account(payload)?;
    Ok(Some(account))
}

pub fn run_quota_alert_if_needed() -> Result<(), String> {
    let config = crate::modules::config::get_user_config();
    if !config.codebuddy_quota_alert_enabled {
        return Ok(());
    }
    let threshold = config.codebuddy_quota_alert_threshold;
    if threshold <= 0 {
        return Ok(());
    }

    let accounts = list_accounts();
    let now = now_ts();
    let mut last_sent = CODEBUDDY_QUOTA_ALERT_LAST_SENT
        .lock()
        .map_err(|_| "获取预警锁失败".to_string())?;

    for account in &accounts {
        let cooldown_key = account.id.clone();
        if let Some(last) = last_sent.get(&cooldown_key) {
            if now - last < CODEBUDDY_QUOTA_ALERT_COOLDOWN_SECONDS {
                continue;
            }
        }

        let should_alert = match account.dosage_notify_code.as_deref() {
            Some(code) if code != "USAGE_NORMAL" && !code.is_empty() => true,
            _ => false,
        };

        if should_alert {
            last_sent.insert(cooldown_key, now);
            if let Some(app) = crate::get_app_handle() {
                let msg = account
                    .dosage_notify_zh
                    .as_deref()
                    .or(account.dosage_notify_en.as_deref())
                    .unwrap_or("配额即将耗尽");

                let _ = app.emit(
                    "quota:alert",
                    serde_json::json!({
                        "platform": "codebuddy",
                        "accountId": account.id,
                        "email": account.email,
                        "message": msg,
                    }),
                );
            }
        }
    }

    Ok(())
}
