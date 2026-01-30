use crate::models::codex::CodexAccount;
use crate::modules::{codex_account, codex_oauth, logger};
use serde_json::json;
use std::fs;
use std::path::PathBuf;

/// 获取 OpenCode 的 auth.json 路径
///
/// - 根据系统数据目录自动定位（与 OpenCode 保持一致）
pub fn get_opencode_auth_json_path() -> Result<PathBuf, String> {
    let data_dir = dirs::data_dir().ok_or("无法获取系统数据目录")?;
    Ok(data_dir.join("opencode").join("auth.json"))
}

fn atomic_write(path: &PathBuf, content: &str) -> Result<(), String> {
    let parent = path.parent().ok_or("无法获取 auth.json 目录")?;
    fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;

    let tmp_path = parent.join(format!(
        ".auth.json.tmp.{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    fs::write(&tmp_path, content).map_err(|e| format!("写入临时文件失败: {}", e))?;

    if let Err(err) = fs::rename(&tmp_path, path) {
        if path.exists() {
            fs::remove_file(path).map_err(|e| format!("删除旧 auth.json 失败: {}", e))?;
            fs::rename(&tmp_path, path).map_err(|e| format!("替换 auth.json 失败: {}", e))?;
        } else {
            return Err(format!("替换 auth.json 失败: {}", err));
        }
    }
    Ok(())
}

fn build_openai_payload(account: &CodexAccount) -> Result<serde_json::Value, String> {
    let refresh = account
        .tokens
        .refresh_token
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Codex refresh_token 缺失，无法同步到 OpenCode".to_string())?;
    let expires = decode_token_exp_ms(&account.tokens.access_token)
        .ok_or_else(|| "Codex access_token 缺少 exp，无法同步到 OpenCode".to_string())?;

    let mut payload = json!({
        "type": "oauth",
        "access": account.tokens.access_token,
        "refresh": refresh,
        "expires": expires,
    });

    let fallback_account_id = extract_account_id_from_tokens(account);
    if let Some(account_id) = account.account_id.clone().or(fallback_account_id) {
        payload["accountId"] = json!(account_id);
    }

    Ok(payload)
}

fn decode_token_exp_ms(access_token: &str) -> Option<i64> {
    let payload = codex_account::decode_jwt_payload(access_token).ok()?;
    payload.exp.map(|exp| exp * 1000)
}

fn extract_account_id_from_tokens(account: &CodexAccount) -> Option<String> {
    codex_account::extract_chatgpt_account_id_from_access_token(&account.tokens.access_token)
}

/// 使用 Codex 账号的 token 替换 OpenCode auth.json 中的 openai 记录
pub fn replace_openai_entry_from_codex(account: &CodexAccount) -> Result<(), String> {
    // 确保 token 未过期
    if codex_oauth::is_token_expired(&account.tokens.access_token) {
        return Err("Codex access_token 已过期，无法同步到 OpenCode".to_string());
    }

    let auth_path = get_opencode_auth_json_path()?;
    let mut auth_json = if auth_path.exists() {
        let content = fs::read_to_string(&auth_path)
            .map_err(|e| format!("读取 OpenCode auth.json 失败: {}", e))?;
        serde_json::from_str::<serde_json::Value>(&content)
            .map_err(|e| format!("解析 OpenCode auth.json 失败: {}", e))?
    } else {
        json!({})
    };

    if !auth_json.is_object() {
        auth_json = json!({});
    }

    let openai_payload = build_openai_payload(account)?;
    if let Some(map) = auth_json.as_object_mut() {
        map.insert("openai".to_string(), openai_payload);
    }

    let content = serde_json::to_string_pretty(&auth_json)
        .map_err(|e| format!("序列化 OpenCode auth.json 失败: {}", e))?;
    atomic_write(&auth_path, &content)?;

    logger::log_info("已更新 OpenCode auth.json 中的 openai 记录");
    Ok(())
}
