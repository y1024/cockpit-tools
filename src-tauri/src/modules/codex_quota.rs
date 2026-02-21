use crate::models::codex::{CodexAccount, CodexQuota, CodexQuotaErrorInfo};
use crate::modules::{codex_account, logger};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};
use serde::{Deserialize, Serialize};

// 使用 wham/usage 端点（Quotio 使用的）
const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

fn get_header_value(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string()
}

fn extract_detail_code_from_body(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;

    if let Some(code) = value
        .get("detail")
        .and_then(|detail| detail.get("code"))
        .and_then(|code| code.as_str())
    {
        return Some(code.to_string());
    }

    if let Some(code) = value.get("code").and_then(|code| code.as_str()) {
        return Some(code.to_string());
    }

    None
}

fn extract_error_code_from_message(message: &str) -> Option<String> {
    let marker = "[error_code:";
    let start = message.find(marker)?;
    let code_start = start + marker.len();
    let end = message[code_start..].find(']')?;
    Some(message[code_start..code_start + end].to_string())
}

fn write_quota_error(account: &mut CodexAccount, message: String) {
    account.quota_error = Some(CodexQuotaErrorInfo {
        code: extract_error_code_from_message(&message),
        message,
        timestamp: chrono::Utc::now().timestamp(),
    });
}

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

fn normalize_remaining_percentage(window: &WindowInfo) -> i32 {
    let used = window.used_percent.unwrap_or(0).clamp(0, 100);
    100 - used
}

fn normalize_window_minutes(window: &WindowInfo) -> Option<i64> {
    let seconds = window.limit_window_seconds?;
    if seconds <= 0 {
        return None;
    }
    Some((seconds + 59) / 60)
}

fn normalize_reset_time(window: &WindowInfo) -> Option<i64> {
    if let Some(reset_at) = window.reset_at {
        return Some(reset_at);
    }

    let reset_after_seconds = window.reset_after_seconds?;
    if reset_after_seconds < 0 {
        return None;
    }

    Some(chrono::Utc::now().timestamp() + reset_after_seconds)
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
    let account_id = account.account_id.clone().or_else(|| {
        codex_account::extract_chatgpt_account_id_from_access_token(&account.tokens.access_token)
    });

    if let Some(ref acc_id) = account_id {
        if !acc_id.is_empty() {
            headers.insert(
                "ChatGPT-Account-Id",
                HeaderValue::from_str(acc_id)
                    .map_err(|e| format!("构建 Account-Id 头失败: {}", e))?,
            );
        }
    }

    logger::log_info(&format!(
        "Codex 配额请求: {} (account_id: {:?})",
        USAGE_URL, account_id
    ));

    let response = client
        .get(USAGE_URL)
        .headers(headers)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .text()
        .await
        .map_err(|e| format!("读取响应失败: {}", e))?;

    let request_id = get_header_value(&headers, "request-id");
    let x_request_id = get_header_value(&headers, "x-request-id");
    let cf_ray = get_header_value(&headers, "cf-ray");
    let body_len = body.len();

    logger::log_info(&format!(
        "Codex 配额响应元信息: url={}, status={}, request-id={}, x-request-id={}, cf-ray={}, body_len={}",
        USAGE_URL, status, request_id, x_request_id, cf_ray, body_len
    ));

    if !status.is_success() {
        let detail_code = extract_detail_code_from_body(&body);

        logger::log_error(&format!(
            "Codex 配额接口返回非成功状态: url={}, status={}, request-id={}, x-request-id={}, cf-ray={}, detail_code={:?}, body={}",
            USAGE_URL, status, request_id, x_request_id, cf_ray, detail_code, body
        ));

        let body_preview = if body.len() > 200 {
            &body[..200]
        } else {
            &body
        };
        let mut error_message = format!("API 返回错误 {}", status);
        if let Some(code) = detail_code {
            error_message.push_str(&format!(" [error_code:{}]", code));
        }
        error_message.push_str(&format!(" - {}", body_preview));
        return Err(error_message);
    }

    // 解析响应
    let usage: UsageResponse =
        serde_json::from_str(&body).map_err(|e| format!("解析 JSON 失败: {}", e))?;

    parse_quota_from_usage(&usage, &body)
}

/// 从使用率响应中解析配额信息
fn parse_quota_from_usage(usage: &UsageResponse, raw_body: &str) -> Result<CodexQuota, String> {
    let rate_limit = usage.rate_limit.as_ref();
    let primary_window = rate_limit.and_then(|r| r.primary_window.as_ref());
    let secondary_window = rate_limit.and_then(|r| r.secondary_window.as_ref());

    // Primary window = 5小时配额（session）
    let (hourly_percentage, hourly_reset_time, hourly_window_minutes) =
        if let Some(primary) = primary_window {
            (
                normalize_remaining_percentage(primary),
                normalize_reset_time(primary),
                normalize_window_minutes(primary),
            )
        } else {
            (100, None, None)
        };

    // Secondary window = 周配额
    let (weekly_percentage, weekly_reset_time, weekly_window_minutes) =
        if let Some(secondary) = secondary_window {
            (
                normalize_remaining_percentage(secondary),
                normalize_reset_time(secondary),
                normalize_window_minutes(secondary),
            )
        } else {
            (100, None, None)
        };

    // 保存原始响应
    let raw_data: Option<serde_json::Value> = serde_json::from_str(raw_body).ok();

    Ok(CodexQuota {
        hourly_percentage,
        hourly_reset_time,
        hourly_window_minutes,
        hourly_window_present: Some(primary_window.is_some()),
        weekly_percentage,
        weekly_reset_time,
        weekly_window_minutes,
        weekly_window_present: Some(secondary_window.is_some()),
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
                    let message = format!("Token 已过期且刷新失败: {}", e);
                    write_quota_error(&mut account, message.clone());
                    if let Err(save_err) = codex_account::save_account(&account) {
                        logger::log_warn(&format!("写入 Codex 配额错误失败: {}", save_err));
                    }
                    return Err(message);
                }
            }
        } else {
            let message = "Token 已过期且无 refresh_token".to_string();
            write_quota_error(&mut account, message.clone());
            if let Err(save_err) = codex_account::save_account(&account) {
                logger::log_warn(&format!("写入 Codex 配额错误失败: {}", save_err));
            }
            return Err(message);
        }
    }

    let quota = match fetch_quota(&account).await {
        Ok(quota) => quota,
        Err(e) => {
            write_quota_error(&mut account, e.clone());
            if let Err(save_err) = codex_account::save_account(&account) {
                logger::log_warn(&format!("写入 Codex 配额错误失败: {}", save_err));
            }
            return Err(e);
        }
    };

    account.quota = Some(quota.clone());
    account.quota_error = None;
    codex_account::save_account(&account)?;

    Ok(quota)
}

/// 刷新所有账号配额
pub async fn refresh_all_quotas() -> Result<Vec<(String, Result<CodexQuota, String>)>, String> {
    use futures::future::join_all;
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    const MAX_CONCURRENT: usize = 5;
    let accounts = codex_account::list_accounts();

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let tasks: Vec<_> = accounts
        .into_iter()
        .map(|account| {
            let account_id = account.id;
            let semaphore = semaphore.clone();
            async move {
                let _permit = semaphore
                    .acquire_owned()
                    .await
                    .map_err(|e| format!("获取 Codex 刷新并发许可失败: {}", e))?;
                let result = refresh_account_quota(&account_id).await;
                Ok::<(String, Result<CodexQuota, String>), String>((account_id, result))
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
