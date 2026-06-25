use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use tauri::Emitter;
use tiny_http::{Header, Method, Response, Server, StatusCode};

use crate::modules::{app_data, logger, platform_package};

const ZED_PLATFORM_ID: &str = "zed";
const KIRO_PLATFORM_ID: &str = "kiro";
const GITHUB_COPILOT_PLATFORM_ID: &str = "github-copilot";
const WINDSURF_PLATFORM_ID: &str = "windsurf";
const CURSOR_PLATFORM_ID: &str = "cursor";
const GEMINI_PLATFORM_ID: &str = "gemini";
const TRAE_PLATFORM_ID: &str = "trae";
const QODER_PLATFORM_ID: &str = "qoder";
const CODEBUDDY_PLATFORM_ID: &str = "codebuddy";
const CODEBUDDY_CN_PLATFORM_ID: &str = "codebuddy_cn";
const WORKBUDDY_PLATFORM_ID: &str = "workbuddy";
const CLAUDE_MANAGER_PLATFORM_ID: &str = "claude_manager";
const CODEX_PLATFORM_ID: &str = "codex";
const ANTIGRAVITY_PLATFORM_ID: &str = "antigravity";
const ANTIGRAVITY_IDE_PLATFORM_ID: &str = "antigravity_ide";
const ADAPTER_BOOT_TIMEOUT: Duration = Duration::from_secs(10);
const ADAPTER_CALL_TIMEOUT: Duration = Duration::from_secs(180);
const ADAPTER_STOP_TIMEOUT: Duration = Duration::from_secs(3);
const HOST_EVENT_PATH: &str = "/platform-adapter-events";
const HOST_EVENT_BODY_LIMIT_BYTES: u64 = 1024 * 1024;
const DATA_DIR_ENV: &str = "COCKPIT_TOOLS_DATA_DIR";
const PROFILE_ENV: &str = "COCKPIT_TOOLS_PROFILE";

static PLATFORM_ADAPTERS: std::sync::LazyLock<Mutex<HashMap<String, AdapterProcess>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));
static HOST_EVENT_BRIDGE: std::sync::LazyLock<Mutex<Option<HostEventBridge>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Clone)]
struct HostEventBridge {
    url: String,
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostAdapterEvent {
    event: String,
    #[serde(default)]
    payload: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostEventResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug)]
struct AdapterProcess {
    package_dir: PathBuf,
    executable_path: PathBuf,
    executable_len: Option<u64>,
    executable_modified: Option<SystemTime>,
    child: Child,
    endpoint: AdapterEndpoint,
}

#[derive(Debug, Clone)]
struct AdapterEndpoint {
    url: String,
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdapterBootstrap {
    ok: bool,
    protocol: String,
    host: String,
    port: u16,
    token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdapterRequest {
    method: String,
    payload: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdapterError {
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdapterResponse {
    ok: bool,
    data: Option<Value>,
    error: Option<AdapterError>,
}

#[derive(Debug)]
enum AdapterRequestError {
    Transport(String),
    Protocol(String),
}

impl std::fmt::Display for AdapterRequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdapterRequestError::Transport(message) | AdapterRequestError::Protocol(message) => {
                formatter.write_str(message)
            }
        }
    }
}

fn hidden_command(path: &PathBuf) -> Command {
    let mut command = Command::new(path);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }

    command
}

fn json_response(status: u16, response: HostEventResponse) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(&response)
        .unwrap_or_else(|_| b"{\"ok\":false,\"error\":\"serialize response failed\"}".to_vec());
    let mut result = Response::from_data(body).with_status_code(StatusCode(status));
    if let Ok(header) = Header::from_bytes(
        b"Content-Type".as_slice(),
        b"application/json; charset=utf-8".as_slice(),
    ) {
        result = result.with_header(header);
    }
    result
}

fn event_request_authorized(request: &tiny_http::Request, token: &str) -> bool {
    let expected = format!("Bearer {}", token);
    request
        .headers()
        .iter()
        .any(|header| header.field.equiv("Authorization") && header.value.as_str() == expected)
}

fn handle_host_event_request(mut request: tiny_http::Request, token: &str) {
    if request.method() != &Method::Post || request.url() != HOST_EVENT_PATH {
        let _ = request.respond(json_response(
            404,
            HostEventResponse {
                ok: false,
                error: Some("platform adapter event route not found".to_string()),
            },
        ));
        return;
    }

    if !event_request_authorized(&request, token) {
        let _ = request.respond(json_response(
            401,
            HostEventResponse {
                ok: false,
                error: Some("platform adapter event token invalid".to_string()),
            },
        ));
        return;
    }

    let mut body = String::new();
    if let Err(error) = request
        .as_reader()
        .take(HOST_EVENT_BODY_LIMIT_BYTES)
        .read_to_string(&mut body)
    {
        let _ = request.respond(json_response(
            400,
            HostEventResponse {
                ok: false,
                error: Some(format!("read platform adapter event failed: {}", error)),
            },
        ));
        return;
    }

    let event_request: HostAdapterEvent = match serde_json::from_str(&body) {
        Ok(value) => value,
        Err(error) => {
            let _ = request.respond(json_response(
                400,
                HostEventResponse {
                    ok: false,
                    error: Some(format!("parse platform adapter event failed: {}", error)),
                },
            ));
            return;
        }
    };

    let event_name = event_request.event.trim();
    if event_name.is_empty() {
        let _ = request.respond(json_response(
            400,
            HostEventResponse {
                ok: false,
                error: Some("platform adapter event name is empty".to_string()),
            },
        ));
        return;
    }

    match crate::get_app_handle() {
        Some(app) => {
            if let Err(error) = app.emit(event_name, event_request.payload) {
                logger::log_warn(&format!(
                    "[PlatformAdapter] 转发 adapter 事件失败: event={}, error={}",
                    event_name, error
                ));
                let _ = request.respond(json_response(
                    500,
                    HostEventResponse {
                        ok: false,
                        error: Some(format!("emit platform adapter event failed: {}", error)),
                    },
                ));
                return;
            }
        }
        None => {
            logger::log_warn(&format!(
                "[PlatformAdapter] 转发 adapter 事件失败: AppHandle 不可用, event={}",
                event_name
            ));
            let _ = request.respond(json_response(
                503,
                HostEventResponse {
                    ok: false,
                    error: Some("host app handle not ready".to_string()),
                },
            ));
            return;
        }
    }

    let _ = request.respond(json_response(
        200,
        HostEventResponse {
            ok: true,
            error: None,
        },
    ));
}

