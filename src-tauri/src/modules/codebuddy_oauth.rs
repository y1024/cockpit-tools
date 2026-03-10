use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

use crate::models::codebuddy::{
    CodebuddyOAuthCompletePayload, CodebuddyOAuthStartResponse, CodebuddyQuotaRequestHeaders,
};
use crate::modules::logger;

const CODEBUDDY_API_ENDPOINT: &str = "https://www.codebuddy.ai";
const CODEBUDDY_API_PREFIX: &str = "/v2/plugin";
const CODEBUDDY_PLATFORM: &str = "ide";
const OAUTH_TIMEOUT_SECONDS: u64 = 600;
const OAUTH_POLL_INTERVAL_MS: u64 = 1500;
pub const PRE_AUTH_SNAPSHOT_WAIT_SECONDS: u64 = 90;

#[derive(Clone)]
struct PendingOAuthState {
    login_id: String,
    expires_at: i64,
    state: String,
    cancelled: bool,
    wait_pre_auth_snapshot: bool,
    pre_auth_quota_snapshot: Option<Value>,
    quota_bind_decision: Option<String>,
}

lazy_static::lazy_static! {
    static ref PENDING_OAUTH_STATE: Arc<Mutex<Option<PendingOAuthState>>> = Arc::new(Mutex::new(None));
}

fn now_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

fn generate_login_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..16).map(|_| rng.gen::<u8>()).collect();
    format!(
        "cb_{}",
        bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    )
}

fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))
}

pub fn set_wait_pre_auth_snapshot_by_state(
    state: Option<&str>,
    enabled: bool,
) -> Result<(), String> {
    let mut pending = PENDING_OAUTH_STATE
        .lock()
        .map_err(|_| "获取锁失败".to_string())?;
    let Some(current) = pending.as_mut() else {
        return Ok(());
    };
    if let Some(expected_state) = state {
        if current.state != expected_state {
            return Ok(());
        }
    }
    current.wait_pre_auth_snapshot = enabled;
    Ok(())
}

fn should_wait_for_pre_auth_snapshot(login_id: &str) -> Result<bool, String> {
    let pending = PENDING_OAUTH_STATE
        .lock()
        .map_err(|_| "获取锁失败".to_string())?;
    Ok(pending
        .as_ref()
        .map(|s| s.login_id == login_id && s.wait_pre_auth_snapshot)
        .unwrap_or(false))
}

