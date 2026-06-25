use cockpit_core::models::gemini::{GeminiAccount, GeminiOAuthCompletePayload};
use cockpit_core::models::{DefaultInstanceSettings, InstanceProfileView, InstanceStore};
use cockpit_core::modules::{config, gemini_account, gemini_instance, gemini_oauth, process};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tiny_http::{Header, Method, Response, Server, StatusCode};
use tokio::runtime::Runtime;
use uuid::Uuid;

const DEFAULT_INSTANCE_ID: &str = "__default__";

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
struct AccountIdPayload {
    account_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountIdsPayload {
    account_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstanceStorePayload {
    store: InstanceStore,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonImportPayload {
    json_content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TagsPayload {
    account_id: String,
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenPayload {
    #[serde(alias = "access_token")]
    access_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginIdPayload {
    login_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginCancelPayload {
    login_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CallbackUrlPayload {
    login_id: String,
    callback_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateInstancePayload {
    name: String,
    user_data_dir: String,
    working_dir: Option<String>,
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
    working_dir: Option<String>,
    extra_args: Option<String>,
    bind_account_id: Option<Option<String>>,
    follow_local_account: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstanceIdPayload {
    instance_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecuteLaunchPayload {
    instance_id: String,
    terminal: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiInstanceLaunchInfo {
    instance_id: String,
    user_data_dir: String,
    launch_command: String,
}

struct GeminiLaunchContext {
    user_data_dir: String,
    working_dir: Option<String>,
    extra_args: String,
    use_home_env: bool,
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
        .map_err(|error| format!("序列化 Gemini adapter 响应失败: {}", error))
}

fn parse_payload<T: for<'de> Deserialize<'de>>(payload: Value) -> Result<T, String> {
    serde_json::from_value(payload)
        .map_err(|error| format!("解析 Gemini adapter 请求失败: {}", error))
}

fn sanitize_instance_store(store: &InstanceStore) -> InstanceStore {
    let mut next = store.clone();
    next.default_settings.last_pid = None;
    for instance in &mut next.instances {
        instance.last_pid = None;
        instance.last_launched_at = None;
    }
    next
}

fn is_profile_initialized(user_data_dir: &str) -> bool {
    gemini_instance::is_profile_initialized(Path::new(user_data_dir))
}

fn default_instance_view(
    default_dir: &Path,
    settings: DefaultInstanceSettings,
) -> InstanceProfileView {
    InstanceProfileView {
        id: DEFAULT_INSTANCE_ID.to_string(),
        name: String::new(),
        user_data_dir: default_dir.to_string_lossy().to_string(),
        working_dir: settings.working_dir,
        extra_args: settings.extra_args,
        bind_account_id: settings.bind_account_id,
        created_at: 0,
        last_launched_at: None,
        last_pid: None,
        running: false,
        initialized: gemini_instance::is_profile_initialized(default_dir),
        is_default: true,
        follow_local_account: false,
    }
}

async fn refresh_gemini_account_after_login(mut account: GeminiAccount) -> GeminiAccount {
    let account_id = account.id.clone();
    match gemini_account::refresh_account_token(&account_id).await {
        Ok(refreshed) => refreshed,
        Err(error) => {
            let _ = gemini_account::set_account_status(&account_id, Some("error"), Some(&error));
            account.status = Some("error".to_string());
            account.status_reason = Some(error);
            account
        }
    }
}

fn import_json(runtime: &Runtime, payload: Value) -> Result<Value, String> {
    let payload: JsonImportPayload = parse_payload(payload)?;
    let mut accounts = gemini_account::import_from_json(&payload.json_content)?;

    for account in accounts.iter_mut() {
        match runtime.block_on(gemini_account::refresh_account_token(&account.id)) {
            Ok(refreshed) => *account = refreshed,
            Err(error) => {
                let _ =
                    gemini_account::set_account_status(&account.id, Some("error"), Some(&error));
                account.status = Some("error".to_string());
                account.status_reason = Some(error);
            }
        }
    }

    to_value(accounts)
}

fn import_local(runtime: &Runtime) -> Result<Value, String> {
    let mut account = match gemini_account::import_from_local()? {
        Some(account) => account,
        None => return Err("未找到本地 Gemini 登录信息".to_string()),
    };

    match runtime.block_on(gemini_account::refresh_account_token(&account.id)) {
        Ok(refreshed) => account = refreshed,
        Err(error) => {
            let _ = gemini_account::set_account_status(&account.id, Some("error"), Some(&error));
            account.status = Some("error".to_string());
            account.status_reason = Some(error);
        }
    }

    to_value(vec![account])
}

fn add_with_token(runtime: &Runtime, payload: Value) -> Result<Value, String> {
    let payload: TokenPayload = parse_payload(payload)?;
    let account_payload = GeminiOAuthCompletePayload {
        email: "unknown@gmail.com".to_string(),
        auth_id: None,
        name: None,
        access_token: payload.access_token,
        refresh_token: None,
        id_token: None,
        token_type: None,
        scope: None,
        expiry_date: None,
        selected_auth_type: Some("oauth-personal".to_string()),
        project_id: None,
        tier_id: None,
        plan_name: None,
        gemini_auth_raw: None,
        gemini_usage_raw: None,
        status: None,
        status_reason: None,
    };
    let account = gemini_account::upsert_account(account_payload)?;
    to_value(runtime.block_on(refresh_gemini_account_after_login(account)))
}

fn complete_oauth(runtime: &Runtime, payload: Value) -> Result<Value, String> {
    let payload: LoginIdPayload = parse_payload(payload)?;
    let account_payload = runtime.block_on(gemini_oauth::complete_login(&payload.login_id))?;
    let account = gemini_account::upsert_account(account_payload)?;
    to_value(runtime.block_on(refresh_gemini_account_after_login(account)))
}

fn switch_inject(payload: Value) -> Result<Value, String> {
    let payload: AccountIdPayload = parse_payload(payload)?;
    let account = gemini_account::load_account(&payload.account_id)
        .ok_or_else(|| format!("Gemini account not found: {}", payload.account_id))?;
    gemini_account::inject_to_gemini(&payload.account_id)?;
    to_value(format!("切换完成: {}", account.email))
}

fn inject_default_profile(payload: Value) -> Result<Value, String> {
    let payload: AccountIdPayload = parse_payload(payload)?;
    gemini_account::inject_to_gemini_home(&payload.account_id, None)?;
    Ok(Value::Null)
}

fn list_instances() -> Result<Vec<InstanceProfileView>, String> {
    let store = gemini_instance::load_instance_store()?;
    let default_dir = gemini_instance::get_default_gemini_cli_home_root()?;
    let default_settings = store.default_settings.clone();

    let mut result: Vec<InstanceProfileView> = store
        .instances
        .into_iter()
        .map(|instance| {
            let initialized = is_profile_initialized(&instance.user_data_dir);
            InstanceProfileView::from_profile(instance, false, initialized)
        })
        .collect();

    result.push(default_instance_view(&default_dir, default_settings));
    Ok(result)
}

fn create_instance(payload: Value) -> Result<Value, String> {
    let payload: CreateInstancePayload = parse_payload(payload)?;
    let instance = gemini_instance::create_instance(gemini_instance::CreateInstanceParams {
        name: payload.name,
        user_data_dir: payload.user_data_dir,
        working_dir: payload.working_dir,
        extra_args: payload.extra_args.unwrap_or_default(),
        bind_account_id: payload.bind_account_id,
        copy_source_instance_id: payload.copy_source_instance_id,
        init_mode: payload.init_mode,
    })?;
    to_value(InstanceProfileView::from_profile(
        instance.clone(),
        false,
        is_profile_initialized(&instance.user_data_dir),
    ))
}

fn update_instance(payload: Value) -> Result<Value, String> {
    let payload: UpdateInstancePayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = gemini_instance::get_default_gemini_cli_home_root()?;
        let updated = gemini_instance::update_default_settings(
            payload.bind_account_id,
            payload.working_dir,
            payload.extra_args,
            payload.follow_local_account,
        )?;
        return to_value(default_instance_view(&default_dir, updated));
    }

    let wants_bind = payload
        .bind_account_id
        .as_ref()
        .and_then(|next| next.as_ref())
        .is_some();
    if wants_bind {
        let store = gemini_instance::load_instance_store()?;
        if let Some(target) = store
            .instances
            .iter()
            .find(|item| item.id == payload.instance_id)
        {
            if !is_profile_initialized(&target.user_data_dir) {
                return Err(
                    "INSTANCE_NOT_INITIALIZED:请先启动一次实例创建数据后，再进行账号绑定"
                        .to_string(),
                );
            }
        }
    }

    let instance = gemini_instance::update_instance(gemini_instance::UpdateInstanceParams {
        instance_id: payload.instance_id,
        name: payload.name,
        working_dir: payload.working_dir,
        extra_args: payload.extra_args,
        bind_account_id: payload.bind_account_id,
    })?;
    to_value(InstanceProfileView::from_profile(
        instance.clone(),
        false,
        is_profile_initialized(&instance.user_data_dir),
    ))
}

fn delete_instance(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        return Err("默认实例不可删除".to_string());
    }
    gemini_instance::delete_instance(&payload.instance_id)?;
    Ok(Value::Null)
}

fn start_instance(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = gemini_instance::get_default_gemini_cli_home_root()?;
        let default_settings = gemini_instance::load_default_settings()?;
        if let Some(ref account_id) = default_settings.bind_account_id {
            gemini_account::inject_to_gemini_home(account_id, None)?;
        }
        let _ = gemini_instance::update_default_pid(None)?;
        return to_value(default_instance_view(&default_dir, default_settings));
    }

    let store = gemini_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;

    if let Some(ref account_id) = instance.bind_account_id {
        gemini_account::inject_to_gemini_home(
            account_id,
            Some(Path::new(&instance.user_data_dir)),
        )?;
    }

    let updated = gemini_instance::update_instance_last_launched(&instance.id)?;
    to_value(InstanceProfileView::from_profile(
        updated.clone(),
        false,
        is_profile_initialized(&updated.user_data_dir),
    ))
}

fn stop_instance(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = gemini_instance::get_default_gemini_cli_home_root()?;
        let default_settings = gemini_instance::load_default_settings()?;
        let _ = gemini_instance::update_default_pid(None)?;
        return to_value(default_instance_view(&default_dir, default_settings));
    }

    let updated = gemini_instance::update_instance_pid(&payload.instance_id, None)?;
    to_value(InstanceProfileView::from_profile(
        updated.clone(),
        false,
        is_profile_initialized(&updated.user_data_dir),
    ))
}

fn close_all_instances() -> Result<Value, String> {
    gemini_instance::clear_all_pids()?;
    Ok(Value::Null)
}

fn open_instance_window(_payload: Value) -> Result<Value, String> {
    Err("Gemini Cli 不支持窗口定位，请使用“启动”后的命令在终端中运行".to_string())
}

#[cfg(not(target_os = "windows"))]
fn posix_shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    let needs_quote = value.chars().any(|ch| {
        ch.is_whitespace()
            || matches!(
                ch,
                '\'' | '"' | '$' | '`' | '\\' | '&' | '|' | ';' | '<' | '>' | '(' | ')'
            )
    });
    if !needs_quote {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(target_os = "windows")]
fn windows_cmd_quote(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".to_string();
    }
    let needs_quote = value
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '^' | '&' | '|' | '<' | '>' | '%'));
    if !needs_quote {
        return value.to_string();
    }
    format!("\"{}\"", value.replace('"', "\\\""))
}

fn resolve_instance_launch_context(instance_id: &str) -> Result<GeminiLaunchContext, String> {
    if instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = gemini_instance::get_default_gemini_cli_home_root()?;
        let default_settings = gemini_instance::load_default_settings()?;
        return Ok(GeminiLaunchContext {
            user_data_dir: default_dir.to_string_lossy().to_string(),
            working_dir: default_settings.working_dir,
            extra_args: default_settings.extra_args,
            use_home_env: false,
        });
    }

    let store = gemini_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == instance_id)
        .ok_or("实例不存在")?;
    Ok(GeminiLaunchContext {
        user_data_dir: instance.user_data_dir,
        working_dir: instance.working_dir,
        extra_args: instance.extra_args,
        use_home_env: true,
    })
}

fn build_launch_command(context: &GeminiLaunchContext) -> String {
    let parsed_args = process::parse_extra_args(&context.extra_args);
    let mut command_parts = Vec::new();

    if let Some(ref dir) = context.working_dir {
        if !dir.trim().is_empty() {
            #[cfg(target_os = "windows")]
            command_parts.push(format!("cd /d \"{}\"", dir.replace('"', "\"\"")));
            #[cfg(not(target_os = "windows"))]
            command_parts.push(format!("cd {}", posix_shell_quote(dir)));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if context.use_home_env {
            let escaped_home = context.user_data_dir.replace('"', "\"\"");
            command_parts.push(format!("set \"GEMINI_CLI_HOME={}\"", escaped_home));
        }

        let mut gemini_cmd = "gemini".to_string();
        for arg in parsed_args {
            if !arg.trim().is_empty() {
                gemini_cmd.push(' ');
                gemini_cmd.push_str(&windows_cmd_quote(arg.trim()));
            }
        }
        command_parts.push(gemini_cmd);
        return command_parts.join(" && ");
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut gemini_cmd = if context.use_home_env {
            format!(
                "GEMINI_CLI_HOME={} gemini",
                posix_shell_quote(&context.user_data_dir)
            )
        } else {
            "gemini".to_string()
        };

        for arg in parsed_args {
            let trimmed = arg.trim();
            if !trimmed.is_empty() {
                gemini_cmd.push(' ');
                gemini_cmd.push_str(&posix_shell_quote(trimmed));
            }
        }
        command_parts.push(gemini_cmd);
        command_parts.join(" && ")
    }
}

fn get_instance_launch_command(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    let context = resolve_instance_launch_context(&payload.instance_id)?;
    to_value(GeminiInstanceLaunchInfo {
        instance_id: payload.instance_id,
        launch_command: build_launch_command(&context),
        user_data_dir: context.user_data_dir,
    })
}

#[cfg(target_os = "macos")]
fn escape_applescript(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn execute_instance_launch_command(payload: Value) -> Result<Value, String> {
    let payload: ExecuteLaunchPayload = parse_payload(payload)?;
    let context = resolve_instance_launch_context(&payload.instance_id)?;
    let command = build_launch_command(&context);

    let config = config::get_user_config();
    let terminal = payload
        .terminal
        .unwrap_or(config.default_terminal)
        .trim()
        .to_string();

    #[cfg(target_os = "macos")]
    {
        let is_iterm = terminal.to_lowercase().contains("iterm");
        let is_terminal_app = terminal == "system" || terminal.is_empty() || terminal == "Terminal";
        let app_name = if is_terminal_app {
            "Terminal"
        } else {
            &terminal
        };

        let script = if is_iterm {
            format!(
                "tell application \"iTerm\"
                    activate
                    if not (exists window 1) then
                        create window with default profile
                        tell current session of current window
                            write text \"{}\"
                        end tell
                    else
                        tell current window
                            create tab with default profile
                            tell current session
                                write text \"{}\"
                            end tell
                        end tell
                    end if
                end tell",
                escape_applescript(&command),
                escape_applescript(&command)
            )
        } else if is_terminal_app {
            format!(
                "tell application \"Terminal\"
                    activate
                    do script \"{}\"
                end tell",
                escape_applescript(&command)
            )
        } else {
            return Err(format!(
                "当前终端暂不支持直接执行：{}。请改用 Terminal 或 iTerm2。",
                terminal
            ));
        };

        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| format!("打开终端失败 ({}): {}", app_name, e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("终端执行失败: {}", stderr.trim()));
        }
        return to_value(format!("已在 {} 执行 Gemini Cli 命令", app_name));
    }

    #[cfg(target_os = "windows")]
    {
        let mut cmd;
        if terminal == "PowerShell" || terminal == "powershell" {
            cmd = Command::new("powershell");
            cmd.args(["-NoExit", "-Command", &command]);
        } else if terminal == "pwsh" {
            cmd = Command::new("pwsh");
            cmd.args(["-NoExit", "-Command", &command]);
        } else if terminal == "wt" {
            cmd = Command::new("wt");
            cmd.args(["-p", "Command Prompt", "cmd", "/K", &command]);
        } else {
            cmd = Command::new("cmd");
            cmd.args(["/C", "start", "", "cmd", "/K", &command]);
        }

        cmd.spawn().map_err(|e| format!("打开终端失败: {}", e))?;
        return to_value("已在终端执行 Gemini Cli 命令".to_string());
    }

    #[cfg(target_os = "linux")]
    {
        let shell_command = format!("{}; exec bash", command);
        let mut cmd = if terminal == "system" || terminal.is_empty() {
            Command::new("x-terminal-emulator")
        } else {
            Command::new(&terminal)
        };

        cmd.args(["-e", "bash", "-lc", &shell_command])
            .spawn()
            .or_else(|_| {
                if terminal == "system" || terminal.is_empty() {
                    Command::new("gnome-terminal")
                        .args(["--", "bash", "-lc", &shell_command])
                        .spawn()
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "指定终端未找到",
                    ))
                }
            })
            .or_else(|_| {
                if terminal == "system" || terminal.is_empty() {
                    Command::new("konsole")
                        .args(["-e", "bash", "-lc", &shell_command])
                        .spawn()
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "指定终端未找到",
                    ))
                }
            })
            .or_else(|_| Command::new("sh").args(["-lc", &command]).spawn())
            .map_err(|e| format!("执行 Gemini Cli 命令失败: {}", e))?;
        return to_value("已执行 Gemini Cli 命令".to_string());
    }