fn ensure_host_event_bridge() -> Result<HostEventBridge, String> {
    let mut bridge = HOST_EVENT_BRIDGE
        .lock()
        .map_err(|_| "获取平台 adapter 事件桥锁失败".to_string())?;
    if let Some(existing) = bridge.clone() {
        return Ok(existing);
    }

    let server = Server::http("127.0.0.1:0")
        .map_err(|error| format!("启动平台 adapter 事件桥失败: {}", error))?;
    let address = server.server_addr().to_string();
    let token = uuid::Uuid::new_v4().to_string();
    let next = HostEventBridge {
        url: format!("http://{}{}", address, HOST_EVENT_PATH),
        token: token.clone(),
    };
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            handle_host_event_request(request, &token);
        }
    });
    logger::log_info(&format!(
        "[PlatformAdapter] adapter 事件桥已启动: endpoint={}",
        next.url
    ));
    *bridge = Some(next.clone());
    Ok(next)
}

fn child_is_running(child: &mut Child) -> bool {
    match child.try_wait() {
        Ok(Some(_)) => false,
        Ok(None) => true,
        Err(_) => false,
    }
}

fn stop_child(child: &mut Child) {
    if child_is_running(child) {
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn drain_adapter_reader<R>(platform_id: String, stream_name: &'static str, reader: BufReader<R>)
where
    R: Read + Send + 'static,
{
    for line in reader.lines() {
        match line {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let message = format!(
                    "[PlatformAdapter][{}][{}] {}",
                    platform_id, stream_name, line
                );
                if stream_name == "stderr" {
                    logger::log_warn(&message);
                } else {
                    logger::log_info(&message);
                }
            }
            Err(error) => {
                logger::log_warn(&format!(
                    "[PlatformAdapter] 读取 adapter {} 失败: platform={}, error={}",
                    stream_name, platform_id, error
                ));
                break;
            }
        }
    }
}

fn start_adapter_stderr_drain(platform_id: &str, child: &mut Child) {
    let Some(stderr) = child.stderr.take() else {
        return;
    };
    let platform_label = platform_id.to_string();
    std::thread::spawn(move || {
        drain_adapter_reader(platform_label, "stderr", BufReader::new(stderr));
    });
}

fn read_bootstrap_line(platform_id: &str, child: &mut Child) -> Result<String, String> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("平台 adapter stdout 未捕获: {}", platform_id))?;
    let platform_label = platform_id.to_string();
    let platform_id_for_thread = platform_label.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let result = reader.read_line(&mut line).map(|_| line).map_err(|error| {
            format!(
                "读取平台 adapter 启动信息失败: platform={}, error={}",
                platform_id_for_thread, error
            )
        });
        if sender.send(result).is_ok() {
            drain_adapter_reader(platform_id_for_thread, "stdout", reader);
        }
    });
    receiver
        .recv_timeout(ADAPTER_BOOT_TIMEOUT)
        .map_err(|_| format!("平台 adapter 启动超时: {}", platform_label))?
}

fn platform_adapter_log_file_prefix(platform_id: &str) -> String {
    logger::platform_log_file_prefix(platform_id)
}