async fn wait_and_take_pre_auth_snapshot(
    login_id: &str,
    timeout_seconds: u64,
) -> Result<Option<Value>, String> {
    let started = std::time::Instant::now();
    loop {
        {
            let mut pending = PENDING_OAUTH_STATE
                .lock()
                .map_err(|_| "获取锁失败".to_string())?;
            match pending.as_mut() {
                None => return Ok(None),
                Some(s) => {
                    if s.login_id != login_id || s.cancelled {
                        return Ok(None);
                    }
                    if let Some(snapshot) = s.pre_auth_quota_snapshot.take() {
                        return Ok(Some(snapshot));
                    }
                }
            }
        }
        if started.elapsed().as_secs() >= timeout_seconds {
            return Ok(None);
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
}

fn clear_pending_login(login_id: &str) -> Result<(), String> {
    let mut pending = PENDING_OAUTH_STATE
        .lock()
        .map_err(|_| "获取锁失败".to_string())?;
    if pending
        .as_ref()
        .map(|s| s.login_id == login_id)
        .unwrap_or(false)
    {
        *pending = None;
    }
    Ok(())
}

pub fn should_wait_pre_auth_snapshot(login_id: &str) -> Result<bool, String> {
    should_wait_for_pre_auth_snapshot(login_id)
}

pub async fn wait_pre_auth_snapshot(
    login_id: &str,
    timeout_seconds: u64,
) -> Result<Option<Value>, String> {
    wait_and_take_pre_auth_snapshot(login_id, timeout_seconds).await
}

pub fn set_quota_bind_decision(action: &str) -> Result<(), String> {
    let normalized = action.trim().to_lowercase();
    if normalized != "quota_retry" && normalized != "quota_skip" {
        return Err(format!("未知配额绑定决策: {}", action));
    }
    let mut pending = PENDING_OAUTH_STATE
        .lock()
        .map_err(|_| "获取锁失败".to_string())?;
    let state = pending
        .as_mut()
        .ok_or_else(|| "当前没有待处理的 OAuth 会话".to_string())?;
    if state.cancelled {
        return Err("OAuth 会话已取消".to_string());
    }
    state.quota_bind_decision = Some(normalized);
    Ok(())
}

pub async fn wait_quota_bind_decision(
    login_id: &str,
    timeout_seconds: u64,
) -> Result<Option<String>, String> {
    let started = std::time::Instant::now();
    loop {
        {
            let mut pending = PENDING_OAUTH_STATE
                .lock()
                .map_err(|_| "获取锁失败".to_string())?;
            match pending.as_mut() {
                None => return Ok(None),
                Some(s) => {
                    if s.login_id != login_id || s.cancelled {
                        return Ok(None);
                    }
                    if let Some(decision) = s.quota_bind_decision.take() {
                        return Ok(Some(decision));
                    }
                }
            }
        }
        if started.elapsed().as_secs() >= timeout_seconds {
            return Ok(None);
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
}

pub fn clear_pending_oauth_login(login_id: &str) -> Result<(), String> {
    clear_pending_login(login_id)
}

pub async fn start_login() -> Result<CodebuddyOAuthStartResponse, String> {
    let client = build_client()?;
    let url = format!(
        "{}{}/auth/state?platform={}",
        CODEBUDDY_API_ENDPOINT, CODEBUDDY_API_PREFIX, CODEBUDDY_PLATFORM
    );

    logger::log_info(&format!("[CodeBuddy OAuth] 请求 auth/state: {}", url));

    let resp = client
        .post(&url)
        .json(&json!({}))
        .send()
        .await
        .map_err(|e| format!("请求 auth/state 失败: {}", e))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析 auth/state 响应失败: {}", e))?;

    let data = body
        .get("data")
        .ok_or_else(|| format!("auth/state 响应缺少 data 字段: {}", body))?;

    let state = data
        .get("state")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "auth/state 响应缺少 state".to_string())?
        .to_string();

    let auth_url = data
        .get("authUrl")
        .or_else(|| data.get("auth_url"))
        .or_else(|| data.get("url"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let login_id = generate_login_id();

    let verification_uri = if auth_url.is_empty() {
        format!("{}/login?state={}", CODEBUDDY_API_ENDPOINT, state)
    } else {
        auth_url.clone()
    };

    {
        let mut pending = PENDING_OAUTH_STATE
            .lock()
            .map_err(|_| "获取锁失败".to_string())?;
        *pending = Some(PendingOAuthState {
            login_id: login_id.clone(),
            expires_at: now_timestamp() + OAUTH_TIMEOUT_SECONDS as i64,
            state: state.clone(),
            cancelled: false,
            wait_pre_auth_snapshot: false,
            pre_auth_quota_snapshot: None,
            quota_bind_decision: None,
        });
    }

    logger::log_info(&format!(
        "[CodeBuddy OAuth] 登录已启动: login_id={}, state={}",
        login_id, state
    ));

    Ok(CodebuddyOAuthStartResponse {
        login_id,
        verification_uri: verification_uri.clone(),
        verification_uri_complete: Some(verification_uri),
        expires_in: OAUTH_TIMEOUT_SECONDS,
        interval_seconds: OAUTH_POLL_INTERVAL_MS / 1000 + 1,
    })
}

pub async fn complete_login(login_id: &str) -> Result<CodebuddyOAuthCompletePayload, String> {
    let client = build_client()?;
    let start = now_timestamp();

    loop {
        let state_info = {
            let pending = PENDING_OAUTH_STATE
                .lock()
                .map_err(|_| "获取锁失败".to_string())?;
            match pending.as_ref() {
                None => return Err("没有待处理的登录请求".to_string()),
                Some(s) => {
                    if s.login_id != login_id {
                        return Err("login_id 不匹配".to_string());
                    }
                    if s.cancelled {
                        return Err("登录已取消".to_string());
                    }
                    if now_timestamp() > s.expires_at {
                        return Err("登录超时".to_string());
                    }
                    s.clone()
                }
            }
        };

        let url = format!(
            "{}{}/auth/token?state={}",
            CODEBUDDY_API_ENDPOINT, CODEBUDDY_API_PREFIX, state_info.state
        );

        match client.get(&url).send().await {
            Ok(resp) => {
                if let Ok(body) = resp.json::<Value>().await {
                    let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);

                    if code == 0 || code == 200 {
                        if let Some(data) = body.get("data") {
                            let access_token = data
                                .get("accessToken")
                                .or_else(|| data.get("access_token"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();

                            if !access_token.is_empty() {
                                logger::log_info("[CodeBuddy OAuth] 获取 token 成功");

                                let refresh_token = data
                                    .get("refreshToken")
                                    .or_else(|| data.get("refresh_token"))
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());

                                let expires_at = data
                                    .get("expiresAt")
                                    .or_else(|| data.get("expires_at"))
                                    .and_then(|v| v.as_i64());

                                let domain = data
                                    .get("domain")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());

                                let token_type = data
                                    .get("tokenType")
                                    .or_else(|| data.get("token_type"))
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());

                                let auth_raw = Some(data.clone());

                                let account_info = fetch_account_info(
                                    &client,
                                    &access_token,
                                    &state_info.state,
                                    domain.as_deref(),
                                )
                                .await;

                                let (
                                    uid,
                                    nickname,
                                    email,
                                    enterprise_id,
                                    enterprise_name,
                                    profile_raw,
                                ) = match account_info {
                                    Ok(info) => info,
                                    Err(e) => {
                                        logger::log_warn(&format!(
                                            "[CodeBuddy OAuth] 获取账号信息失败: {}",
                                            e
                                        ));
                                        (None, None, String::new(), None, None, None)
                                    }
                                };

                                return Ok(CodebuddyOAuthCompletePayload {
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
                                    auth_raw,
                                    profile_raw,
                                    usage_raw: None,
                                    quota_binding: None,
                                    status: Some("normal".to_string()),
                                    status_reason: None,
                                });
                            }
                        }
                    }
                }
            }
            Err(e) => {
                logger::log_warn(&format!("[CodeBuddy OAuth] 轮询 token 请求失败: {}", e));
            }
        }

        if now_timestamp() - start > OAUTH_TIMEOUT_SECONDS as i64 {
            let mut pending = PENDING_OAUTH_STATE
                .lock()
                .map_err(|_| "获取锁失败".to_string())?;
            *pending = None;
            return Err("登录超时".to_string());
        }

        tokio::time::sleep(std::time::Duration::from_millis(OAUTH_POLL_INTERVAL_MS)).await;
    }
}

pub fn cancel_login(login_id: Option<&str>) -> Result<(), String> {
    let mut pending = PENDING_OAUTH_STATE
        .lock()
        .map_err(|_| "获取锁失败".to_string())?;
    if let Some(state) = pending.as_mut() {
        if login_id.is_none() || login_id == Some(state.login_id.as_str()) {
            state.cancelled = true;
            *pending = None;
        }
    }
    Ok(())
}

pub fn cache_pre_auth_quota_snapshot(raw_json: &str) -> Result<(), String> {
    let parsed: Value =
        serde_json::from_str(raw_json).map_err(|e| format!("解析预拉取配额结果失败: {}", e))?;

    let mut pending = PENDING_OAUTH_STATE
        .lock()
        .map_err(|_| "获取锁失败".to_string())?;
    let state = pending
        .as_mut()
        .ok_or_else(|| "当前没有待处理的 OAuth 会话".to_string())?;
    if state.cancelled {
        return Err("OAuth 会话已取消".to_string());
    }
    state.pre_auth_quota_snapshot = Some(parsed);
    logger::log_info(&format!(
        "[CodeBuddy OAuth] 已缓存预拉取配额结果: login_id={}",
        state.login_id
    ));
    Ok(())
}

async fn fetch_account_info(
    client: &reqwest::Client,
    access_token: &str,
    state: &str,
    domain: Option<&str>,
) -> Result<
    (
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<Value>,
    ),
    String,
> {
    let url = format!(
        "{}{}/login/account?state={}",
        CODEBUDDY_API_ENDPOINT, CODEBUDDY_API_PREFIX, state
    );

    let mut req = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token));

    if let Some(d) = domain {
        req = req.header("X-Domain", d);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("请求 login/account 失败: {}", e))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析 login/account 响应失败: {}", e))?;

    let data = body.get("data").cloned().unwrap_or(json!({}));

    let uid = data
        .get("uid")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let nickname = data
        .get("nickname")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let email = data
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let enterprise_id = data
        .get("enterpriseId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let enterprise_name = data
        .get("enterpriseName")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let email_final = if email.is_empty() {
        nickname.clone().or_else(|| uid.clone()).unwrap_or_default()
    } else {
        email
    };

    Ok((
        uid,
        nickname,
        email_final,
        enterprise_id,
        enterprise_name,
        Some(data),
    ))
}

pub async fn refresh_token(
    access_token: &str,
    refresh_token: &str,
    domain: Option<&str>,
) -> Result<Value, String> {
    let client = build_client()?;
    let url = format!(
        "{}{}/auth/token/refresh",
        CODEBUDDY_API_ENDPOINT, CODEBUDDY_API_PREFIX
    );

    let mut req = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("X-Refresh-Token", refresh_token)
        .json(&json!({}));

    if let Some(d) = domain {
        req = req.header("X-Domain", d);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("刷新 token 失败: {}", e))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析刷新响应失败: {}", e))?;

    let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code != 0 && code != 200 {
        let msg = body
            .get("message")
            .or_else(|| body.get("msg"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Err(format!("刷新 token 失败 (code={}): {}", code, msg));
    }

    body.get("data")
        .cloned()
        .ok_or_else(|| "刷新响应缺少 data 字段".to_string())
}

pub async fn fetch_dosage_notify(
    access_token: &str,
    uid: Option<&str>,
    enterprise_id: Option<&str>,
    domain: Option<&str>,
) -> Result<Value, String> {
    let client = build_client()?;
    let url = format!(
        "{}/v2/billing/meter/get-dosage-notify",
        CODEBUDDY_API_ENDPOINT
    );

    let mut req = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json");

    if let Some(u) = uid {
        req = req.header("X-User-Id", u);
    }
    if let Some(eid) = enterprise_id {
        req = req.header("X-Enterprise-Id", eid);
        req = req.header("X-Tenant-Id", eid);
    }
    if let Some(d) = domain {
        req = req.header("X-Domain", d);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("请求 dosage notify 失败: {}", e))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析 dosage 响应失败: {}", e))?;

    Ok(body)
}

pub async fn fetch_payment_type(
    access_token: &str,
    uid: Option<&str>,
    enterprise_id: Option<&str>,
    domain: Option<&str>,
) -> Result<Value, String> {
    let client = build_client()?;
    let url = format!(
        "{}/v2/billing/meter/get-payment-type",
        CODEBUDDY_API_ENDPOINT
    );

    let mut req = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json");

    if let Some(u) = uid {
        req = req.header("X-User-Id", u);
    }
    if let Some(eid) = enterprise_id {
        req = req.header("X-Enterprise-Id", eid);
        req = req.header("X-Tenant-Id", eid);
    }
    if let Some(d) = domain {
        req = req.header("X-Domain", d);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("请求 payment type 失败: {}", e))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析 payment type 响应失败: {}", e))?;

    Ok(body)
}

pub async fn fetch_user_resource_with_cookie(
    cookie_header: &str,
    product_code: &str,
    status: &[i32],
    package_end_time_range_begin: &str,
    package_end_time_range_end: &str,
    page_number: i32,
    page_size: i32,
    request_headers: Option<&CodebuddyQuotaRequestHeaders>,
    user_agent_fallback: Option<&str>,
) -> Result<Value, String> {
    let client = build_client()?;
    let url = format!("{}/billing/meter/get-user-resource", CODEBUDDY_API_ENDPOINT);

    let body = json!({
        "PageNumber": page_number,
        "PageSize": page_size,
        "ProductCode": product_code,
        "Status": status,
        "PackageEndTimeRangeBegin": package_end_time_range_begin,
        "PackageEndTimeRangeEnd": package_end_time_range_end
    });

    let accept = request_headers
        .and_then(|h| h.accept.as_deref())
        .unwrap_or("application/json, text/plain, */*");
    let accept_language = request_headers
        .and_then(|h| h.accept_language.as_deref())
        .unwrap_or("zh-CN,zh;q=0.9");
    let content_type = request_headers
        .and_then(|h| h.content_type.as_deref())
        .unwrap_or("application/json");
    let origin = request_headers
        .and_then(|h| h.origin.as_deref())
        .unwrap_or(CODEBUDDY_API_ENDPOINT);
    let referer = request_headers
        .and_then(|h| h.referer.as_deref())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}/profile/usage", CODEBUDDY_API_ENDPOINT));
    let sec_fetch_site = request_headers
        .and_then(|h| h.sec_fetch_site.as_deref())
        .unwrap_or("same-origin");
    let sec_fetch_mode = request_headers
        .and_then(|h| h.sec_fetch_mode.as_deref())
        .unwrap_or("cors");
    let sec_fetch_dest = request_headers
        .and_then(|h| h.sec_fetch_dest.as_deref())
        .unwrap_or("empty");
    let effective_ua = request_headers
        .and_then(|h| h.user_agent.as_deref())
        .or(user_agent_fallback)
        .unwrap_or(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36",
        );

    let resp = client
        .post(&url)
        .header("Accept", accept)
        .header("Accept-Language", accept_language)
        .header("Content-Type", content_type)
        .header("Origin", origin)
        .header("Referer", referer)
        .header("Sec-Fetch-Site", sec_fetch_site)
        .header("Sec-Fetch-Mode", sec_fetch_mode)
        .header("Sec-Fetch-Dest", sec_fetch_dest)
        .header("User-Agent", effective_ua)
        .header("Cookie", cookie_header)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求 user resource（Cookie）失败: {}", e))?;

    let status_code = resp.status();

    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 user resource（Cookie）响应失败: {}", e))?;

    let text_snippet = {
        let preview = text.replace('\n', "\\n");
        let max_chars = 600usize;
        let mut out = String::new();
        let mut count = 0usize;
        for ch in preview.chars() {
            if count >= max_chars {
                out.push_str("...<truncated>");
                break;
            }
            out.push(ch);
            count += 1;
        }
        out
    };

    let result: Value = serde_json::from_str(&text).map_err(|e| {
        format!(
            "解析 user resource（Cookie）响应失败: {}, http={}, body_preview={}",
            e,
            status_code.as_u16(),
            text_snippet
        )
    })?;

    if !status_code.is_success() {
        return Err(format!(
            "请求 user resource（Cookie）失败: http={}, body={}",
            status_code.as_u16(),
            text
        ));
    }

    let code = result.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code != 0 && code != 200 {
        let msg = result
            .get("msg")
            .or_else(|| result.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Err(format!(
            "user resource（Cookie）返回失败: code={}, msg={}",
            code, msg
        ));
    }

    Ok(result)
}

pub async fn fetch_user_resource_with_raw_request(
    request_url: &str,
    request_method: &str,
    request_headers: &[(String, String)],
    request_body: Option<&str>,
) -> Result<Value, String> {
    let client = build_client()?;
    let method = reqwest::Method::from_bytes(request_method.trim().as_bytes())
        .map_err(|e| format!("cURL 请求方法无效: {}", e))?;
    let url = request_url.trim();
    if url.is_empty() {
        return Err("cURL 缺少请求 URL".to_string());
    }

    let mut req = client.request(method, url);
    for (name_raw, value_raw) in request_headers {
        let name = name_raw.trim();
        let value = value_raw.trim();
        if name.is_empty() || value.is_empty() {
            continue;
        }
        if name.eq_ignore_ascii_case("host") || name.eq_ignore_ascii_case("content-length") {
            continue;
        }
        let header_name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|e| format!("cURL 请求头无效（{}）: {}", name, e))?;
        let header_value = reqwest::header::HeaderValue::from_str(value)
            .map_err(|e| format!("cURL 请求头值无效（{}）: {}", name, e))?;
        req = req.header(header_name, header_value);
    }

    if let Some(body) = request_body {
        if !body.trim().is_empty() {
            req = req.body(body.to_string());
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("重放 cURL 请求 user resource 失败: {}", e))?;

    let status_code = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 user resource（cURL）响应失败: {}", e))?;

    let text_snippet = {
        let preview = text.replace('\n', "\\n");
        let max_chars = 600usize;
        let mut out = String::new();
        let mut count = 0usize;
        for ch in preview.chars() {
            if count >= max_chars {
                out.push_str("...<truncated>");
                break;
            }
            out.push(ch);
            count += 1;
        }
        out
    };

    let result: Value = serde_json::from_str(&text).map_err(|e| {
        format!(
            "解析 user resource（cURL）响应失败: {}, http={}, body_preview={}",
            e,
            status_code.as_u16(),
            text_snippet
        )
    })?;

    if !status_code.is_success() {
        return Err(format!(
            "请求 user resource（cURL）失败: http={}, body={}",
            status_code.as_u16(),
            text
        ));
    }

    let code = result.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code != 0 && code != 200 {
        let msg = result
            .get("msg")
            .or_else(|| result.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Err(format!(
            "user resource（cURL）返回失败: code={}, msg={}",
            code, msg
        ));
    }

    Ok(result)
}

async fn refresh_payload_for_account_inner(
    account: &crate::models::codebuddy::CodebuddyAccount,
    require_user_resource: bool,
) -> Result<CodebuddyOAuthCompletePayload, String> {
    let mut new_access_token = account.access_token.clone();
    let mut new_refresh_token = account.refresh_token.clone();
    let mut new_expires_at = account.expires_at;
    let mut new_domain = account.domain.clone();

    if let Some(refresh_tk) = account.refresh_token.as_deref() {
        match refresh_token(&account.access_token, refresh_tk, account.domain.as_deref()).await {
            Ok(token_data) => {
                new_access_token = token_data
                    .get("accessToken")
                    .or_else(|| token_data.get("access_token"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&account.access_token)
                    .to_string();

                new_refresh_token = token_data
                    .get("refreshToken")
                    .or_else(|| token_data.get("refresh_token"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| account.refresh_token.clone());

                new_expires_at = token_data
                    .get("expiresAt")
                    .or_else(|| token_data.get("expires_at"))
                    .and_then(|v| v.as_i64())
                    .or(account.expires_at);

                new_domain = token_data
                    .get("domain")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| account.domain.clone());
            }
            Err(e) => {
                logger::log_warn(&format!(
                    "[CodeBuddy] Token 刷新失败，将使用现有 token 查询配额: {}",
                    e
                ));
            }
        }
    }

    let dosage = fetch_dosage_notify(
        &new_access_token,
        account.uid.as_deref(),
        account.enterprise_id.as_deref(),
        new_domain.as_deref(),
    )
    .await
    .ok();

    let payment = fetch_payment_type(
        &new_access_token,
        account.uid.as_deref(),
        account.enterprise_id.as_deref(),
        new_domain.as_deref(),
    )
    .await
    .ok();

    let user_resource = if let Some(binding) = account.quota_binding.as_ref() {
        match fetch_user_resource_with_cookie(
            binding.cookie_header.as_str(),
            binding.product_code.as_str(),
            &binding.status,
            binding.package_end_time_range_begin.as_str(),
            binding.package_end_time_range_end.as_str(),
            binding.page_number,
            binding.page_size,
            binding.request_headers.as_ref(),
            binding.user_agent.as_deref(),
        )
        .await
        {
            Ok(payload) => Some(payload),
            Err(err) => {
                if require_user_resource {
                    return Err(format!("使用绑定参数刷新 user_resource 失败: {}", err));
                }
                logger::log_warn(&format!(
                    "[CodeBuddy] 使用绑定参数刷新 user_resource 失败: {}",
                    err
                ));
                None
            }
        }
    } else {
        if require_user_resource {
            return Err(
                "未配置查询配额绑定参数，无法刷新 user_resource（需要 session/session_2）"
                    .to_string(),
            );
        }
        logger::log_warn("[CodeBuddy] 未配置查询配额绑定参数，跳过 user_resource 刷新（请先完成一次 WebView 配额自动绑定）");
        None
    };

    let dosage_data = dosage.as_ref().and_then(|v| v.get("data"));
    let dosage_notify_code = dosage_data
        .and_then(|d| d.get("dosageNotifyCode"))
        .map(|v| match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => v.to_string(),
        });
    let dosage_notify_zh = dosage_data
        .and_then(|d| d.get("dosageNotifyZh"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let dosage_notify_en = dosage_data
        .and_then(|d| d.get("dosageNotifyEn"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let payment_data = payment.as_ref().and_then(|v| v.get("data"));
    let payment_type = payment_data
        .and_then(|d| {
            d.as_str().map(|s| s.to_string()).or_else(|| {
                d.get("paymentType")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
        })
        .or_else(|| account.payment_type.clone());

    let mut combined_quota = serde_json::Map::new();
    if let Some(d) = &dosage {
        combined_quota.insert("dosage".to_string(), d.clone());
    }
    if let Some(p) = &payment {
        combined_quota.insert("payment".to_string(), p.clone());
    }
    if let Some(r) = &user_resource {
        combined_quota.insert("userResource".to_string(), r.clone());
    }

    let quota_raw = if combined_quota.is_empty() {
        account.quota_raw.clone()
    } else {
        Some(Value::Object(combined_quota))
    };

    Ok(CodebuddyOAuthCompletePayload {
        email: account.email.clone(),
        uid: account.uid.clone(),
        nickname: account.nickname.clone(),
        enterprise_id: account.enterprise_id.clone(),
        enterprise_name: account.enterprise_name.clone(),
        access_token: new_access_token,
        refresh_token: new_refresh_token,
        token_type: account.token_type.clone(),
        expires_at: new_expires_at,
        domain: new_domain,
        plan_type: account.plan_type.clone(),
        dosage_notify_code,
        dosage_notify_zh,
        dosage_notify_en,
        payment_type,
        quota_raw,
        auth_raw: account.auth_raw.clone(),
        profile_raw: account.profile_raw.clone(),
        usage_raw: user_resource.or_else(|| account.usage_raw.clone()),
        quota_binding: account.quota_binding.clone(),
        status: account.status.clone(),
        status_reason: account.status_reason.clone(),
    })
}

pub async fn refresh_payload_for_account(
    account: &crate::models::codebuddy::CodebuddyAccount,
) -> Result<CodebuddyOAuthCompletePayload, String> {
    refresh_payload_for_account_inner(account, false).await
}

pub async fn refresh_payload_for_account_strict(
    account: &crate::models::codebuddy::CodebuddyAccount,
) -> Result<CodebuddyOAuthCompletePayload, String> {
    refresh_payload_for_account_inner(account, true).await
}

pub async fn build_payload_from_token(
    access_token: &str,
) -> Result<CodebuddyOAuthCompletePayload, String> {
    let client = build_client()?;

    let url = format!(
        "{}{}/accounts",
        CODEBUDDY_API_ENDPOINT, CODEBUDDY_API_PREFIX
    );

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("请求 accounts 失败: {}", e))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析 accounts 响应失败: {}", e))?;

    let accounts = body
        .get("data")
        .and_then(|d| d.get("accounts"))
        .and_then(|a| a.as_array());

    let account_data = accounts
        .and_then(|arr| {
            arr.iter().find(|a| {
                a.get("lastLogin")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            })
        })
        .or_else(|| accounts.and_then(|arr| arr.first()))
        .cloned()
        .unwrap_or(json!({}));

    let uid = account_data
        .get("uid")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let nickname = account_data
        .get("nickname")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let email = account_data
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let enterprise_id = account_data
        .get("enterpriseId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let enterprise_name = account_data
        .get("enterpriseName")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let email_final = if email.is_empty() {
        nickname
            .clone()
            .or_else(|| uid.clone())
            .unwrap_or_else(|| "unknown".to_string())
    } else {
        email
    };

    Ok(CodebuddyOAuthCompletePayload {
        email: email_final,
        uid,
        nickname,
        enterprise_id,
        enterprise_name,
        access_token: access_token.to_string(),
        refresh_token: None,
        token_type: Some("Bearer".to_string()),
        expires_at: None,
        domain: None,
        plan_type: None,
        dosage_notify_code: None,
        dosage_notify_zh: None,
        dosage_notify_en: None,
        payment_type: None,
        quota_raw: None,
        auth_raw: None,
        profile_raw: Some(account_data),
        usage_raw: None,
        quota_binding: None,
        status: Some("normal".to_string()),
        status_reason: None,
    })
}
