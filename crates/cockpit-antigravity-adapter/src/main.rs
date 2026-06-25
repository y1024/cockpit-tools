use base64::{engine::general_purpose, Engine as _};
use cockpit_core::models::{
    Account, DefaultInstanceSettings, InstanceProfileView, InstanceStore, TokenData,
};
use cockpit_core::modules::{
    self, account, antigravity_legacy_instance, antigravity_switch_history, instance, logger,
    process, wakeup, wakeup_history, wakeup_scheduler, wakeup_verification,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tiny_http::{Header, Method, Response, Server, StatusCode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use tokio::sync::{oneshot, watch};
use tokio::time::timeout;
use uuid::Uuid;

const DEFAULT_INSTANCE_ID: &str = "__default__";
const HOST_EVENT_TIMEOUT: Duration = Duration::from_secs(5);
const OAUTH_CALLBACK_PATH: &str = "/oauth-callback";
const MAX_HTTP_REQUEST_BYTES: usize = 32 * 1024;
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(5);
const OAUTH_FLOW_WAIT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeTarget {
    Legacy,
    Ide,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RpcRequest {
    method: String,
    #[serde(default)]
    payload: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcError {
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostEventResponse {
    ok: bool,
    error: Option<String>,
}

struct OAuthFlowState {
    auth_url: String,
    redirect_uri: String,
    expected_state: String,
    cancel_tx: watch::Sender<bool>,
    code_tx: Arc<tokio::sync::Mutex<Option<oneshot::Sender<Result<String, String>>>>>,
    code_rx: Option<oneshot::Receiver<Result<String, String>>>,
}

static OAUTH_FLOW_STATE: OnceLock<Mutex<Option<OAuthFlowState>>> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountIdPayload {
    account_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveAccountPayload {
    account: Account,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountIdsPayload {
    account_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RefreshTokenPayload {
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuthCallbackPayload {
    callback_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelScopePayload {
    cancel_scope_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfficialLsVersionModePayload {
    official_ls_version_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CrontabPayload {
    expr: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WakeupHistoryItemsPayload {
    items: Vec<wakeup_history::WakeupHistoryItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WakeupTriggerPayload {
    account_id: String,
    model: String,
    prompt: Option<String>,
    max_output_tokens: Option<u32>,
    cancel_scope_id: Option<String>,
    official_ls_version_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WakeupSyncStatePayload {
    enabled: bool,
    tasks: Vec<wakeup_scheduler::WakeupTaskInput>,
    official_ls_version_mode: Option<String>,
    run_startup_tasks: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WakeupRunEnabledTasksPayload {
    trigger_source: Option<String>,
    official_ls_version_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WakeupVerificationRunBatchPayload {
    account_ids: Vec<String>,
    model: String,
    prompt: Option<String>,
    max_output_tokens: Option<u32>,
    official_ls_version_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WakeupTaskIdPayload {
    task_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchIdsPayload {
    batch_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonImportPayload {
    json_content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileImportPayload {
    file_paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TagsPayload {
    account_id: String,
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotesPayload {
    account_id: String,
    notes: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupDataPayload {
    data: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstanceStorePayload {
    store: InstanceStore,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetectLaunchPathPayload {
    force: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateInstancePayload {
    name: String,
    user_data_dir: String,
    extra_args: Option<String>,
    bind_account_id: Option<String>,
    copy_source_instance_id: Option<String>,
    init_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInstancePayload {
    instance_id: String,
    name: Option<String>,
    extra_args: Option<String>,
    bind_account_id: Option<Option<String>>,
    follow_local_account: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstanceIdPayload {
    instance_id: String,
}

#[derive(Debug, Serialize)]
struct SimpleAccount {
    email: String,
    refresh_token: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
}

fn runtime_target() -> RuntimeTarget {
    match std::env::var("COCKPIT_PLATFORM_ID")
        .unwrap_or_default()
        .trim()
    {
        "antigravity" => RuntimeTarget::Legacy,
        _ => RuntimeTarget::Ide,
    }
}

fn runtime_label(target: RuntimeTarget) -> &'static str {
    match target {
        RuntimeTarget::Legacy => "Antigravity",
        RuntimeTarget::Ide => "Antigravity IDE",
    }
}

fn json_header() -> Header {
    Header::from_bytes(
        &b"Content-Type"[..],
        &b"application/json; charset=utf-8"[..],
    )
    .expect("valid content-type header")
}

fn to_value<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value)
        .map_err(|error| format!("序列化 Antigravity adapter 响应失败: {}", error))
}

fn parse_payload<T: for<'de> Deserialize<'de>>(payload: Value) -> Result<T, String> {
    serde_json::from_value(payload)
        .map_err(|error| format!("解析 Antigravity adapter 请求失败: {}", error))
}

fn emit_host_event_on_blocking_thread(event: String, payload: Value) -> Result<(), String> {
    let url = env::var("COCKPIT_HOST_EVENT_URL")
        .map_err(|_| "Antigravity adapter 缺少宿主事件桥 URL".to_string())?;
    let token = env::var("COCKPIT_HOST_EVENT_TOKEN")
        .map_err(|_| "Antigravity adapter 缺少宿主事件桥 token".to_string())?;
    let response = reqwest::blocking::Client::builder()
        .timeout(HOST_EVENT_TIMEOUT)
        .build()
        .map_err(|error| format!("创建宿主事件桥客户端失败: {}", error))?
        .post(url)
        .bearer_auth(token)
        .json(&json!({
            "event": event,
            "payload": payload,
        }))
        .send()
        .map_err(|error| format!("发送宿主事件失败: {}", error))?;
    if !response.status().is_success() {
        return Err(format!("宿主事件桥返回 HTTP {}", response.status()));
    }
    let body = response
        .json::<HostEventResponse>()
        .map_err(|error| format!("解析宿主事件桥响应失败: {}", error))?;
    if body.ok {
        Ok(())
    } else {
        Err(body
            .error
            .unwrap_or_else(|| "宿主事件桥转发失败".to_string()))
    }
}

fn emit_host_event(event: &str, payload: Value) -> Result<(), String> {
    let event = event.to_string();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = emit_host_event_on_blocking_thread(event, payload);
        let _ = sender.send(result);
    });
    receiver
        .recv_timeout(HOST_EVENT_TIMEOUT.saturating_add(Duration::from_secs(1)))
        .map_err(|_| "发送宿主事件超时".to_string())?
}

fn wakeup_scheduler_event_emitter() -> wakeup_scheduler::WakeupSchedulerEventEmitter {
    Arc::new(|event, payload| {
        if let Err(error) = emit_host_event(event, payload) {
            logger::log_warn(&format!(
                "[AntigravityAdapter] 转发唤醒调度事件失败: event={}, error={}",
                event, error
            ));
        }
    })
}

fn wakeup_verification_progress_emitter() -> wakeup_verification::WakeupVerificationProgressEmitter
{
    Arc::new(|payload| {
        if let Err(error) = emit_host_event(
            wakeup_verification::WAKEUP_VERIFICATION_PROGRESS_EVENT,
            payload,
        ) {
            logger::log_warn(&format!(
                "[AntigravityAdapter] 转发账户检测进度事件失败: {}",
                error
            ));
        }
    })
}

fn get_oauth_flow_state() -> &'static Mutex<Option<OAuthFlowState>> {
    OAUTH_FLOW_STATE.get_or_init(|| Mutex::new(None))
}

fn oauth_success_html() -> &'static str {
    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
    <html>\
    <body style='font-family: sans-serif; text-align: center; padding: 50px; background: #0d1117; color: #fff;'>\
        <h1 style='color: #4ade80;'>授权成功</h1>\
        <p>您可以关闭此窗口返回应用。</p>\
        <script>setTimeout(function() { window.close(); }, 2000);</script>\
    </body>\
    </html>"
}

fn oauth_fail_html(message: &str) -> String {
    format!(
        "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
    <html>\
    <body style='font-family: sans-serif; text-align: center; padding: 50px; background: #0d1117; color: #fff;'>\
        <h1 style='color: #f87171;'>授权失败</h1>\
        <p>{}</p>\
    </body>\
    </html>",
        message
    )
}

fn oauth_not_found_response() -> &'static str {
    "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nNot Found"
}

fn oauth_options_response() -> &'static str {
    "HTTP/1.1 200 OK\r\n\
    Access-Control-Allow-Origin: *\r\n\
    Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
    Access-Control-Allow-Headers: Content-Type\r\n\
    Content-Length: 0\r\n\r\n"
}

fn clear_oauth_flow_state() {
    if let Ok(mut lock) = get_oauth_flow_state().lock() {
        *lock = None;
    }
}

fn extract_code_from_callback_url(
    callback_url: &url::Url,
    expected_state: &str,
) -> Result<String, String> {
    let mut code = None;
    let mut state = None;
    for (key, value) in callback_url.query_pairs() {
        match key.as_ref() {
            "code" if code.is_none() => code = Some(value.into_owned()),
            "state" if state.is_none() => state = Some(value.into_owned()),
            _ => {}
        }
    }

    let Some(code) = code.filter(|value| !value.trim().is_empty()) else {
        return Err("未能在回调中获取 Authorization Code".to_string());
    };
    let Some(state) = state.filter(|value| !value.trim().is_empty()) else {
        return Err("未能在回调中获取 OAuth state".to_string());
    };
    if state != expected_state {
        return Err("OAuth state 校验失败".to_string());
    }
    Ok(code)
}

fn parse_manual_callback_url(
    raw_callback_url: &str,
    redirect_uri: &str,
) -> Result<url::Url, String> {
    let trimmed = raw_callback_url.trim();
    if trimmed.is_empty() {
        return Err("回调链接不能为空".to_string());
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return url::Url::parse(trimmed).map_err(|e| format!("OAuth 回调 URL 解析失败: {}", e));
    }

    let redirect =
        url::Url::parse(redirect_uri).map_err(|e| format!("OAuth redirect_uri 无效: {}", e))?;
    let host = redirect
        .host_str()
        .ok_or_else(|| "OAuth redirect_uri 缺少 host".to_string())?;
    let origin = match redirect.port() {
        Some(port) => format!("{}://{}:{}", redirect.scheme(), host, port),
        None => format!("{}://{}", redirect.scheme(), host),
    };

    if trimmed.starts_with('/') {
        return url::Url::parse(format!("{}{}", origin, trimmed).as_str())
            .map_err(|e| format!("OAuth 回调 URL 解析失败: {}", e));
    }

    url::Url::parse(
        format!(
            "{}{}?{}",
            origin,
            OAUTH_CALLBACK_PATH,
            trimmed.trim_start_matches('?')
        )
        .as_str(),
    )
    .map_err(|e| format!("OAuth 回调 URL 解析失败: {}", e))
}

async fn read_http_request(stream: &mut tokio::net::TcpStream) -> Result<String, String> {
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0u8; 2048];

    loop {
        let bytes_read = timeout(REQUEST_READ_TIMEOUT, stream.read(&mut chunk))
            .await
            .map_err(|_| "读取 OAuth 回调请求超时".to_string())?
            .map_err(|e| format!("读取 OAuth 回调请求失败: {}", e))?;

        if bytes_read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..bytes_read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n")
            || buffer.len() >= MAX_HTTP_REQUEST_BYTES
        {
            break;
        }
    }

    if buffer.is_empty() {
        return Err("OAuth 回调请求为空".to_string());
    }
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

fn parse_request_target(request: &str) -> Result<(String, String), String> {
    let request_line = request
        .lines()
        .next()
        .ok_or_else(|| "OAuth 回调请求行为空".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "OAuth 回调请求缺少 method".to_string())?;
    let target = parts
        .next()
        .ok_or_else(|| "OAuth 回调请求缺少 target".to_string())?;
    Ok((method.to_string(), target.to_string()))
}

async fn process_callback_request(
    stream: &mut tokio::net::TcpStream,
    port: u16,
    expected_state: &str,
) -> Option<Result<String, String>> {
    let request = match read_http_request(stream).await {
        Ok(request) => request,
        Err(err) => {
            let response = oauth_fail_html("回调请求读取失败，请返回应用重试。");
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return Some(Err(err));
        }
    };

    let (method, target) = match parse_request_target(&request) {
        Ok(parsed) => parsed,
        Err(err) => {
            let response = oauth_fail_html("回调请求格式无效，请返回应用重试。");
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return Some(Err(err));
        }
    };

    if method.eq_ignore_ascii_case("OPTIONS") {
        let _ = stream.write_all(oauth_options_response().as_bytes()).await;
        let _ = stream.flush().await;
        return None;
    }

    let callback_url = match if target.starts_with("http://") || target.starts_with("https://") {
        url::Url::parse(&target)
    } else {
        url::Url::parse(&format!("http://localhost:{}{}", port, target))
    } {
        Ok(url) => url,
        Err(_) => {
            let response = oauth_fail_html("回调 URL 解析失败，请返回应用重试。");
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return Some(Err("OAuth 回调 URL 解析失败".to_string()));
        }
    };

    if callback_url.path() != OAUTH_CALLBACK_PATH {
        let _ = stream
            .write_all(oauth_not_found_response().as_bytes())
            .await;
        let _ = stream.flush().await;
        return None;
    }

    let code = match extract_code_from_callback_url(&callback_url, expected_state) {
        Ok(code) => code,
        Err(err) if err == "未能在回调中获取 Authorization Code" => {
            let response = oauth_fail_html("未能获取授权 code，请返回应用重试。");
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return Some(Err(err));
        }
        Err(err) if err == "未能在回调中获取 OAuth state" => {
            let response = oauth_fail_html("未能获取授权状态 state，请返回应用重试。");
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return Some(Err(err));
        }
        Err(err) => {
            let response = oauth_fail_html("授权状态校验失败，请返回应用重新发起授权。");
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return Some(Err(err));
        }
    };

    let _ = stream.write_all(oauth_success_html().as_bytes()).await;
    let _ = stream.flush().await;
    Some(Ok(code))
}

fn path_missing_error(error: &str) -> bool {
    error.starts_with("APP_PATH_NOT_FOUND:")
}

fn is_profile_initialized(target: RuntimeTarget, user_data_dir: &str) -> bool {
    let path = Path::new(user_data_dir);
    match target {
        RuntimeTarget::Legacy => antigravity_legacy_instance::is_profile_initialized(path),
        RuntimeTarget::Ide => instance::is_profile_initialized(path),
    }
}

fn default_user_data_dir(target: RuntimeTarget) -> Result<PathBuf, String> {
    match target {
        RuntimeTarget::Legacy => antigravity_legacy_instance::get_default_user_data_dir(),
        RuntimeTarget::Ide => instance::get_default_user_data_dir(),
    }
}

fn load_instance_store(target: RuntimeTarget) -> Result<InstanceStore, String> {
    match target {
        RuntimeTarget::Legacy => antigravity_legacy_instance::load_instance_store(),
        RuntimeTarget::Ide => instance::load_instance_store(),
    }
}

fn save_instance_store(target: RuntimeTarget, store: &InstanceStore) -> Result<(), String> {
    match target {
        RuntimeTarget::Legacy => antigravity_legacy_instance::save_instance_store(store),
        RuntimeTarget::Ide => instance::save_instance_store(store),
    }
}

fn load_default_settings(target: RuntimeTarget) -> Result<DefaultInstanceSettings, String> {
    match target {
        RuntimeTarget::Legacy => antigravity_legacy_instance::load_default_settings(),
        RuntimeTarget::Ide => instance::load_default_settings(),
    }
}

fn update_default_settings(
    target: RuntimeTarget,
    bind_account_id: Option<Option<String>>,
    extra_args: Option<String>,
    follow_local_account: Option<bool>,
) -> Result<DefaultInstanceSettings, String> {
    match target {
        RuntimeTarget::Legacy => antigravity_legacy_instance::update_default_settings(
            bind_account_id,
            extra_args,
            follow_local_account,
        ),
        RuntimeTarget::Ide => {
            instance::update_default_settings(bind_account_id, extra_args, follow_local_account)
        }
    }
}

fn update_default_pid(
    target: RuntimeTarget,
    pid: Option<u32>,
) -> Result<DefaultInstanceSettings, String> {
    match target {
        RuntimeTarget::Legacy => antigravity_legacy_instance::update_default_pid(pid),
        RuntimeTarget::Ide => instance::update_default_pid(pid),
    }
}

fn update_instance_pid(
    target: RuntimeTarget,
    instance_id: &str,
    pid: Option<u32>,
) -> Result<cockpit_core::models::InstanceProfile, String> {
    match target {
        RuntimeTarget::Legacy => antigravity_legacy_instance::update_instance_pid(instance_id, pid),
        RuntimeTarget::Ide => instance::update_instance_pid(instance_id, pid),
    }
}

fn clear_all_pids(target: RuntimeTarget) -> Result<(), String> {
    match target {
        RuntimeTarget::Legacy => antigravity_legacy_instance::clear_all_pids(),
        RuntimeTarget::Ide => instance::clear_all_pids(),
    }
}

fn resolve_pid(
    target: RuntimeTarget,
    last_pid: Option<u32>,
    user_data_dir: Option<&str>,
) -> Option<u32> {
    match target {
        RuntimeTarget::Legacy => process::resolve_antigravity_legacy_pid(last_pid, user_data_dir),
        RuntimeTarget::Ide => process::resolve_antigravity_pid(last_pid, user_data_dir),
    }
}

fn ensure_launch_path(target: RuntimeTarget) -> Result<(), String> {
    match target {
        RuntimeTarget::Legacy => process::ensure_antigravity_legacy_launch_path_configured(),
        RuntimeTarget::Ide => process::ensure_antigravity_launch_path_configured(),
    }
}

fn inject_account_to_profile(
    target: RuntimeTarget,
    profile_dir: &Path,
    account_id: &str,
) -> Result<(), String> {
    match target {
        RuntimeTarget::Legacy => {
            antigravity_legacy_instance::inject_account_to_profile(profile_dir, account_id)
        }
        RuntimeTarget::Ide => instance::inject_account_to_profile(profile_dir, account_id),
    }
}

fn parse_extra_args(value: &str) -> Vec<String> {
    process::parse_extra_args(value)
}

fn start_runtime_with_args(
    target: RuntimeTarget,
    user_data_dir: &str,
    extra_args: &[String],
) -> Result<u32, String> {
    match target {
        RuntimeTarget::Legacy => {
            process::start_antigravity_legacy_with_args(user_data_dir, extra_args)
        }
        RuntimeTarget::Ide => process::start_antigravity_with_args(user_data_dir, extra_args),
    }
}

fn close_runtime_dirs(target: RuntimeTarget, target_dirs: &[String]) -> Result<(), String> {
    match target {
        RuntimeTarget::Legacy => {
            let default_dir = antigravity_legacy_instance::get_default_user_data_dir()?;
            process::close_antigravity_legacy_instances(
                target_dirs,
                &default_dir.to_string_lossy(),
                20,
            )
        }
        RuntimeTarget::Ide => process::close_antigravity_instances(target_dirs, 20),
    }
}

fn focus_instance(
    target: RuntimeTarget,
    last_pid: Option<u32>,
    user_data_dir: Option<&str>,
) -> Result<(), String> {
    match target {
        RuntimeTarget::Legacy => {
            process::focus_antigravity_legacy_instance(last_pid, user_data_dir)
                .map(|_| ())
                .map_err(|err| format!("定位 Antigravity 实例窗口失败: {}", err))
        }
        RuntimeTarget::Ide => process::focus_antigravity_instance(last_pid, user_data_dir)
            .map(|_| ())
            .map_err(|err| format!("定位 Antigravity IDE 实例窗口失败: {}", err)),
    }
}

fn collect_instance_views(target: RuntimeTarget) -> Result<Vec<InstanceProfileView>, String> {
    let store = load_instance_store(target)?;
    let default_settings = store.default_settings.clone();
    let mut result: Vec<InstanceProfileView> = match target {
        RuntimeTarget::Legacy => {
            let entries = process::collect_antigravity_legacy_process_entries();
            store
                .instances
                .into_iter()
                .map(|instance| {
                    let resolved_pid = process::resolve_antigravity_legacy_pid_from_entries(
                        instance.last_pid,
                        Some(&instance.user_data_dir),
                        &entries,
                    );
                    let initialized = is_profile_initialized(target, &instance.user_data_dir);
                    let mut view = InstanceProfileView::from_profile(
                        instance,
                        resolved_pid.is_some(),
                        initialized,
                    );
                    view.last_pid = resolved_pid;
                    view
                })
                .collect()
        }
        RuntimeTarget::Ide => {
            let entries = process::collect_antigravity_process_entries();
            store
                .instances
                .into_iter()
                .map(|instance| {
                    let resolved_pid = process::resolve_antigravity_pid_from_entries(
                        instance.last_pid,
                        Some(&instance.user_data_dir),
                        &entries,
                    );
                    let initialized = is_profile_initialized(target, &instance.user_data_dir);
                    let mut view = InstanceProfileView::from_profile(
                        instance,
                        resolved_pid.is_some(),
                        initialized,
                    );
                    view.last_pid = resolved_pid;
                    view
                })
                .collect()
        }
    };

    let default_dir = default_user_data_dir(target)?;
    let default_dir_str = default_dir.to_string_lossy().to_string();
    let default_pid = resolve_pid(target, default_settings.last_pid, None);
    result.push(InstanceProfileView {
        id: DEFAULT_INSTANCE_ID.to_string(),
        name: String::new(),
        user_data_dir: default_dir_str,
        working_dir: None,
        extra_args: default_settings.extra_args.clone(),
        bind_account_id: resolve_default_account_id(target, &default_settings),
        created_at: 0,
        last_launched_at: None,
        last_pid: default_pid,
        running: default_pid.is_some(),
        initialized: match target {
            RuntimeTarget::Legacy => {
                antigravity_legacy_instance::is_profile_initialized(&default_dir)
            }
            RuntimeTarget::Ide => instance::is_profile_initialized(&default_dir),
        },
        is_default: true,
        follow_local_account: default_settings.follow_local_account,
    });

    Ok(result)
}

fn resolve_default_account_id(
    target: RuntimeTarget,
    settings: &DefaultInstanceSettings,
) -> Option<String> {
    if !settings.follow_local_account {
        return settings.bind_account_id.clone();
    }
    match target {
        RuntimeTarget::Legacy => resolve_legacy_local_account_id()
            .or_else(|| account::get_current_account_id().ok().flatten()),
        RuntimeTarget::Ide => resolve_ide_local_account_id(),
    }
}

fn resolve_ide_local_account_id() -> Option<String> {
    let db_path = modules::db::get_db_path().ok()?;
    resolve_account_id_from_state_db(&db_path)
}

fn resolve_legacy_local_account_id() -> Option<String> {
    let default_dir = antigravity_legacy_instance::get_default_user_data_dir().ok()?;
    let db_path = default_dir
        .join("User")
        .join("globalStorage")
        .join("state.vscdb");
    resolve_account_id_from_state_db(&db_path)
}

fn resolve_account_id_from_state_db(db_path: &Path) -> Option<String> {
    let conn = Connection::open(db_path).ok()?;
    let state_data: String = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = ?",
            ["antigravityUnifiedStateSync.oauthToken"],
            |row| row.get(0),
        )
        .ok()?;
    let blob = general_purpose::STANDARD.decode(&state_data).ok()?;
    let local_refresh_token =
        cockpit_core::utils::protobuf::extract_refresh_token_from_unified_oauth_token(&blob)?;
    if local_refresh_token.is_empty() {
        return None;
    }
    account::list_accounts()
        .ok()?
        .into_iter()
        .find(|account| account.token.refresh_token == local_refresh_token)
        .map(|account| account.id)
}

fn create_instance(target: RuntimeTarget, payload: Value) -> Result<Value, String> {
    let payload: CreateInstancePayload = parse_payload(payload)?;
    let params = modules::instance_store::CreateInstanceParams {
        working_dir: None,
        name: payload.name,
        user_data_dir: payload.user_data_dir,
        extra_args: payload.extra_args.unwrap_or_default(),
        bind_account_id: payload.bind_account_id,
        copy_source_instance_id: payload.copy_source_instance_id,
        init_mode: payload.init_mode,
    };
    let profile = match target {
        RuntimeTarget::Legacy => antigravity_legacy_instance::create_instance(params)?,
        RuntimeTarget::Ide => instance::create_instance(params)?,
    };
    to_value(InstanceProfileView::from_profile(
        profile.clone(),
        false,
        is_profile_initialized(target, &profile.user_data_dir),
    ))
}

fn update_instance(target: RuntimeTarget, payload: Value) -> Result<Value, String> {
    let payload: UpdateInstancePayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = default_user_data_dir(target)?;
        let updated = update_default_settings(
            target,
            payload.bind_account_id,
            payload.extra_args,
            payload.follow_local_account,
        )?;
        return to_value(InstanceProfileView {
            id: DEFAULT_INSTANCE_ID.to_string(),
            name: String::new(),
            user_data_dir: default_dir.to_string_lossy().to_string(),
            working_dir: None,
            extra_args: updated.extra_args.clone(),
            bind_account_id: resolve_default_account_id(target, &updated),
            created_at: 0,
            last_launched_at: None,
            last_pid: updated.last_pid,
            running: updated
                .last_pid
                .and_then(|pid| resolve_pid(target, Some(pid), None))
                .is_some(),
            initialized: match target {
                RuntimeTarget::Legacy => {
                    antigravity_legacy_instance::is_profile_initialized(&default_dir)
                }
                RuntimeTarget::Ide => instance::is_profile_initialized(&default_dir),
            },
            is_default: true,
            follow_local_account: updated.follow_local_account,
        });
    }

    if payload
        .bind_account_id
        .as_ref()
        .and_then(|next| next.as_ref())
        .is_some()
    {
        let store = load_instance_store(target)?;
        if let Some(profile) = store
            .instances
            .iter()
            .find(|item| item.id == payload.instance_id)
        {
            if !is_profile_initialized(target, &profile.user_data_dir) {
                return Err(
                    "INSTANCE_NOT_INITIALIZED:请先启动一次实例创建数据后，再进行账号绑定"
                        .to_string(),
                );
            }
        }
    }

    let params = modules::instance_store::UpdateInstanceParams {
        working_dir: None,
        instance_id: payload.instance_id,
        name: payload.name,
        extra_args: payload.extra_args,
        bind_account_id: payload.bind_account_id,
    };
    let profile = match target {
        RuntimeTarget::Legacy => antigravity_legacy_instance::update_instance(params)?,
        RuntimeTarget::Ide => instance::update_instance(params)?,
    };
    let running = profile
        .last_pid
        .and_then(|pid| resolve_pid(target, Some(pid), Some(&profile.user_data_dir)))
        .is_some();
    let initialized = is_profile_initialized(target, &profile.user_data_dir);
    to_value(InstanceProfileView::from_profile(
        profile,
        running,
        initialized,
    ))
}

fn delete_instance(target: RuntimeTarget, payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        return Err("默认实例不可删除".to_string());
    }
    match target {
        RuntimeTarget::Legacy => antigravity_legacy_instance::delete_instance(&payload.instance_id),
        RuntimeTarget::Ide => instance::delete_instance(&payload.instance_id),
    }?;
    Ok(Value::Null)
}

fn start_instance(
    target: RuntimeTarget,
    payload: Value,
    runtime: &Runtime,
) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    ensure_launch_path(target)?;

    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = default_user_data_dir(target)?;
        let default_dir_str = default_dir.to_string_lossy().to_string();
        let default_settings = load_default_settings(target)?;
        let bind_account_id = resolve_default_account_id(target, &default_settings);
        if let Some(pid) = resolve_pid(target, default_settings.last_pid, None) {
            process::close_pid(pid, 20)?;
            let _ = update_default_pid(target, None);
        }
        if let Some(account_id) = bind_account_id.as_deref() {
            let _ = runtime.block_on(account::prepare_account_for_injection(account_id))?;
            inject_account_to_profile(target, &default_dir, account_id)?;
        }
        let extra_args = parse_extra_args(&default_settings.extra_args);
        let pid = start_runtime_with_args(target, "", &extra_args)?;
        let _ = update_default_pid(target, Some(pid));
        return to_value(InstanceProfileView {
            id: DEFAULT_INSTANCE_ID.to_string(),
            name: String::new(),
            user_data_dir: default_dir_str,
            working_dir: None,
            extra_args: default_settings.extra_args,
            bind_account_id,
            created_at: 0,
            last_launched_at: None,
            last_pid: Some(pid),
            running: process::is_pid_running(pid),
            initialized: match target {
                RuntimeTarget::Legacy => {
                    antigravity_legacy_instance::is_profile_initialized(&default_dir)
                }
                RuntimeTarget::Ide => instance::is_profile_initialized(&default_dir),
            },
            is_default: true,
            follow_local_account: default_settings.follow_local_account,
        });
    }

    let store = load_instance_store(target)?;
    let profile = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;
    if let Some(pid) = resolve_pid(target, profile.last_pid, Some(&profile.user_data_dir)) {
        process::close_pid(pid, 20)?;
        let _ = update_instance_pid(target, &profile.id, None)?;
    }
    if let Some(account_id) = profile.bind_account_id.as_deref() {
        let _ = runtime.block_on(account::prepare_account_for_injection(account_id))?;
        inject_account_to_profile(target, Path::new(&profile.user_data_dir), account_id)?;
    }
    let extra_args = parse_extra_args(&profile.extra_args);
    let pid = start_runtime_with_args(target, &profile.user_data_dir, &extra_args)?;
    let updated = match target {
        RuntimeTarget::Legacy => {
            antigravity_legacy_instance::update_instance_after_start(&profile.id, pid)?
        }
        RuntimeTarget::Ide => instance::update_instance_after_start(&profile.id, pid)?,
    };
    to_value(InstanceProfileView::from_profile(
        updated.clone(),
        process::is_pid_running(pid),
        is_profile_initialized(target, &updated.user_data_dir),
    ))
}

fn stop_instance(target: RuntimeTarget, payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = default_user_data_dir(target)?;
        let default_settings = load_default_settings(target)?;
        if let Some(pid) = resolve_pid(target, default_settings.last_pid, None) {
            process::close_pid(pid, 20)?;
        }
        let _ = update_default_pid(target, None);
        return to_value(InstanceProfileView {
            id: DEFAULT_INSTANCE_ID.to_string(),
            name: String::new(),
            user_data_dir: default_dir.to_string_lossy().to_string(),
            working_dir: None,
            extra_args: default_settings.extra_args.clone(),
            bind_account_id: resolve_default_account_id(target, &default_settings),
            created_at: 0,
            last_launched_at: None,
            last_pid: None,
            running: false,
            initialized: match target {
                RuntimeTarget::Legacy => {
                    antigravity_legacy_instance::is_profile_initialized(&default_dir)
                }
                RuntimeTarget::Ide => instance::is_profile_initialized(&default_dir),
            },
            is_default: true,
            follow_local_account: default_settings.follow_local_account,
        });
    }

    let store = load_instance_store(target)?;
    let profile = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;
    if let Some(pid) = resolve_pid(target, profile.last_pid, Some(&profile.user_data_dir)) {
        process::close_pid(pid, 20)?;
    }
    let updated = update_instance_pid(target, &profile.id, None)?;
    let initialized = is_profile_initialized(target, &updated.user_data_dir);
    to_value(InstanceProfileView::from_profile(
        updated,
        false,
        initialized,
    ))
}

fn close_all_instances(target: RuntimeTarget) -> Result<Value, String> {
    let store = load_instance_store(target)?;
    let default_dir = default_user_data_dir(target)?;
    let mut target_dirs = vec![default_dir.to_string_lossy().to_string()];
    for profile in &store.instances {
        let dir = profile.user_data_dir.trim();
        if !dir.is_empty() {
            target_dirs.push(dir.to_string());
        }
    }
    close_runtime_dirs(target, &target_dirs)?;
    let _ = clear_all_pids(target);
    Ok(Value::Null)
}

fn open_instance_window(target: RuntimeTarget, payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_settings = load_default_settings(target)?;
        focus_instance(target, default_settings.last_pid, None)?;
        return Ok(Value::Null);
    }
    let store = load_instance_store(target)?;
    let profile = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;
    focus_instance(target, profile.last_pid, Some(&profile.user_data_dir))?;
    Ok(Value::Null)
}

fn switch_account_restart(
    target: RuntimeTarget,
    account_id: String,
    runtime: &Runtime,
) -> Result<Account, String> {
    logger::log_info(&format!(
        "[AntigravityAdapter] 开始切换 {} 账号: {}",
        runtime_label(target),
        account_id
    ));
    ensure_launch_path(target)?;
    let mut account = runtime.block_on(account::prepare_account_for_injection(&account_id))?;
    account::set_current_account_id(&account_id)?;
    account.update_last_used();
    account::save_account(&account)?;
    if let Err(error) =
        update_default_settings(target, Some(Some(account_id.clone())), None, Some(false))
    {
        logger::log_warn(&format!(
            "[AntigravityAdapter] 更新默认实例绑定账号失败: {}",
            error
        ));
    }
    let default_dir = default_user_data_dir(target)?;
    let default_dir_str = default_dir.to_string_lossy().to_string();
    close_runtime_dirs(target, &[default_dir_str.clone()])?;
    let _ = update_default_pid(target, None);
    inject_account_to_profile(target, &default_dir, &account_id)?;
    let default_settings = load_default_settings(target)?;
    let extra_args = parse_extra_args(&default_settings.extra_args);
    match start_runtime_with_args(target, "", &extra_args) {
        Ok(pid) => {
            let _ = update_default_pid(target, Some(pid));
        }
        Err(error) if path_missing_error(&error) => return Err(error),
        Err(error) => {
            return Err(format!(
                "账号已切换，但启动 {} 失败: {}",
                runtime_label(target),
                error
            ));
        }
    }
    Ok(account)
}

fn switch_account(payload: Value, runtime: &Runtime) -> Result<Value, String> {
    let target = runtime_target();
    let payload: AccountIdPayload = parse_payload(payload)?;
    if target == RuntimeTarget::Ide
        && modules::config::get_user_config().antigravity_dual_switch_no_restart_enabled
    {
        return to_value(runtime.block_on(account::switch_account_dual_no_restart(
            &payload.account_id,
            "manual",
            "tools.account.switch",
            "dual_no_restart",
            None,
        ))?);
    }
    to_value(switch_account_restart(target, payload.account_id, runtime)?)
}

fn export_accounts(payload: Value) -> Result<Value, String> {
    let payload: AccountIdsPayload = parse_payload(payload)?;
    let mut accounts_to_export = Vec::new();
    if payload.account_ids.is_empty() {
        accounts_to_export = account::list_accounts()?;
    } else {
        for id in payload.account_ids {
            if let Ok(account) = account::load_account(&id) {
                accounts_to_export.push(account);
            }
        }
    }
    let simplified: Vec<SimpleAccount> = accounts_to_export
        .into_iter()
        .map(|account| SimpleAccount {
            email: account.email,
            refresh_token: account.token.refresh_token,
            tags: account.tags,
            notes: account.notes,
        })
        .collect();
    serde_json::to_string_pretty(&simplified)
        .map(Value::String)
        .map_err(|error| format!("序列化失败: {}", error))
}

fn load_account_groups() -> Result<Value, String> {
    let path = account::get_data_dir()?.join("account_groups.json");
    if !path.exists() {
        return Ok(Value::String("[]".to_string()));
    }
    std::fs::read_to_string(path)
        .map(Value::String)
        .map_err(|error| format!("Failed to read groups: {}", error))
}

fn save_account_groups(payload: Value) -> Result<Value, String> {
    let payload: GroupDataPayload = parse_payload(payload)?;
    let dir = account::get_data_dir()?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("Failed to create dir: {}", error))?;
    }
    let path = dir.join("account_groups.json");
    std::fs::write(path, payload.data)
        .map_err(|error| format!("Failed to write groups: {}", error))?;
    Ok(Value::Null)
}

fn refresh_account_quota(payload: Value, runtime: &Runtime) -> Result<Value, String> {
    let payload: AccountIdPayload = parse_payload(payload)?;
    let mut target_account = account::load_account(&payload.account_id)?;
    let quota = runtime
        .block_on(account::fetch_quota_with_fresh_token(
            &mut target_account,
            true,
        ))
        .map_err(|error| error.to_string())?;
    account::update_account_quota(&payload.account_id, quota)?;
    to_value(account::load_account(&payload.account_id)?)
}

fn refresh_all_quotas(runtime: &Runtime) -> Result<Value, String> {
    let result = runtime.block_on(account::refresh_all_quotas_logic(
        account::QuotaRefreshTrigger::ManualBatch,
    ))?;
    if let Err(error) = account::run_quota_alert_if_needed() {
        logger::log_warn(&format!("[AntigravityAdapter] 配额预警检查失败: {}", error));
    }
    to_value(result)
}

async fn ensure_oauth_flow_prepared() -> Result<String, String> {
    if let Ok(state) = get_oauth_flow_state().lock() {
        if let Some(s) = state.as_ref() {
            return Ok(s.auth_url.clone());
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("无法绑定本地端口: {}", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("无法获取本地端口: {}", e))?
        .port();

    let redirect_uri = format!("http://localhost:{}/oauth-callback", port);
    let state_token = Uuid::new_v4().to_string();
    let auth_url = modules::oauth::get_auth_url(&redirect_uri, Some(&state_token));
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let (code_tx, code_rx) = oneshot::channel::<Result<String, String>>();
    let code_tx = Arc::new(tokio::sync::Mutex::new(Some(code_tx)));

    let tx = code_tx.clone();
    let mut rx = cancel_rx;
    let expected_state = state_token.clone();
    tokio::spawn(async move {
        loop {
            let accept_result = tokio::select! {
                res = listener.accept() => Some(res),
                _ = rx.changed() => None,
            };

            let Some(accept_result) = accept_result else {
                break;
            };
            let Ok((mut stream, _)) = accept_result else {
                continue;
            };

            let result = process_callback_request(&mut stream, port, &expected_state).await;
            if let Some(result) = result {
                if let Some(sender) = tx.lock().await.take() {
                    let _ = emit_host_event("oauth-callback-received", Value::Null);
                    let _ = sender.send(result);
                }
                break;
            }
        }
    });

    if let Ok(mut state) = get_oauth_flow_state().lock() {
        *state = Some(OAuthFlowState {
            auth_url: auth_url.clone(),
            redirect_uri,
            expected_state: state_token,
            cancel_tx,
            code_tx: code_tx.clone(),
            code_rx: Some(code_rx),
        });
    }

    let _ = emit_host_event("oauth-url-generated", Value::String(auth_url.clone()));
    Ok(auth_url)
}

fn cancel_oauth_flow() {
    if let Ok(mut state) = get_oauth_flow_state().lock() {
        if let Some(s) = state.take() {
            let _ = s.cancel_tx.send(true);
        }
    }
}

fn submit_oauth_callback_url(payload: Value, runtime: &Runtime) -> Result<Value, String> {
    let payload: OAuthCallbackPayload = parse_payload(payload)?;
    let (redirect_uri, expected_state, code_tx) = {
        let lock = get_oauth_flow_state()
            .lock()
            .map_err(|_| "OAuth 状态锁被污染".to_string())?;
        let state = lock
            .as_ref()
            .ok_or_else(|| "OAuth 状态不存在，请先发起授权".to_string())?;
        (
            state.redirect_uri.clone(),
            state.expected_state.clone(),
            state.code_tx.clone(),
        )
    };

    let parsed = parse_manual_callback_url(&payload.callback_url, &redirect_uri)?;
    if parsed.path() != OAUTH_CALLBACK_PATH {
        return Err(format!("回调链接路径无效，必须为 {}", OAUTH_CALLBACK_PATH));
    }
    let code = extract_code_from_callback_url(&parsed, expected_state.as_str())?;
    let mut tx = runtime.block_on(code_tx.lock());
    let sender = tx
        .take()
        .ok_or_else(|| "OAuth 回调已处理，请勿重复提交".to_string())?;
    sender
        .send(Ok(code))
        .map_err(|_| "OAuth 回调发送失败，请重新发起授权".to_string())?;
    let _ = emit_host_event("oauth-callback-received", Value::Null);
    Ok(Value::Null)
}

fn save_oauth_token_response(
    token_res: modules::oauth::TokenResponse,
    runtime: &Runtime,
) -> Result<Account, String> {
    let refresh_token = token_res.refresh_token.ok_or_else(|| {
        "未获取到 Refresh Token。\n\n可能原因：您之前已授权过此应用\n\n解决方案：\n1. 访问 https://myaccount.google.com/permissions\n2. 撤销 'Antigravity Tools' 的访问权限\n3. 重新进行 OAuth 授权"
            .to_string()
    })?;
    let user_info = runtime.block_on(modules::oauth::get_user_info(&token_res.access_token))?;
    let token_data = TokenData::new(
        token_res.access_token,
        refresh_token,
        token_res.expires_in,
        Some(user_info.email.clone()),
        None,
        user_info.id.clone(),
    )
    .with_oauth_metadata(token_res.oauth_client_key, token_res.id_token);
    let account = account::upsert_account(
        user_info.email.clone(),
        user_info.get_display_name(),
        token_data,
    )?;
    let account_id = account.id.clone();
    let mut refreshing_account = account;
    match runtime.block_on(account::fetch_quota_with_fresh_token(
        &mut refreshing_account,
        true,
    )) {
        Ok(quota) => {
            if let Err(error) = account::update_account_quota(&account_id, quota) {
                logger::log_warn(&format!(
                    "[AntigravityAdapter] OAuth 登录后配额写回失败: account_id={}, error={}",
                    account_id, error
                ));
                return Ok(refreshing_account);
            }
            Ok(account::load_account(&account_id).unwrap_or(refreshing_account))
        }
        Err(error) => {
            logger::log_warn(&format!(
                "[AntigravityAdapter] OAuth 登录后配额刷新失败: account_id={}, error={}",
                account_id, error
            ));
            Ok(refreshing_account)
        }
    }
}

fn complete_oauth_login(runtime: &Runtime) -> Result<Value, String> {
    let _ = runtime.block_on(ensure_oauth_flow_prepared())?;
    let (code_rx, redirect_uri) = {
        let mut lock = get_oauth_flow_state()
            .lock()
            .map_err(|_| "OAuth 状态锁被污染".to_string())?;
        let Some(state) = lock.as_mut() else {
            return Err("OAuth 状态不存在".to_string());
        };
        let rx = state
            .code_rx
            .take()
            .ok_or_else(|| "OAuth 授权已在进行中".to_string())?;
        (rx, state.redirect_uri.clone())
    };

    let callback_result = runtime.block_on(timeout(OAUTH_FLOW_WAIT_TIMEOUT, code_rx));
    let code = match callback_result {
        Ok(Ok(Ok(code))) => code,
        Ok(Ok(Err(e))) => {
            clear_oauth_flow_state();
            return Err(e);
        }
        Ok(Err(_)) => {
            clear_oauth_flow_state();
            return Err("等待 OAuth 回调失败".to_string());
        }
        Err(_) => {
            cancel_oauth_flow();
            return Err("等待 OAuth 回调超时，请重试".to_string());
        }
    };

    clear_oauth_flow_state();
    let token_res = runtime.block_on(modules::oauth::exchange_code(&code, &redirect_uri))?;
    let account = save_oauth_token_response(token_res, runtime)?;
    logger::log_info(&format!(
        "[AntigravityAdapter] OAuth 账号已保存: {}",
        account.email
    ));
    to_value(account)
}

fn add_account_with_refresh_token(payload: Value, runtime: &Runtime) -> Result<Value, String> {
    let payload: RefreshTokenPayload = parse_payload(payload)?;
    let token_response =
        runtime.block_on(modules::oauth::refresh_access_token(&payload.refresh_token))?;
    let user_info =
        runtime.block_on(modules::oauth::get_user_info(&token_response.access_token))?;
    let token = TokenData::new(
        token_response.access_token,
        payload.refresh_token,
        token_response.expires_in,
        Some(user_info.email.clone()),
        None,
        None,
    )
    .with_oauth_metadata(token_response.oauth_client_key, token_response.id_token);
    let account =
        account::upsert_account(user_info.email.clone(), user_info.get_display_name(), token)?;
    logger::log_info(&format!(
        "[AntigravityAdapter] 添加账号成功: {}",
        account.email
    ));
    to_value(account)
}

fn refresh_current_quota(runtime: &Runtime) -> Result<Value, String> {
    let Some(mut current_account) = account::get_current_account()? else {
        return Err("未找到当前账号".to_string());
    };
    let account_id = current_account.id.clone();
    let quota = runtime
        .block_on(account::fetch_quota_with_fresh_token(
            &mut current_account,
            true,
        ))
        .map_err(|error| error.to_string())?;
    account::update_account_quota(&account_id, quota)?;

    let switched = match runtime.block_on(account::run_auto_switch_if_needed()) {
        Ok(Some(account)) => {
            logger::log_info(&format!(
                "[AntigravityAdapter][AutoSwitch] 当前账号刷新后自动切号完成: {}",
                account.email
            ));
            true
        }
        Ok(None) => false,
        Err(error) => {
            logger::log_warn(&format!(
                "[AntigravityAdapter][AutoSwitch] 当前账号刷新后自动切号失败: {}",
                error
            ));
            false
        }
    };

    if !switched {
        if let Err(error) = account::run_quota_alert_if_needed() {
            logger::log_warn(&format!(
                "[AntigravityAdapter][QuotaAlert] 当前账号刷新后预警检查失败: {}",
                error
            ));
        }
    }

    Ok(Value::Null)
}

fn restore_runtime() -> Result<Value, String> {
    let event_emitter = wakeup_scheduler_event_emitter();
    wakeup_scheduler::restore_state_from_disk();
    wakeup_scheduler::ensure_started_with_event_emitter(None, Some(event_emitter.clone()));
    wakeup_scheduler::trigger_startup_tasks_if_needed_with_event_emitter(None, Some(event_emitter));
    Ok(Value::Null)
}

fn handle_rpc(runtime: &Runtime, request: RpcRequest) -> Result<Value, String> {
    let target = runtime_target();
    match request.method.as_str() {
        "health.check" => Ok(json!({
            "status": "ok",
            "runtimeTarget": match target {
                RuntimeTarget::Legacy => "antigravity",
                RuntimeTarget::Ide => "antigravity_ide",
            }
        })),
        "adapter.shutdown" => Ok(Value::Null),
        "accounts.list" => to_value(account::list_accounts()?),
        "accounts.current" => to_value(account::get_current_account()?),
        "accounts.currentId" => to_value(account::get_current_account_id()?),
        "accounts.addRefreshToken" => add_account_with_refresh_token(request.payload, runtime),
        "accounts.setCurrent" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            account::set_current_account_id(&payload.account_id)?;
            Ok(Value::Null)
        }
        "accounts.delete" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            account::delete_account(&payload.account_id)?;
            Ok(Value::Null)
        }
        "accounts.deleteMany" => {
            let payload: AccountIdsPayload = parse_payload(request.payload)?;
            account::delete_accounts(&payload.account_ids)?;
            Ok(Value::Null)
        }
        "accounts.reorder" => {
            let payload: AccountIdsPayload = parse_payload(request.payload)?;
            account::reorder_accounts(&payload.account_ids)?;
            Ok(Value::Null)
        }
        "accounts.saveSnapshot" => {
            let payload: SaveAccountPayload = parse_payload(request.payload)?;
            account::save_account(&payload.account)?;
            to_value(payload.account)
        }
        "accounts.importOldTools" => {
            to_value(runtime.block_on(modules::import::import_from_old_tools_logic())?)
        }
        "accounts.importLocal" => {
            to_value(runtime.block_on(modules::import::import_from_local_logic())?)
        }
        "accounts.syncFromExtension" => {
            to_value(runtime.block_on(modules::import::import_from_extension_credentials(None))?)
        }
        "accounts.importJson" => {
            let payload: JsonImportPayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(modules::import::import_from_json_logic(
                payload.json_content,
            ))?)
        }
        "accounts.importFiles" => {
            let payload: FileImportPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(modules::import::import_from_files_logic(payload.file_paths))?,
            )
        }
        "accounts.export" => export_accounts(request.payload),
        "accounts.refresh" => refresh_account_quota(request.payload, runtime),
        "accounts.refreshAll" => refresh_all_quotas(runtime),
        "accounts.updateTags" => {
            let payload: TagsPayload = parse_payload(request.payload)?;
            to_value(account::update_account_tags(
                &payload.account_id,
                payload.tags,
            )?)
        }
        "accounts.updateNotes" => {
            let payload: NotesPayload = parse_payload(request.payload)?;
            to_value(account::update_account_notes(
                &payload.account_id,
                payload.notes,
            )?)
        }
        "accounts.groups.load" => load_account_groups(),
        "accounts.groups.save" => save_account_groups(request.payload),
        "accounts.syncCurrentFromClient" => Ok(Value::Null),
        "accounts.refreshCurrent" => refresh_current_quota(runtime),
        "quota.alertPayload" => to_value(account::run_quota_alert_if_needed()?),
        "switch.inject" => switch_account(request.payload, runtime),
        "switch.history.load" => to_value(antigravity_switch_history::load_history()?),
        "switch.history.clear" => {
            antigravity_switch_history::clear_history()?;
            Ok(Value::Null)
        }
        "oauth.prepareUrl" => to_value(runtime.block_on(ensure_oauth_flow_prepared())?),
        "oauth.complete" => complete_oauth_login(runtime),
        "oauth.cancel" => {
            cancel_oauth_flow();
            Ok(Value::Null)
        }
        "oauth.submitCallbackUrl" => submit_oauth_callback_url(request.payload, runtime),
        "wakeup.fetchAvailableModels" => {
            to_value(runtime.block_on(wakeup::fetch_available_models())?)
        }
        "wakeup.trigger" => {
            let payload: WakeupTriggerPayload = parse_payload(request.payload)?;
            let final_prompt = payload.prompt.unwrap_or_else(|| "hi".to_string());
            let final_tokens = payload.max_output_tokens.unwrap_or(0);
            wakeup::set_official_ls_version_mode(payload.official_ls_version_mode.as_deref())?;
            to_value(runtime.block_on(wakeup::trigger_wakeup(
                &payload.account_id,
                &payload.model,
                &final_prompt,
                final_tokens,
                payload.cancel_scope_id.as_deref(),
            ))?)
        }
        "wakeup.scheduler.syncState" => {
            let payload: WakeupSyncStatePayload = parse_payload(request.payload)?;
            wakeup::set_official_ls_version_mode(payload.official_ls_version_mode.as_deref())?;
            let event_emitter = wakeup_scheduler_event_emitter();
            wakeup_scheduler::sync_state(payload.enabled, payload.tasks);
            wakeup_scheduler::ensure_started_with_event_emitter(None, Some(event_emitter.clone()));
            if payload.run_startup_tasks.unwrap_or(false) {
                wakeup_scheduler::trigger_startup_tasks_if_needed_with_event_emitter(
                    None,
                    Some(event_emitter),
                );
            }
            Ok(Value::Null)
        }
        "wakeup.scheduler.runEnabledTasks" => {
            let payload: WakeupRunEnabledTasksPayload = parse_payload(request.payload)?;
            wakeup::set_official_ls_version_mode(payload.official_ls_version_mode.as_deref())?;
            let source = payload
                .trigger_source
                .unwrap_or_else(|| "startup".to_string());
            let event_emitter = wakeup_scheduler_event_emitter();
            let started =
                runtime.block_on(wakeup_scheduler::run_enabled_tasks_now_with_event_emitter(
                    None,
                    Some(&event_emitter),
                    &source,
                ));
            to_value(started as u32)
        }
        "wakeup.scheduler.confirmTask" => {
            let payload: WakeupTaskIdPayload = parse_payload(request.payload)?;
            let event_emitter = wakeup_scheduler_event_emitter();
            runtime.block_on(
                wakeup_scheduler::execute_pending_confirmation_with_event_emitter(
                    None,
                    Some(&event_emitter),
                    &payload.task_id,
                ),
            )?;
            Ok(Value::Null)
        }
        "wakeup.scheduler.cancelTask" => {
            let payload: WakeupTaskIdPayload = parse_payload(request.payload)?;
            wakeup_scheduler::cancel_pending_confirmation(&payload.task_id)?;
            Ok(Value::Null)
        }
        "wakeup.scheduler.checkTimeouts" => {
            let event_emitter = wakeup_scheduler_event_emitter();
            runtime.block_on(
                wakeup_scheduler::check_and_handle_timeouts_with_event_emitter(
                    None,
                    Some(&event_emitter),
                ),
            )?;
            Ok(Value::Null)
        }
        "wakeup.runtime.ensureReady" => {
            let payload: OfficialLsVersionModePayload = parse_payload(request.payload)?;
            wakeup::set_official_ls_version_mode(payload.official_ls_version_mode.as_deref())?;
            to_value(wakeup::ensure_wakeup_runtime_ready()?)
        }
        "wakeup.crontab.validate" => {
            let payload: CrontabPayload = parse_payload(request.payload)?;
            wakeup_scheduler::validate_crontab_expression(&payload.expr)?;
            Ok(Value::Null)
        }
        "wakeup.sharedHistory.load" => to_value(wakeup_history::load_history()?),
        "wakeup.sharedHistory.add" => {
            let payload: WakeupHistoryItemsPayload = parse_payload(request.payload)?;
            wakeup_history::add_history_items(payload.items)?;
            Ok(Value::Null)
        }
        "wakeup.sharedHistory.clear" => {
            wakeup_history::clear_history()?;
            Ok(Value::Null)
        }
        "wakeup.sharedScope.cancel" => {
            let payload: CancelScopePayload = parse_payload(request.payload)?;
            wakeup::cancel_wakeup_scope(&payload.cancel_scope_id)?;
            Ok(Value::Null)
        }
        "wakeup.sharedScope.release" => {
            let payload: CancelScopePayload = parse_payload(request.payload)?;
            wakeup::release_wakeup_scope(&payload.cancel_scope_id)?;
            Ok(Value::Null)
        }
        "wakeup.setOfficialLsVersionMode" => {
            let payload: OfficialLsVersionModePayload = parse_payload(request.payload)?;
            wakeup::set_official_ls_version_mode(payload.official_ls_version_mode.as_deref())?;
            Ok(Value::Null)
        }
        "wakeup.verification.loadState" => {
            to_value(wakeup_verification::build_display_state_for_all_accounts()?)
        }
        "wakeup.verification.loadHistory" => to_value(wakeup_verification::load_history()?),
        "wakeup.verification.deleteHistory" => {
            let payload: BatchIdsPayload = parse_payload(request.payload)?;
            to_value(wakeup_verification::delete_history(payload.batch_ids)?)
        }
        "wakeup.verification.runBatch" => {
            let payload: WakeupVerificationRunBatchPayload = parse_payload(request.payload)?;
            let final_prompt = payload.prompt.unwrap_or_else(|| "hi".to_string());
            let final_tokens = payload.max_output_tokens.unwrap_or(0);
            wakeup::set_official_ls_version_mode(payload.official_ls_version_mode.as_deref())?;
            let progress_emitter = wakeup_verification_progress_emitter();
            to_value(
                runtime.block_on(wakeup_verification::run_batch_with_progress_emitter(
                    None,
                    Some(&progress_emitter),
                    payload.account_ids,
                    &payload.model,
                    &final_prompt,
                    final_tokens,
                ))?,
            )
        }
        "instances.store.get" => to_value(load_instance_store(target)?),
        "instances.store.replace" => {
            let payload: InstanceStorePayload = parse_payload(request.payload)?;
            save_instance_store(target, &payload.store)?;
            Ok(Value::Null)
        }
        "instance.getDefaults" => match target {
            RuntimeTarget::Legacy => {
                to_value(antigravity_legacy_instance::get_instance_defaults()?)
            }
            RuntimeTarget::Ide => to_value(instance::get_instance_defaults()?),
        },
        "instance.list" => to_value(collect_instance_views(target)?),
        "instance.create" => create_instance(target, request.payload),
        "instance.update" => update_instance(target, request.payload),
        "instance.delete" => delete_instance(target, request.payload),
        "instance.start" => start_instance(target, request.payload, runtime),
        "instance.stop" => stop_instance(target, request.payload),
        "instance.closeAll" => close_all_instances(target),
        "instance.openWindow" => open_instance_window(target, request.payload),
        "runtime.detectLaunchPath" => {
            let payload: DetectLaunchPathPayload = parse_payload(request.payload)?;
            let app = match target {
                RuntimeTarget::Legacy => "antigravity_legacy",
                RuntimeTarget::Ide => "antigravity",
            };
            to_value(process::detect_and_save_app_path(
                app,
                payload.force.unwrap_or(false),
            ))
        }
        "runtime.status" => to_value(collect_instance_views(target)?),
        "runtime.startDefault" => start_instance(
            target,
            json!({ "instanceId": DEFAULT_INSTANCE_ID }),
            runtime,
        ),
        "runtime.stopDefault" => {
            stop_instance(target, json!({ "instanceId": DEFAULT_INSTANCE_ID }))
        }
        "runtime.restartDefault" => {
            let _ = stop_instance(target, json!({ "instanceId": DEFAULT_INSTANCE_ID }));
            start_instance(
                target,
                json!({ "instanceId": DEFAULT_INSTANCE_ID }),
                runtime,
            )
        }
        "runtime.focusDefault" => {
            open_instance_window(target, json!({ "instanceId": DEFAULT_INSTANCE_ID }))
        }
        "runtime.restore" => restore_runtime(),
        other => Err(format!("未知 Antigravity adapter 方法: {}", other)),
    }
}

fn success_response(data: Value) -> RpcResponse {
    RpcResponse {
        ok: true,
        data: Some(data),
        error: None,
    }
}

fn error_response(message: String) -> RpcResponse {
    RpcResponse {
        ok: false,
        data: None,
        error: Some(RpcError { message }),
    }
}

fn write_json_response(request: tiny_http::Request, status: u16, response: RpcResponse) {
    let body = serde_json::to_string(&response).unwrap_or_else(|error| {
        json!({
            "ok": false,
            "error": { "message": format!("序列化 Antigravity adapter HTTP 响应失败: {}", error) }
        })
        .to_string()
    });
    let _ = request.respond(
        Response::from_string(body)
            .with_status_code(StatusCode(status))
            .with_header(json_header()),
    );
}

fn is_authorized(request: &tiny_http::Request, token: &str) -> bool {
    request.headers().iter().any(|header| {
        header.field.equiv("Authorization") && header.value.as_str() == format!("Bearer {}", token)
    })
}

fn handle_http_request(
    runtime: &Runtime,
    shutdown: &AtomicBool,
    token: &str,
    mut request: tiny_http::Request,
) {
    if request.method() != &Method::Post || request.url() != "/rpc" {
        write_json_response(
            request,
            404,
            error_response("Antigravity adapter 路由不存在".to_string()),
        );
        return;
    }

    if !is_authorized(&request, token) {
        write_json_response(
            request,
            401,
            error_response("Antigravity adapter token 无效".to_string()),
        );
        return;
    }

    let mut body = String::new();
    if let Err(error) = request.as_reader().read_to_string(&mut body) {
        write_json_response(
            request,
            400,
            error_response(format!("读取 Antigravity adapter 请求失败: {}", error)),
        );
        return;
    }

    let rpc_request: RpcRequest = match serde_json::from_str(&body) {
        Ok(value) => value,
        Err(error) => {
            write_json_response(
                request,
                400,
                error_response(format!("解析 Antigravity adapter 请求失败: {}", error)),
            );
            return;
        }
    };

    let is_shutdown = rpc_request.method == "adapter.shutdown";
    match handle_rpc(runtime, rpc_request) {
        Ok(data) => {
            if is_shutdown {
                shutdown.store(true, Ordering::SeqCst);
            }
            write_json_response(request, 200, success_response(data));
        }
        Err(error) => write_json_response(request, 200, error_response(error)),
    }
}

fn main() {
    cockpit_core::modules::logger::init_logger();

    let runtime = Runtime::new().expect("create Antigravity adapter tokio runtime");
    let server = Server::http("127.0.0.1:0").expect("bind Antigravity adapter server");
    let address = server.server_addr().to_string();
    let port = address
        .rsplit(':')
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .expect("resolve Antigravity adapter port");
    let token = Uuid::new_v4().to_string();
    println!(
        "{}",
        json!({
            "ok": true,
            "protocol": "http-json-v1",
            "host": "127.0.0.1",
            "port": port,
            "token": token,
        })
    );

    let shutdown = Arc::new(AtomicBool::new(false));
    for request in server.incoming_requests() {
        handle_http_request(&runtime, &shutdown, &token, request);
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
    }
}