fn adapter_profile_env_value() -> String {
    std::env::var(PROFILE_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(app_data::profile_name)
}

fn log_adapter_start_failure(platform_id: &str, error: &str) {
    logger::log_warn(&format!(
        "[PlatformAdapter] adapter 启动失败: platform={}, error={}",
        platform_id, error
    ));
}

fn spawn_adapter(
    platform_id: &str,
    installed: platform_package::InstalledPlatformAdapter,
) -> Result<AdapterProcess, String> {
    if installed.adapter.protocol != "http-json-v1" {
        return Err(format!(
            "平台 adapter 协议不支持: {}",
            installed.adapter.protocol
        ));
    }

    let host_event_bridge = ensure_host_event_bridge()?;
    let data_dir = app_data::resolve_data_dir()?;
    let profile = adapter_profile_env_value();
    let mut command = hidden_command(&installed.executable_path);
    command
        .current_dir(&installed.current_dir)
        .env(DATA_DIR_ENV, &data_dir)
        .env(PROFILE_ENV, &profile)
        .env("COCKPIT_PLATFORM_ID", platform_id)
        .env("COCKPIT_PLATFORM_PACKAGE_DIR", &installed.current_dir)
        .env(
            "COCKPIT_PLATFORM_LOG_FILE_PREFIX",
            platform_adapter_log_file_prefix(platform_id),
        )
        .env("COCKPIT_HOST_EVENT_URL", &host_event_bridge.url)
        .env("COCKPIT_HOST_EVENT_TOKEN", &host_event_bridge.token);

    let mut child = command.spawn().map_err(|error| {
        format!(
            "启动平台 adapter 失败: platform={}, path={}, error={}",
            platform_id,
            installed.executable_path.display(),
            error
        )
    })?;

    start_adapter_stderr_drain(platform_id, &mut child);

    let bootstrap_line = match read_bootstrap_line(platform_id, &mut child) {
        Ok(line) => line,
        Err(error) => {
            log_adapter_start_failure(platform_id, &error);
            stop_child(&mut child);
            return Err(error);
        }
    };
    let bootstrap: AdapterBootstrap =
        serde_json::from_str(bootstrap_line.trim()).map_err(|error| {
            stop_child(&mut child);
            format!("解析平台 adapter 启动信息失败: {}", error)
        })?;
    if !bootstrap.ok || bootstrap.protocol != installed.adapter.protocol {
        stop_child(&mut child);
        return Err("平台 adapter 启动握手失败".to_string());
    }
    if bootstrap.host != "127.0.0.1" || bootstrap.token.trim().is_empty() {
        stop_child(&mut child);
        return Err("平台 adapter 启动握手地址或 token 非法".to_string());
    }

    let endpoint = AdapterEndpoint {
        url: format!("http://{}:{}/rpc", bootstrap.host, bootstrap.port),
        token: bootstrap.token,
    };
    logger::log_info(&format!(
        "[PlatformAdapter] adapter 已启动: platform={}, pid={}, endpoint={}",
        platform_id,
        child.id(),
        endpoint.url
    ));

    Ok(AdapterProcess {
        package_dir: installed.current_dir,
        executable_len: adapter_executable_len(&installed.executable_path),
        executable_modified: adapter_executable_modified(&installed.executable_path),
        executable_path: installed.executable_path,
        child,
        endpoint,
    })
}

fn adapter_executable_len(path: &PathBuf) -> Option<u64> {
    std::fs::metadata(path).ok().map(|metadata| metadata.len())
}

fn adapter_executable_modified(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
}

fn adapter_process_matches(
    process: &AdapterProcess,
    installed: &platform_package::InstalledPlatformAdapter,
    executable_len: Option<u64>,
    executable_modified: Option<SystemTime>,
) -> bool {
    process.package_dir == installed.current_dir
        && process.executable_path == installed.executable_path
        && process.executable_len == executable_len
        && process.executable_modified == executable_modified
}

fn adapter_endpoint(platform_id: &str) -> Result<AdapterEndpoint, String> {
    let installed = installed_platform_adapter_with_repair(platform_id)?;
    let executable_len = adapter_executable_len(&installed.executable_path);
    let executable_modified = adapter_executable_modified(&installed.executable_path);

    let old_process = {
        let mut adapters = PLATFORM_ADAPTERS
            .lock()
            .map_err(|_| "获取平台 adapter 锁失败".to_string())?;

        if let Some(process) = adapters.get_mut(platform_id) {
            if adapter_process_matches(process, &installed, executable_len, executable_modified)
                && child_is_running(&mut process.child)
            {
                return Ok(process.endpoint.clone());
            }
        }

        adapters.remove(platform_id)
    };

    if let Some(mut old) = old_process {
        stop_child(&mut old.child);
    }

    let mut new_process = spawn_adapter(platform_id, installed.clone())?;
    let new_endpoint = new_process.endpoint.clone();
    let mut adapters = PLATFORM_ADAPTERS
        .lock()
        .map_err(|_| "获取平台 adapter 锁失败".to_string())?;

    if let Some(process) = adapters.get_mut(platform_id) {
        if adapter_process_matches(process, &installed, executable_len, executable_modified)
            && child_is_running(&mut process.child)
        {
            stop_child(&mut new_process.child);
            return Ok(process.endpoint.clone());
        }
        if let Some(mut old) = adapters.remove(platform_id) {
            stop_child(&mut old.child);
        }
    }

    adapters.insert(platform_id.to_string(), new_process);
    Ok(new_endpoint)
}

fn should_repair_installed_adapter(error: &str) -> bool {
    error.contains("平台包缺少 adapter 声明")
        || error.contains("平台包 adapter entry 不存在")
        || error.contains("平台包 manifest 与 runtime adapter 声明不一致")
}

fn installed_platform_adapter_with_repair(
    platform_id: &str,
) -> Result<platform_package::InstalledPlatformAdapter, String> {
    match platform_package::installed_platform_adapter(platform_id) {
        Ok(installed) => Ok(installed),
        Err(error) => {
            if !should_repair_installed_adapter(&error) {
                return Err(error);
            }
            let Some(app) = crate::get_app_handle() else {
                return Err(error);
            };
            logger::log_warn(&format!(
                "[PlatformAdapter] 已安装平台包 adapter 声明异常，尝试重新安装修复: platform={}, error={}",
                platform_id, error
            ));
            platform_package::install_platform_package(app, platform_id).map_err(
                |repair_error| {
                    format!(
                        "{}；自动修复平台包失败，请在平台管理中手动修复或重新安装: {}",
                        error, repair_error
                    )
                },
            )?;
            platform_package::installed_platform_adapter(platform_id).map_err(|retry_error| {
                format!(
                    "{}；自动修复平台包后仍无法加载 adapter: {}",
                    error, retry_error
                )
            })
        }
    }
}

fn post_adapter_request_on_blocking_thread(
    endpoint: &AdapterEndpoint,
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<Value, AdapterRequestError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|error| {
            AdapterRequestError::Protocol(format!("创建平台 adapter HTTP 客户端失败: {}", error))
        })?;
    let response = client
        .post(&endpoint.url)
        .bearer_auth(&endpoint.token)
        .json(&AdapterRequest {
            method: method.to_string(),
            payload,
        })
        .send()
        .map_err(|error| {
            AdapterRequestError::Transport(format!("调用平台 adapter 失败: {}", error))
        })?;
    if !response.status().is_success() {
        return Err(AdapterRequestError::Protocol(format!(
            "平台 adapter 返回 HTTP {}",
            response.status()
        )));
    }
    let response = response.json::<AdapterResponse>().map_err(|error| {
        AdapterRequestError::Protocol(format!("解析平台 adapter 响应失败: {}", error))
    })?;
    if response.ok {
        return Ok(response.data.unwrap_or(Value::Null));
    }
    Err(AdapterRequestError::Protocol(
        response
            .error
            .map(|error| error.message)
            .unwrap_or_else(|| "平台 adapter 调用失败".to_string()),
    ))
}