    #[allow(unreachable_code)]
    Err("不支持的操作系统".to_string())
}

fn handle_rpc(runtime: &Runtime, request: RpcRequest) -> Result<Value, String> {
    match request.method.as_str() {
        "health.check" => Ok(json!({ "status": "ok" })),
        "adapter.shutdown" => Ok(Value::Null),
        "accounts.list" => to_value(gemini_account::list_accounts_checked()?),
        "accounts.current" => to_value(
            gemini_account::resolve_current_account(&gemini_account::list_accounts_checked()?)
                .map(|account| account.id),
        ),
        "accounts.delete" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            gemini_account::remove_account(&payload.account_id)?;
            Ok(Value::Null)
        }
        "accounts.deleteMany" => {
            let payload: AccountIdsPayload = parse_payload(request.payload)?;
            gemini_account::remove_accounts(&payload.account_ids)?;
            Ok(Value::Null)
        }
        "accounts.importJson" => import_json(runtime, request.payload),
        "accounts.importLocal" => import_local(runtime),
        "accounts.addToken" => add_with_token(runtime, request.payload),
        "accounts.export" => {
            let payload: AccountIdsPayload = parse_payload(request.payload)?;
            to_value(gemini_account::export_accounts(&payload.account_ids)?)
        }
        "accounts.indexPath" => to_value(gemini_account::accounts_index_path_string()?),
        "accounts.refresh" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            let account =
                runtime.block_on(gemini_account::refresh_account_token(&payload.account_id))?;
            let _ = gemini_account::run_quota_alert_if_needed();
            to_value(account)
        }
        "accounts.refreshAll" => {
            let results = runtime.block_on(gemini_account::refresh_all_tokens())?;
            let success_count = results.iter().filter(|(_, item)| item.is_ok()).count();
            if success_count > 0 {
                let _ = gemini_account::run_quota_alert_if_needed();
            }
            to_value(success_count as i32)
        }
        "accounts.updateTags" => {
            let payload: TagsPayload = parse_payload(request.payload)?;
            to_value(gemini_account::update_account_tags(
                &payload.account_id,
                payload.tags,
            )?)
        }
        "quota.alertPayload" => to_value(gemini_account::run_quota_alert_if_needed()?),
        "oauth.start" => to_value(runtime.block_on(gemini_oauth::start_login())?),
        "oauth.complete" => complete_oauth(runtime, request.payload),
        "oauth.cancel" => {
            let payload: LoginCancelPayload = parse_payload(request.payload)?;
            gemini_oauth::cancel_login(payload.login_id.as_deref())?;
            Ok(Value::Null)
        }
        "oauth.submitCallbackUrl" => {
            let payload: CallbackUrlPayload = parse_payload(request.payload)?;
            gemini_oauth::submit_callback_url(&payload.login_id, &payload.callback_url)?;
            Ok(Value::Null)
        }
        "oauth.restorePendingListener" => {
            gemini_oauth::restore_pending_oauth_state();
            Ok(Value::Null)
        }
        "switch.inject" => switch_inject(request.payload),
        "switch.injectDefaultProfile" => inject_default_profile(request.payload),
        "instances.store.get" => to_value(gemini_instance::load_instance_store()?),
        "instances.store.replace" => {
            let payload: InstanceStorePayload = parse_payload(request.payload)?;
            let store = sanitize_instance_store(&payload.store);
            gemini_instance::save_instance_store(&store)?;
            Ok(Value::Null)
        }
        "instance.getDefaults" => to_value(gemini_instance::get_instance_defaults()?),
        "instance.list" => to_value(list_instances()?),
        "instance.create" => create_instance(request.payload),
        "instance.update" => update_instance(request.payload),
        "instance.delete" => delete_instance(request.payload),
        "instance.start" => start_instance(request.payload),
        "instance.stop" => stop_instance(request.payload),
        "instance.closeAll" => close_all_instances(),
        "instance.openWindow" => open_instance_window(request.payload),
        "instance.getLaunchCommand" => get_instance_launch_command(request.payload),
        "instance.executeLaunchCommand" => execute_instance_launch_command(request.payload),
        "runtime.status" => to_value(list_instances()?),
        "runtime.startDefault" => start_instance(json!({ "instanceId": DEFAULT_INSTANCE_ID })),
        "runtime.stopDefault" => stop_instance(json!({ "instanceId": DEFAULT_INSTANCE_ID })),
        "runtime.restartDefault" => {
            let _ = stop_instance(json!({ "instanceId": DEFAULT_INSTANCE_ID }));
            start_instance(json!({ "instanceId": DEFAULT_INSTANCE_ID }))
        }
        "runtime.focusDefault" => {
            open_instance_window(json!({ "instanceId": DEFAULT_INSTANCE_ID }))
        }
        other => Err(format!("未知 Gemini adapter 方法: {}", other)),
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
        serde_json::json!({
            "ok": false,
            "error": { "message": format!("序列化 Gemini adapter HTTP 响应失败: {}", error) }
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
            error_response("Gemini adapter 路由不存在".to_string()),
        );
        return;
    }
    if !is_authorized(&request, token) {
        write_json_response(
            request,
            401,
            error_response("Gemini adapter token 无效".to_string()),
        );
        return;
    }

    let mut body = String::new();
    if let Err(error) = request.as_reader().read_to_string(&mut body) {
        write_json_response(
            request,
            400,
            error_response(format!("读取 Gemini adapter 请求失败: {}", error)),
        );
        return;
    }

    let rpc_request = match serde_json::from_str::<RpcRequest>(&body) {
        Ok(value) => value,
        Err(error) => {
            write_json_response(
                request,
                400,
                error_response(format!("解析 Gemini adapter 请求 JSON 失败: {}", error)),
            );
            return;
        }
    };

    let should_shutdown = rpc_request.method == "adapter.shutdown";
    let response = match handle_rpc(runtime, rpc_request) {
        Ok(data) => success_response(data),
        Err(error) => error_response(error),
    };
    write_json_response(request, 200, response);
    if should_shutdown {
        shutdown.store(true, Ordering::SeqCst);
    }
}

fn main() {
    cockpit_core::modules::logger::init_logger();

    let runtime = Runtime::new().expect("create tokio runtime");
    let server = Server::http("127.0.0.1:0").expect("bind gemini adapter server");
    let address = server.server_addr().to_string();
    let port = address
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .expect("parse gemini adapter port");
    let token = Uuid::new_v4().simple().to_string();
    let shutdown = Arc::new(AtomicBool::new(false));

    println!(
        "{}",
        serde_json::json!({
            "ok": true,
            "protocol": "http-json-v1",
            "host": "127.0.0.1",
            "port": port,
            "token": token
        })
    );

    for request in server.incoming_requests() {
        handle_http_request(&runtime, &shutdown, &token, request);
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
    }
}
