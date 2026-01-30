use crate::models::codex::{CodexQuota, CodexAccount};
use crate::modules::{codex_account, logger};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, ACCEPT};
use serde::{Deserialize, Serialize};

// 使用 wham/usage 端点（Quotio 使用的）
const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

/// 使用率窗口（5小时/周）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WindowInfo {
    #[serde(rename = "used_percent")]
    used_percent: Option<i32>,
    #[serde(rename = "limit_window_seconds")]
    limit_window_seconds: Option<i64>,
    #[serde(rename = "reset_after_seconds")]
    reset_after_seconds: Option<i64>,
    #[serde(rename = "reset_at")]
    reset_at: Option<i64>,
}

/// 速率限制信息
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RateLimitInfo {
    allowed: Option<bool>,
    #[serde(rename = "limit_reached")]
    limit_reached: Option<bool>,
    #[serde(rename = "primary_window")]
    primary_window: Option<WindowInfo>,
    #[serde(rename = "secondary_window")]
    secondary_window: Option<WindowInfo>,
}

/// 使用率响应
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UsageResponse {
    #[serde(rename = "plan_type")]
    plan_type: Option<String>,
    #[serde(rename = "rate_limit")]
    rate_limit: Option<RateLimitInfo>,
    #[serde(rename = "code_review_rate_limit")]
    code_review_rate_limit: Option<RateLimitInfo>,
}

/// 查询单个账号的配额
pub async fn fetch_quota(account: &CodexAccount) -> Result<CodexQuota, String> {
    let client = reqwest::Client::new();
    
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", account.tokens.access_token))
            .map_err(|e| format!("构建 Authorization 头失败: {}", e))?,
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    
    // 添加 ChatGPT-Account-Id 头（关键！）
    let account_id = account
        .account_id
        .clone()
        .or_else(|| codex_account::extract_chatgpt_account_id_from_access_token(&account.tokens.access_token));
    
    if let Some(ref acc_id) = account_id {
        if !acc_id.is_empty() {
            headers.insert(
                "ChatGPT-Account-Id",
                HeaderValue::from_str(acc_id)
                    .map_err(|e| format!("构建 Account-Id 头失败: {}", e))?,
            );
        }
    }
    
    logger::log_info(&format!("Codex 配额请求: {} (account_id: {:?})", USAGE_URL, account_id));
    
    let response = client
        .get(USAGE_URL)
        .headers(headers)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;
    
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        // 截断响应，避免日志太长
        let body_preview = if body.len() > 200 { &body[..200] } else { &body };
        return Err(format!("API 返回错误 {} - {}", status, body_preview));
    }
    
    let body = response.text().await
        .map_err(|e| format!("读取响应失败: {}", e))?;
    
    logger::log_info(&format!("Codex 配额响应: {}", &body[..body.len().min(500)]));
    
    // 解析响应
    let usage: UsageResponse = serde_json::from_str(&body)
        .map_err(|e| format!("解析 JSON 失败: {}", e))?;
    
    parse_quota_from_usage(&usage, &body)
}

/// 从使用率响应中解析配额信息
fn parse_quota_from_usage(usage: &UsageResponse, raw_body: &str) -> Result<CodexQuota, String> {
    let rate_limit = usage.rate_limit.as_ref();
    
    // Primary window = 5小时配额（session）
    let (hourly_percentage, hourly_reset_time) = if let Some(primary) = rate_limit.and_then(|r| r.primary_window.as_ref()) {
        let used = primary.used_percent.unwrap_or(0);
        let remaining = 100 - used;
        let reset_at = primary.reset_at;
        (remaining, reset_at)
    } else {
        (100, None)
    };
    
    // Secondary window = 周配额
    let (weekly_percentage, weekly_reset_time) = if let Some(secondary) = rate_limit.and_then(|r| r.secondary_window.as_ref()) {
        let used = secondary.used_percent.unwrap_or(0);
        let remaining = 100 - used;
        let reset_at = secondary.reset_at;
        (remaining, reset_at)
    } else {
        (100, None)
    };
    
    // 保存原始响应
    let raw_data: Option<serde_json::Value> = serde_json::from_str(raw_body).ok();
    
    Ok(CodexQuota {
        hourly_percentage,
        hourly_reset_time,
        weekly_percentage,
        weekly_reset_time,
        raw_data,
    })
}

/// 刷新账号配额并保存（包含 token 自动刷新）
pub async fn refresh_account_quota(account_id: &str) -> Result<CodexQuota, String> {
    let mut account = codex_account::load_account(account_id)
        .ok_or_else(|| format!("账号不存在: {}", account_id))?;
    
    // 检查 token 是否过期，如果过期则刷新
    if crate::modules::codex_oauth::is_token_expired(&account.tokens.access_token) {
        logger::log_info(&format!("账号 {} 的 Token 已过期，尝试刷新", account.email));
        
        if let Some(ref refresh_token) = account.tokens.refresh_token {
            match crate::modules::codex_oauth::refresh_access_token(refresh_token).await {
                Ok(new_tokens) => {
                    logger::log_info(&format!("账号 {} 的 Token 刷新成功", account.email));
                    account.tokens = new_tokens;
                    codex_account::save_account(&account)?;
                }
                Err(e) => {
                    logger::log_error(&format!("账号 {} Token 刷新失败: {}", account.email, e));
                    return Err(format!("Token 已过期且刷新失败: {}", e));
                }
            }
        } else {
            return Err("Token 已过期且无 refresh_token".to_string());
        }
    }
    
    let quota = fetch_quota(&account).await?;
    
    account.quota = Some(quota.clone());
    codex_account::save_account(&account)?;
    
    Ok(quota)
}

/// 刷新所有账号配额
pub async fn refresh_all_quotas() -> Result<Vec<(String, Result<CodexQuota, String>)>, String> {
    let accounts = codex_account::list_accounts();
    let mut results = Vec::new();
    
    for account in accounts {
        let result = refresh_account_quota(&account.id).await;
        results.push((account.id.clone(), result));
    }
    
    Ok(results)
}