fn post_adapter_request(
    endpoint: &AdapterEndpoint,
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<Value, AdapterRequestError> {
    let endpoint = endpoint.clone();
    let method = method.to_string();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = post_adapter_request_on_blocking_thread(&endpoint, &method, payload, timeout);
        let _ = sender.send(result);
    });
    receiver
        .recv_timeout(timeout.saturating_add(Duration::from_secs(1)))
        .map_err(|_| AdapterRequestError::Transport("调用平台 adapter 超时".to_string()))?
}

fn call_platform_adapter_value(
    platform_id: &str,
    method: &str,
    payload: Value,
) -> Result<Value, String> {
    call_platform_adapter_value_with_timeout(platform_id, method, payload, ADAPTER_CALL_TIMEOUT)
}

fn call_platform_adapter_value_with_timeout(
    platform_id: &str,
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<Value, String> {
    platform_package::ensure_platform_package_installed(platform_id)?;
    let endpoint = adapter_endpoint(platform_id)?;
    match post_adapter_request(&endpoint, method, payload.clone(), timeout) {
        Ok(value) => Ok(value),
        Err(AdapterRequestError::Transport(error)) => {
            logger::log_warn(&format!(
                "[PlatformAdapter] adapter 请求失败，准备重启后重试: platform={}, method={}, error={}",
                platform_id, method, error
            ));
            stop_platform_adapter(platform_id);
            let endpoint = adapter_endpoint(platform_id)?;
            post_adapter_request(&endpoint, method, payload, timeout).map_err(|retry_error| {
                format!("{}；重启平台 adapter 后重试仍失败: {}", error, retry_error)
            })
        }
        Err(error) => Err(error.to_string()),
    }
}

fn existing_adapter_endpoint(platform_id: &str) -> Option<AdapterEndpoint> {
    let mut adapters = PLATFORM_ADAPTERS.lock().ok()?;
    let process = adapters.get_mut(platform_id)?;
    if child_is_running(&mut process.child) {
        Some(process.endpoint.clone())
    } else {
        None
    }
}

pub fn call_zed_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(ZED_PLATFORM_ID, method, payload)
}

pub fn call_kiro_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(KIRO_PLATFORM_ID, method, payload)
}

pub fn call_github_copilot_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(GITHUB_COPILOT_PLATFORM_ID, method, payload)
}

pub fn call_windsurf_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(WINDSURF_PLATFORM_ID, method, payload)
}

pub fn call_cursor_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(CURSOR_PLATFORM_ID, method, payload)
}

pub fn call_gemini_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(GEMINI_PLATFORM_ID, method, payload)
}

pub fn call_trae_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(TRAE_PLATFORM_ID, method, payload)
}

pub fn call_qoder_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(QODER_PLATFORM_ID, method, payload)
}

pub fn call_codebuddy_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(CODEBUDDY_PLATFORM_ID, method, payload)
}

pub fn call_codebuddy_cn_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(CODEBUDDY_CN_PLATFORM_ID, method, payload)
}

pub fn call_workbuddy_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(WORKBUDDY_PLATFORM_ID, method, payload)
}

pub fn call_claude_manager_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(CLAUDE_MANAGER_PLATFORM_ID, method, payload)
}

pub fn call_codex_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(CODEX_PLATFORM_ID, method, payload)
}

pub fn call_antigravity_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(ANTIGRAVITY_PLATFORM_ID, method, payload)
}

pub fn call_antigravity_ide_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(ANTIGRAVITY_IDE_PLATFORM_ID, method, payload)
}

fn default_antigravity_series_platform_id() -> &'static str {
    if platform_package::is_platform_package_installed(ANTIGRAVITY_IDE_PLATFORM_ID) {
        ANTIGRAVITY_IDE_PLATFORM_ID
    } else if platform_package::is_platform_package_installed(ANTIGRAVITY_PLATFORM_ID) {
        ANTIGRAVITY_PLATFORM_ID
    } else {
        ANTIGRAVITY_IDE_PLATFORM_ID
    }
}

pub fn call_antigravity_series_value(method: &str, payload: Value) -> Result<Value, String> {
    call_platform_adapter_value(default_antigravity_series_platform_id(), method, payload)
}

pub fn call_antigravity_series_value_with_timeout(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<Value, String> {
    call_platform_adapter_value_with_timeout(
        default_antigravity_series_platform_id(),
        method,
        payload,
        timeout,
    )
}

pub fn call_zed_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value =
        call_platform_adapter_value_with_timeout(ZED_PLATFORM_ID, method, payload, timeout)?;
    serde_json::from_value(value).map_err(|error| format!("解析 Zed adapter 数据失败: {}", error))
}

pub fn call_zed<T: DeserializeOwned>(method: &str, payload: Value) -> Result<T, String> {
    let value = call_zed_value(method, payload)?;
    serde_json::from_value(value).map_err(|error| format!("解析 Zed adapter 数据失败: {}", error))
}

pub fn call_kiro_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value =
        call_platform_adapter_value_with_timeout(KIRO_PLATFORM_ID, method, payload, timeout)?;
    serde_json::from_value(value).map_err(|error| format!("解析 Kiro adapter 数据失败: {}", error))
}

pub fn call_kiro<T: DeserializeOwned>(method: &str, payload: Value) -> Result<T, String> {
    let value = call_kiro_value(method, payload)?;
    serde_json::from_value(value).map_err(|error| format!("解析 Kiro adapter 数据失败: {}", error))
}

pub fn call_github_copilot_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value = call_platform_adapter_value_with_timeout(
        GITHUB_COPILOT_PLATFORM_ID,
        method,
        payload,
        timeout,
    )?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 GitHub Copilot adapter 数据失败: {}", error))
}

pub fn call_github_copilot<T: DeserializeOwned>(method: &str, payload: Value) -> Result<T, String> {
    let value = call_github_copilot_value(method, payload)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 GitHub Copilot adapter 数据失败: {}", error))
}

pub fn call_windsurf_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value =
        call_platform_adapter_value_with_timeout(WINDSURF_PLATFORM_ID, method, payload, timeout)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 Windsurf adapter 数据失败: {}", error))
}

pub fn call_windsurf<T: DeserializeOwned>(method: &str, payload: Value) -> Result<T, String> {
    let value = call_windsurf_value(method, payload)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 Windsurf adapter 数据失败: {}", error))
}

pub fn call_cursor_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value =
        call_platform_adapter_value_with_timeout(CURSOR_PLATFORM_ID, method, payload, timeout)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 Cursor adapter 数据失败: {}", error))
}

pub fn call_cursor<T: DeserializeOwned>(method: &str, payload: Value) -> Result<T, String> {
    let value = call_cursor_value(method, payload)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 Cursor adapter 数据失败: {}", error))
}

pub fn call_gemini_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value =
        call_platform_adapter_value_with_timeout(GEMINI_PLATFORM_ID, method, payload, timeout)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 Gemini adapter 数据失败: {}", error))
}

pub fn call_gemini<T: DeserializeOwned>(method: &str, payload: Value) -> Result<T, String> {
    let value = call_gemini_value(method, payload)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 Gemini adapter 数据失败: {}", error))
}

pub fn call_trae_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value =
        call_platform_adapter_value_with_timeout(TRAE_PLATFORM_ID, method, payload, timeout)?;
    serde_json::from_value(value).map_err(|error| format!("解析 Trae adapter 数据失败: {}", error))
}

pub fn call_trae<T: DeserializeOwned>(method: &str, payload: Value) -> Result<T, String> {
    let value = call_trae_value(method, payload)?;
    serde_json::from_value(value).map_err(|error| format!("解析 Trae adapter 数据失败: {}", error))
}

pub fn call_qoder_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value =
        call_platform_adapter_value_with_timeout(QODER_PLATFORM_ID, method, payload, timeout)?;
    serde_json::from_value(value).map_err(|error| format!("解析 Qoder adapter 数据失败: {}", error))
}

pub fn call_qoder<T: DeserializeOwned>(method: &str, payload: Value) -> Result<T, String> {
    let value = call_qoder_value(method, payload)?;
    serde_json::from_value(value).map_err(|error| format!("解析 Qoder adapter 数据失败: {}", error))
}

pub fn call_codebuddy_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value =
        call_platform_adapter_value_with_timeout(CODEBUDDY_PLATFORM_ID, method, payload, timeout)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 CodeBuddy adapter 数据失败: {}", error))
}

pub fn call_codebuddy<T: DeserializeOwned>(method: &str, payload: Value) -> Result<T, String> {
    let value = call_codebuddy_value(method, payload)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 CodeBuddy adapter 数据失败: {}", error))
}

pub fn call_codebuddy_cn_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value = call_platform_adapter_value_with_timeout(
        CODEBUDDY_CN_PLATFORM_ID,
        method,
        payload,
        timeout,
    )?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 CodeBuddy CN adapter 数据失败: {}", error))
}

pub fn call_codebuddy_cn<T: DeserializeOwned>(method: &str, payload: Value) -> Result<T, String> {
    let value = call_codebuddy_cn_value(method, payload)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 CodeBuddy CN adapter 数据失败: {}", error))
}

pub fn call_workbuddy_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value =
        call_platform_adapter_value_with_timeout(WORKBUDDY_PLATFORM_ID, method, payload, timeout)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 WorkBuddy adapter 数据失败: {}", error))
}

pub fn call_workbuddy<T: DeserializeOwned>(method: &str, payload: Value) -> Result<T, String> {
    let value = call_workbuddy_value(method, payload)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 WorkBuddy adapter 数据失败: {}", error))
}

pub fn call_claude_manager_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value = call_platform_adapter_value_with_timeout(
        CLAUDE_MANAGER_PLATFORM_ID,
        method,
        payload,
        timeout,
    )?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 Claude adapter 数据失败: {}", error))
}

pub fn call_claude_manager<T: DeserializeOwned>(method: &str, payload: Value) -> Result<T, String> {
    let value = call_claude_manager_value(method, payload)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 Claude adapter 数据失败: {}", error))
}

pub fn call_codex_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value =
        call_platform_adapter_value_with_timeout(CODEX_PLATFORM_ID, method, payload, timeout)?;
    serde_json::from_value(value).map_err(|error| format!("解析 Codex adapter 数据失败: {}", error))
}

pub fn call_codex<T: DeserializeOwned>(method: &str, payload: Value) -> Result<T, String> {
    let value = call_codex_value(method, payload)?;
    serde_json::from_value(value).map_err(|error| format!("解析 Codex adapter 数据失败: {}", error))
}

pub fn call_antigravity_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value = call_platform_adapter_value_with_timeout(
        ANTIGRAVITY_PLATFORM_ID,
        method,
        payload,
        timeout,
    )?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 Antigravity adapter 数据失败: {}", error))
}

pub fn call_antigravity<T: DeserializeOwned>(method: &str, payload: Value) -> Result<T, String> {
    let value = call_antigravity_value(method, payload)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 Antigravity adapter 数据失败: {}", error))
}

pub fn call_antigravity_ide_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value = call_platform_adapter_value_with_timeout(
        ANTIGRAVITY_IDE_PLATFORM_ID,
        method,
        payload,
        timeout,
    )?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 Antigravity IDE adapter 数据失败: {}", error))
}

pub fn call_antigravity_ide<T: DeserializeOwned>(
    method: &str,
    payload: Value,
) -> Result<T, String> {
    let value = call_antigravity_ide_value(method, payload)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 Antigravity IDE adapter 数据失败: {}", error))
}

pub fn call_antigravity_series<T: DeserializeOwned>(
    method: &str,
    payload: Value,
) -> Result<T, String> {
    let value = call_antigravity_series_value(method, payload)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 Antigravity 系列 adapter 数据失败: {}", error))
}

pub fn call_antigravity_series_with_timeout<T: DeserializeOwned>(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String> {
    let value = call_antigravity_series_value_with_timeout(method, payload, timeout)?;
    serde_json::from_value(value)
        .map_err(|error| format!("解析 Antigravity 系列 adapter 数据失败: {}", error))
}

pub fn restore_codex_runtime() {
    if !platform_package::is_platform_package_installed(CODEX_PLATFORM_ID) {
        return;
    }
    if let Err(error) = call_codex_value("runtime.restore", json!({})) {
        logger::log_warn(&format!(
            "[PlatformAdapter] 恢复 Codex adapter runtime 失败: {}",
            error
        ));
    }
}

pub fn shutdown_codex_runtime_for_app_exit() {
    if !platform_package::is_platform_package_installed(CODEX_PLATFORM_ID) {
        return;
    }
    if let Err(error) = call_codex_value("runtime.shutdownForAppExit", json!({})) {
        logger::log_warn(&format!(
            "[PlatformAdapter] 退出前清理 Codex adapter runtime 失败: {}",
            error
        ));
    }
}

pub fn restore_zed_runtime() {
    if !platform_package::is_platform_package_installed(ZED_PLATFORM_ID) {
        return;
    }
    if let Err(error) = call_zed_value("oauth.restorePendingListener", json!({})) {
        logger::log_warn(&format!(
            "[PlatformAdapter] 恢复 Zed OAuth adapter 状态失败: {}",
            error
        ));
    }
}

pub fn restore_kiro_runtime() {
    if !platform_package::is_platform_package_installed(KIRO_PLATFORM_ID) {
        return;
    }
    if let Err(error) = call_kiro_value("oauth.restorePendingListener", json!({})) {
        logger::log_warn(&format!(
            "[PlatformAdapter] 恢复 Kiro OAuth adapter 状态失败: {}",
            error
        ));
    }
}

pub fn restore_github_copilot_runtime() {
    if !platform_package::is_platform_package_installed(GITHUB_COPILOT_PLATFORM_ID) {
        return;
    }
    if let Err(error) = call_github_copilot_value("oauth.restorePendingListener", json!({})) {
        logger::log_warn(&format!(
            "[PlatformAdapter] 恢复 GitHub Copilot adapter 状态失败: {}",
            error
        ));
    }
}

pub fn restore_windsurf_runtime() {
    if !platform_package::is_platform_package_installed(WINDSURF_PLATFORM_ID) {
        return;
    }
    if let Err(error) = call_windsurf_value("oauth.restorePendingListener", json!({})) {
        logger::log_warn(&format!(
            "[PlatformAdapter] 恢复 Windsurf adapter 状态失败: {}",
            error
        ));
    }
}

pub fn restore_cursor_runtime() {
    if !platform_package::is_platform_package_installed(CURSOR_PLATFORM_ID) {
        return;
    }
}

pub fn restore_gemini_runtime() {
    if !platform_package::is_platform_package_installed(GEMINI_PLATFORM_ID) {
        return;
    }
    if let Err(error) = call_gemini_value("oauth.restorePendingListener", json!({})) {
        logger::log_warn(&format!(
            "[PlatformAdapter] 恢复 Gemini adapter 状态失败: {}",
            error
        ));
    }
}

pub fn restore_trae_runtime() {
    if !platform_package::is_platform_package_installed(TRAE_PLATFORM_ID) {
        return;
    }
    if let Err(error) = call_trae_value("oauth.restorePendingListener", json!({})) {
        logger::log_warn(&format!(
            "[PlatformAdapter] 恢复 Trae adapter 状态失败: {}",
            error
        ));
    }
}

pub fn restore_qoder_runtime() {
    if !platform_package::is_platform_package_installed(QODER_PLATFORM_ID) {
        return;
    }
    if let Err(error) = call_qoder_value("oauth.restorePendingListener", json!({})) {
        logger::log_warn(&format!(
            "[PlatformAdapter] 恢复 Qoder adapter 状态失败: {}",
            error
        ));
    }
}

pub fn restore_codebuddy_runtime() {
    if !platform_package::is_platform_package_installed(CODEBUDDY_PLATFORM_ID) {
        return;
    }
    if let Err(error) = call_codebuddy_value("oauth.restorePendingListener", json!({})) {
        logger::log_warn(&format!(
            "[PlatformAdapter] 恢复 CodeBuddy adapter 状态失败: {}",
            error
        ));
    }
}

pub fn restore_codebuddy_cn_runtime() {
    if !platform_package::is_platform_package_installed(CODEBUDDY_CN_PLATFORM_ID) {
        return;
    }
    if let Err(error) = call_codebuddy_cn_value("oauth.restorePendingListener", json!({})) {
        logger::log_warn(&format!(
            "[PlatformAdapter] 恢复 CodeBuddy CN adapter 状态失败: {}",
            error
        ));
    }
}

pub fn restore_workbuddy_runtime() {
    if !platform_package::is_platform_package_installed(WORKBUDDY_PLATFORM_ID) {
        return;
    }
    if let Err(error) = call_workbuddy_value("oauth.restorePendingListener", json!({})) {
        logger::log_warn(&format!(
            "[PlatformAdapter] 恢复 WorkBuddy adapter 状态失败: {}",
            error
        ));
    }
}

pub fn restore_antigravity_runtime() {
    if !platform_package::is_platform_package_installed(ANTIGRAVITY_PLATFORM_ID) {
        return;
    }
    if let Err(error) = call_antigravity_value("runtime.restore", json!({})) {
        logger::log_warn(&format!(
            "[PlatformAdapter] 恢复 Antigravity adapter runtime 失败: {}",
            error
        ));
    }
}

pub fn restore_antigravity_ide_runtime() {
    if !platform_package::is_platform_package_installed(ANTIGRAVITY_IDE_PLATFORM_ID) {
        return;
    }
    if let Err(error) = call_antigravity_ide_value("runtime.restore", json!({})) {
        logger::log_warn(&format!(
            "[PlatformAdapter] 恢复 Antigravity IDE adapter runtime 失败: {}",
            error
        ));
    }
}

pub fn stop_platform_adapter(platform_id: &str) {
    let Some(mut process) = (match PLATFORM_ADAPTERS.lock() {
        Ok(mut adapters) => adapters.remove(platform_id),
        Err(_) => None,
    }) else {
        return;
    };

    let _ = post_adapter_request(
        &process.endpoint,
        "adapter.shutdown",
        json!({}),
        Duration::from_secs(2),
    );
    stop_child(&mut process.child);
    logger::log_info(&format!(
        "[PlatformAdapter] adapter 已停止: platform={}",
        platform_id
    ));
}

pub fn stop_zed_runtime_before_uninstall() {
    if let Some(endpoint) = existing_adapter_endpoint(ZED_PLATFORM_ID) {
        if let Err(error) = post_adapter_request(
            &endpoint,
            "oauth.cancel",
            json!({ "loginId": null }),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Zed 前取消 OAuth 状态失败，继续卸载: {}",
                error
            ));
        }
        if let Err(error) = post_adapter_request(
            &endpoint,
            "runtime.stopDefault",
            json!({}),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Zed 前停止运行态失败，继续卸载: {}",
                error
            ));
        }
    }
    stop_platform_adapter(ZED_PLATFORM_ID);
}

pub fn stop_kiro_runtime_before_uninstall() {
    if let Some(endpoint) = existing_adapter_endpoint(KIRO_PLATFORM_ID) {
        if let Err(error) = post_adapter_request(
            &endpoint,
            "oauth.cancel",
            json!({ "loginId": null }),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Kiro 前取消 OAuth 状态失败，继续卸载: {}",
                error
            ));
        }
        if let Err(error) = post_adapter_request(
            &endpoint,
            "runtime.stopDefault",
            json!({}),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Kiro 前停止运行态失败，继续卸载: {}",
                error
            ));
        }
    }
    stop_platform_adapter(KIRO_PLATFORM_ID);
}

pub fn stop_github_copilot_runtime_before_uninstall() {
    if let Some(endpoint) = existing_adapter_endpoint(GITHUB_COPILOT_PLATFORM_ID) {
        if let Err(error) = post_adapter_request(
            &endpoint,
            "oauth.cancel",
            json!({ "loginId": null }),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 GitHub Copilot 前取消 OAuth 状态失败，继续卸载: {}",
                error
            ));
        }
        if let Err(error) = post_adapter_request(
            &endpoint,
            "runtime.stopDefault",
            json!({}),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 GitHub Copilot 前停止运行态失败，继续卸载: {}",
                error
            ));
        }
    }
    stop_platform_adapter(GITHUB_COPILOT_PLATFORM_ID);
}

pub fn stop_windsurf_runtime_before_uninstall() {
    if let Some(endpoint) = existing_adapter_endpoint(WINDSURF_PLATFORM_ID) {
        if let Err(error) = post_adapter_request(
            &endpoint,
            "oauth.cancel",
            json!({ "loginId": null }),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Windsurf 前取消 OAuth 状态失败，继续卸载: {}",
                error
            ));
        }
        if let Err(error) = post_adapter_request(
            &endpoint,
            "runtime.stopDefault",
            json!({}),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Windsurf 前停止运行态失败，继续卸载: {}",
                error
            ));
        }
    }
    stop_platform_adapter(WINDSURF_PLATFORM_ID);
}

pub fn stop_cursor_runtime_before_uninstall() {
    if let Some(endpoint) = existing_adapter_endpoint(CURSOR_PLATFORM_ID) {
        if let Err(error) = post_adapter_request(
            &endpoint,
            "oauth.cancel",
            json!({ "loginId": null }),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Cursor 前取消 OAuth 状态失败，继续卸载: {}",
                error
            ));
        }
        if let Err(error) = post_adapter_request(
            &endpoint,
            "runtime.stopDefault",
            json!({}),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Cursor 前停止运行态失败，继续卸载: {}",
                error
            ));
        }
    }
    stop_platform_adapter(CURSOR_PLATFORM_ID);
}

pub fn stop_gemini_runtime_before_uninstall() {
    if let Some(endpoint) = existing_adapter_endpoint(GEMINI_PLATFORM_ID) {
        if let Err(error) = post_adapter_request(
            &endpoint,
            "oauth.cancel",
            json!({ "loginId": null }),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Gemini 前取消 OAuth 状态失败，继续卸载: {}",
                error
            ));
        }
        if let Err(error) = post_adapter_request(
            &endpoint,
            "runtime.stopDefault",
            json!({}),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Gemini 前停止运行态失败，继续卸载: {}",
                error
            ));
        }
    }
    stop_platform_adapter(GEMINI_PLATFORM_ID);
}

pub fn stop_trae_runtime_before_uninstall() {
    if let Some(endpoint) = existing_adapter_endpoint(TRAE_PLATFORM_ID) {
        if let Err(error) = post_adapter_request(
            &endpoint,
            "oauth.cancel",
            json!({ "loginId": null }),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Trae 前取消 OAuth 状态失败，继续卸载: {}",
                error
            ));
        }
        if let Err(error) = post_adapter_request(
            &endpoint,
            "runtime.stopDefault",
            json!({}),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Trae 前停止运行态失败，继续卸载: {}",
                error
            ));
        }
    }
    stop_platform_adapter(TRAE_PLATFORM_ID);
}

pub fn stop_qoder_runtime_before_uninstall() {
    if let Some(endpoint) = existing_adapter_endpoint(QODER_PLATFORM_ID) {
        if let Err(error) = post_adapter_request(
            &endpoint,
            "oauth.cancel",
            json!({ "loginId": null }),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Qoder 前取消 OAuth 状态失败，继续卸载: {}",
                error
            ));
        }
        if let Err(error) = post_adapter_request(
            &endpoint,
            "runtime.stopDefault",
            json!({}),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Qoder 前停止运行态失败，继续卸载: {}",
                error
            ));
        }
    }
    stop_platform_adapter(QODER_PLATFORM_ID);
}

pub fn stop_codebuddy_runtime_before_uninstall() {
    if let Some(endpoint) = existing_adapter_endpoint(CODEBUDDY_PLATFORM_ID) {
        if let Err(error) = post_adapter_request(
            &endpoint,
            "oauth.cancel",
            json!({ "loginId": null }),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 CodeBuddy 前取消 OAuth 状态失败，继续卸载: {}",
                error
            ));
        }
        if let Err(error) = post_adapter_request(
            &endpoint,
            "runtime.stopDefault",
            json!({}),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 CodeBuddy 前停止运行态失败，继续卸载: {}",
                error
            ));
        }
    }
    stop_platform_adapter(CODEBUDDY_PLATFORM_ID);
}

pub fn stop_codebuddy_cn_runtime_before_uninstall() {
    if let Some(endpoint) = existing_adapter_endpoint(CODEBUDDY_CN_PLATFORM_ID) {
        if let Err(error) = post_adapter_request(
            &endpoint,
            "oauth.cancel",
            json!({ "loginId": null }),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 CodeBuddy CN 前取消 OAuth 状态失败，继续卸载: {}",
                error
            ));
        }
        if let Err(error) = post_adapter_request(
            &endpoint,
            "runtime.stopDefault",
            json!({}),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 CodeBuddy CN 前停止运行态失败，继续卸载: {}",
                error
            ));
        }
    }
    stop_platform_adapter(CODEBUDDY_CN_PLATFORM_ID);
}

pub fn stop_workbuddy_runtime_before_uninstall() {
    if let Some(endpoint) = existing_adapter_endpoint(WORKBUDDY_PLATFORM_ID) {
        if let Err(error) = post_adapter_request(
            &endpoint,
            "oauth.cancel",
            json!({ "loginId": null }),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 WorkBuddy 前取消 OAuth 状态失败，继续卸载: {}",
                error
            ));
        }
        if let Err(error) = post_adapter_request(
            &endpoint,
            "runtime.stopDefault",
            json!({}),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 WorkBuddy 前停止运行态失败，继续卸载: {}",
                error
            ));
        }
    }
    stop_platform_adapter(WORKBUDDY_PLATFORM_ID);
}

pub fn stop_claude_manager_runtime_before_uninstall() {
    if let Some(endpoint) = existing_adapter_endpoint(CLAUDE_MANAGER_PLATFORM_ID) {
        if let Err(error) = post_adapter_request(
            &endpoint,
            "runtime.shutdownForAppExit",
            json!({}),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Claude 前清理运行态失败，继续卸载: {}",
                error
            ));
        }
    }
    stop_platform_adapter(CLAUDE_MANAGER_PLATFORM_ID);
}

pub fn stop_codex_runtime_before_uninstall() {
    if let Some(endpoint) = existing_adapter_endpoint(CODEX_PLATFORM_ID) {
        if let Err(error) = post_adapter_request(
            &endpoint,
            "runtime.shutdownForAppExit",
            json!({}),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 Codex 前清理运行态失败，继续卸载: {}",
                error
            ));
        }
    }
    stop_platform_adapter(CODEX_PLATFORM_ID);
}

fn stop_antigravity_series_runtime_before_uninstall(platform_id: &str, label: &str) {
    if let Some(endpoint) = existing_adapter_endpoint(platform_id) {
        if let Err(error) = post_adapter_request(
            &endpoint,
            "runtime.stopDefault",
            json!({}),
            ADAPTER_STOP_TIMEOUT,
        ) {
            logger::log_warn(&format!(
                "[PlatformAdapter] 卸载 {} 前停止运行态失败，继续卸载: {}",
                label, error
            ));
        }
    }
    stop_platform_adapter(platform_id);
}

pub fn stop_antigravity_runtime_before_uninstall() {
    stop_antigravity_series_runtime_before_uninstall(ANTIGRAVITY_PLATFORM_ID, "Antigravity");
}

pub fn stop_antigravity_ide_runtime_before_uninstall() {
    stop_antigravity_series_runtime_before_uninstall(
        ANTIGRAVITY_IDE_PLATFORM_ID,
        "Antigravity IDE",
    );
}
