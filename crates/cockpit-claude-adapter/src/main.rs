use cockpit_core::models::claude::{ClaudeAuthMode, ClaudeDesktopGatewayModelMapping};
use cockpit_core::models::{
    DefaultInstanceSettings, InstanceLaunchMode, InstanceProfile, InstanceStore,
};
use cockpit_core::modules::{
    claude_account, claude_instance, config, process, provider_current_state,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
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
struct PlanPayload {
    account_id: String,
    plan_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotePayload {
    account_id: String,
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiKeyProviderPayload {
    api_key: String,
    account_name: Option<String>,
    api_base_url: Option<String>,
    api_provider_id: Option<String>,
    api_provider_name: Option<String>,
    api_provider_source_tag: Option<String>,
    api_provider_website: Option<String>,
    api_provider_api_key_url: Option<String>,
    api_key_field: Option<String>,
    api_model_catalog: Option<Vec<String>>,
    api_extra_env: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopGatewayImportPayload {
    api_key: String,
    account_name: Option<String>,
    api_base_url: Option<String>,
    api_provider_id: Option<String>,
    api_provider_name: Option<String>,
    api_provider_source_tag: Option<String>,
    api_provider_website: Option<String>,
    api_provider_api_key_url: Option<String>,
    api_key_field: Option<String>,
    api_model_catalog: Option<Vec<String>>,
    api_extra_env: Option<BTreeMap<String, String>>,
    auth_scheme: Option<String>,
    desktop_gateway_models: Option<Vec<String>>,
    desktop_gateway_connection_mode: Option<String>,
    desktop_gateway_upstream_models: Option<Vec<String>>,
    desktop_gateway_model_mappings: Option<Vec<ClaudeDesktopGatewayModelMapping>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopGatewayUpdatePayload {
    account_id: String,
    api_key: String,
    account_name: Option<String>,
    api_base_url: Option<String>,
    api_provider_id: Option<String>,
    api_provider_name: Option<String>,
    api_provider_source_tag: Option<String>,
    api_provider_website: Option<String>,
    api_provider_api_key_url: Option<String>,
    api_key_field: Option<String>,
    api_model_catalog: Option<Vec<String>>,
    api_extra_env: Option<BTreeMap<String, String>>,
    auth_scheme: Option<String>,
    desktop_gateway_models: Option<Vec<String>>,
    desktop_gateway_connection_mode: Option<String>,
    desktop_gateway_upstream_models: Option<Vec<String>>,
    desktop_gateway_model_mappings: Option<Vec<ClaudeDesktopGatewayModelMapping>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GatewayModelsPayload {
    api_key: String,
    api_base_url: String,
    auth_scheme: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuthCompletePayload {
    login_id: String,
    callback_or_code: String,
    email_hint: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginCancelPayload {
    login_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopLoginStartPayload {
    progress_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopLoginCompletePayload {
    login_id: String,
    account_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CliLaunchPayload {
    account_id: String,
    working_dir: String,
    terminal: Option<String>,
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
    launch_mode: Option<InstanceLaunchMode>,
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
    launch_mode: Option<InstanceLaunchMode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstanceIdPayload {
    instance_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstanceExecutePayload {
    instance_id: String,
    terminal: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetectLaunchPathPayload {
    force: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanLaunchTargetsPayload {
    scan_roots: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SwitchResult {
    message: String,
    current_platform: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeCliLaunchInfo {
    account_id: String,
    account_email: String,
    working_dir: String,
    launch_command: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeInstanceProfileView {
    id: String,
    name: String,
    user_data_dir: String,
    working_dir: Option<String>,
    extra_args: String,
    bind_account_id: Option<String>,
    launch_mode: InstanceLaunchMode,
    created_at: i64,
    last_launched_at: Option<i64>,
    last_pid: Option<u32>,
    running: bool,
    initialized: bool,
    is_default: bool,
    follow_local_account: bool,
}

impl ClaudeInstanceProfileView {
    fn from_profile(profile: InstanceProfile, running: bool, initialized: bool) -> Self {
        let last_pid = if is_cli_launch_mode(&profile.launch_mode) {
            None
        } else {
            profile.last_pid
        };
        Self {
            id: profile.id,
            name: profile.name,
            user_data_dir: profile.user_data_dir,
            working_dir: profile.working_dir,
            extra_args: profile.extra_args,
            bind_account_id: profile.bind_account_id,
            launch_mode: profile.launch_mode,
            created_at: profile.created_at,
            last_launched_at: profile.last_launched_at,
            last_pid,
            running,
            initialized,
            is_default: false,
            follow_local_account: false,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeInstanceLaunchInfo {
    instance_id: String,
    user_data_dir: String,
    launch_command: String,
}

struct ClaudeCliLaunchContext {
    user_data_dir: String,
    working_dir: Option<String>,
    extra_args: String,
    use_config_env: bool,
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
        .map_err(|error| format!("序列化 Claude adapter 响应失败: {}", error))
}

fn parse_payload<T: for<'de> Deserialize<'de>>(payload: Value) -> Result<T, String> {
    serde_json::from_value(payload)
        .map_err(|error| format!("解析 Claude adapter 请求失败: {}", error))
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

fn api_key_provider_config(
    payload: &ApiKeyProviderPayload,
) -> claude_account::ClaudeApiKeyProviderConfig {
    claude_account::ClaudeApiKeyProviderConfig {
        api_base_url: payload.api_base_url.clone(),
        api_provider_id: payload.api_provider_id.clone(),
        api_provider_name: payload.api_provider_name.clone(),
        api_provider_source_tag: payload.api_provider_source_tag.clone(),
        api_provider_website: payload.api_provider_website.clone(),
        api_provider_api_key_url: payload.api_provider_api_key_url.clone(),
        api_key_field: payload.api_key_field.clone(),
        api_model_catalog: payload.api_model_catalog.clone(),
        api_extra_env: payload.api_extra_env.clone(),
    }
}

fn desktop_gateway_provider_config(
    payload: &DesktopGatewayImportPayload,
) -> claude_account::ClaudeApiKeyProviderConfig {
    claude_account::ClaudeApiKeyProviderConfig {
        api_base_url: payload.api_base_url.clone(),
        api_provider_id: payload.api_provider_id.clone(),
        api_provider_name: payload.api_provider_name.clone(),
        api_provider_source_tag: payload.api_provider_source_tag.clone(),
        api_provider_website: payload.api_provider_website.clone(),
        api_provider_api_key_url: payload.api_provider_api_key_url.clone(),
        api_key_field: payload.api_key_field.clone(),
        api_model_catalog: payload.api_model_catalog.clone(),
        api_extra_env: payload.api_extra_env.clone(),
    }
}

fn desktop_gateway_update_provider_config(
    payload: &DesktopGatewayUpdatePayload,
) -> claude_account::ClaudeApiKeyProviderConfig {
    claude_account::ClaudeApiKeyProviderConfig {
        api_base_url: payload.api_base_url.clone(),
        api_provider_id: payload.api_provider_id.clone(),
        api_provider_name: payload.api_provider_name.clone(),
        api_provider_source_tag: payload.api_provider_source_tag.clone(),
        api_provider_website: payload.api_provider_website.clone(),
        api_provider_api_key_url: payload.api_provider_api_key_url.clone(),
        api_key_field: payload.api_key_field.clone(),
        api_model_catalog: payload.api_model_catalog.clone(),
        api_extra_env: payload.api_extra_env.clone(),
    }
}

fn switch_inject(payload: Value) -> Result<Value, String> {
    let payload: AccountIdPayload = parse_payload(payload)?;
    let account = claude_account::load_account(&payload.account_id)
        .ok_or_else(|| format!("Claude account not found: {}", payload.account_id))?;
    claude_account::inject_to_claude(&payload.account_id)?;

    let current_platform = if matches!(
        account.auth_mode,
        ClaudeAuthMode::DesktopOAuth | ClaudeAuthMode::DesktopGateway
    ) {
        "claude_desktop_account"
    } else {
        "claude_code_account"
    };
    provider_current_state::set_current_account_id(current_platform, Some(&payload.account_id))?;

    let message = match account.auth_mode {
        ClaudeAuthMode::DesktopGateway => {
            format!("Claude Desktop 供应商配置已应用: {}", account.email)
        }
        ClaudeAuthMode::DesktopOAuth => format!("Claude Desktop 登录态已切换: {}", account.email),
        ClaudeAuthMode::ApiKey => format!("Claude Code API Key 已应用: {}", account.email),
        _ => format!("切换完成: {}", account.email),
    };

    to_value(SwitchResult {
        message,
        current_platform: current_platform.to_string(),
    })
}

fn is_cli_launch_mode(mode: &InstanceLaunchMode) -> bool {
    matches!(mode, InstanceLaunchMode::Cli)
}

fn default_user_data_dir_for_launch_mode(
    mode: &InstanceLaunchMode,
) -> Result<std::path::PathBuf, String> {
    if is_cli_launch_mode(mode) {
        claude_instance::get_default_claude_cli_config_dir()
    } else {
        claude_instance::get_default_claude_config_dir()
    }
}

fn default_instance_view(
    user_data_dir: &Path,
    settings: &DefaultInstanceSettings,
    running: bool,
    last_pid: Option<u32>,
) -> ClaudeInstanceProfileView {
    ClaudeInstanceProfileView {
        id: DEFAULT_INSTANCE_ID.to_string(),
        name: String::new(),
        user_data_dir: user_data_dir.to_string_lossy().to_string(),
        working_dir: settings.working_dir.clone(),
        extra_args: settings.extra_args.clone(),
        bind_account_id: settings.bind_account_id.clone(),
        launch_mode: settings.launch_mode.clone(),
        created_at: 0,
        last_launched_at: None,
        last_pid: if is_cli_launch_mode(&settings.launch_mode) {
            None
        } else {
            last_pid
        },
        running,
        initialized: claude_instance::is_profile_initialized(user_data_dir),
        is_default: true,
        follow_local_account: false,
    }
}

fn inject_bound_account_for_instance_start(
    user_data_dir: &str,
    bind_account_id: Option<&str>,
    backup_existing: bool,
) -> Result<(), String> {
    let Some(bind_id) = bind_account_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    let account = claude_account::load_account(bind_id)
        .ok_or_else(|| format!("绑定账号不存在: {}", bind_id))?;

    match account.auth_mode {
        ClaudeAuthMode::DesktopOAuth => claude_account::restore_desktop_account_to_profile(
            bind_id,
            Path::new(user_data_dir),
            backup_existing,
        ),
        ClaudeAuthMode::DesktopGateway => {
            claude_account::restore_desktop_gateway_account_to_profile(
                bind_id,
                Path::new(user_data_dir),
                backup_existing,
            )
        }
        ClaudeAuthMode::ApiKey => Err(
            "Claude API Key 账号不能写入 Claude 登录态，请选择 Claude 登录账号或取消绑定。"
                .to_string(),
        ),
        _ => {
            Err("旧 OAuth 账号已不再支持用于 Claude 实例，请重新添加 Claude 登录账号。".to_string())
        }
    }
}

fn inject_bound_account_for_cli_instance_start(
    config_dir: Option<&Path>,
    bind_account_id: Option<&str>,
) -> Result<(), String> {
    let Some(bind_id) = bind_account_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    let account = claude_account::load_account(bind_id)
        .ok_or_else(|| format!("绑定账号不存在: {}", bind_id))?;
    if matches!(
        account.auth_mode,
        ClaudeAuthMode::DesktopOAuth | ClaudeAuthMode::DesktopGateway
    ) {
        return Err(
            "Claude 登录账号不能写入 Claude CLI 实例，请选择 Claude CLI OAuth / API Key 账号。"
                .to_string(),
        );
    }

    if let Some(config_dir) = config_dir {
        let _ = claude_account::sync_cli_account_from_config_dir_if_same(bind_id, config_dir)?;
    }
    claude_account::inject_to_claude_config(bind_id, config_dir)?;
    provider_current_state::set_current_account_id("claude_code_account", Some(bind_id))?;
    Ok(())
}

fn list_instances() -> Result<Vec<ClaudeInstanceProfileView>, String> {
    let store = claude_instance::load_instance_store()?;
    let default_settings = store.default_settings.clone();
    let default_dir = default_user_data_dir_for_launch_mode(&default_settings.launch_mode)?;
    let process_entries = claude_instance::collect_claude_process_entries();

    let mut result: Vec<ClaudeInstanceProfileView> = store
        .instances
        .into_iter()
        .map(|instance| {
            let is_cli = is_cli_launch_mode(&instance.launch_mode);
            let resolved_pid = if is_cli {
                None
            } else {
                claude_instance::resolve_claude_pid_from_entries(
                    instance.last_pid,
                    Some(&instance.user_data_dir),
                    &process_entries,
                )
            };
            let running = !is_cli && resolved_pid.is_some();
            let initialized =
                claude_instance::is_profile_initialized(Path::new(&instance.user_data_dir));
            let mut view = ClaudeInstanceProfileView::from_profile(instance, running, initialized);
            view.last_pid = resolved_pid;
            view
        })
        .collect();

    let default_pid = if is_cli_launch_mode(&default_settings.launch_mode) {
        None
    } else {
        claude_instance::resolve_claude_pid_from_entries(
            default_settings.last_pid,
            None,
            &process_entries,
        )
    };
    result.push(default_instance_view(
        &default_dir,
        &default_settings,
        default_pid.is_some(),
        default_pid,
    ));

    Ok(result)
}

fn create_instance(payload: Value) -> Result<Value, String> {
    let payload: CreateInstancePayload = parse_payload(payload)?;
    let instance = claude_instance::create_instance(claude_instance::CreateInstanceParams {
        name: payload.name,
        user_data_dir: payload.user_data_dir,
        working_dir: payload.working_dir,
        extra_args: payload.extra_args.unwrap_or_default(),
        bind_account_id: payload.bind_account_id,
        copy_source_instance_id: payload.copy_source_instance_id,
        init_mode: payload.init_mode,
        launch_mode: payload.launch_mode,
    })?;
    let initialized = claude_instance::is_profile_initialized(Path::new(&instance.user_data_dir));
    to_value(ClaudeInstanceProfileView::from_profile(
        instance,
        false,
        initialized,
    ))
}

fn update_instance(payload: Value) -> Result<Value, String> {
    let payload: UpdateInstancePayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let updated = claude_instance::update_default_settings(
            payload.bind_account_id,
            payload.working_dir,
            payload.extra_args,
            payload.follow_local_account,
            payload.launch_mode,
        )?;
        let default_dir = default_user_data_dir_for_launch_mode(&updated.launch_mode)?;
        let resolved_pid = if is_cli_launch_mode(&updated.launch_mode) {
            None
        } else {
            claude_instance::resolve_claude_pid(updated.last_pid, None)
        };
        return to_value(default_instance_view(
            &default_dir,
            &updated,
            resolved_pid.is_some(),
            resolved_pid,
        ));
    }

    let instance = claude_instance::update_instance(claude_instance::UpdateInstanceParams {
        instance_id: payload.instance_id,
        name: payload.name,
        working_dir: payload.working_dir,
        extra_args: payload.extra_args,
        bind_account_id: payload.bind_account_id,
        launch_mode: payload.launch_mode,
    })?;
    let resolved_pid = if is_cli_launch_mode(&instance.launch_mode) {
        None
    } else {
        claude_instance::resolve_claude_pid(instance.last_pid, Some(&instance.user_data_dir))
    };
    let initialized = claude_instance::is_profile_initialized(Path::new(&instance.user_data_dir));
    let mut view =
        ClaudeInstanceProfileView::from_profile(instance, resolved_pid.is_some(), initialized);
    view.last_pid = resolved_pid;
    to_value(view)
}

fn delete_instance(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        return Err("默认实例不可删除".to_string());
    }
    claude_instance::delete_instance(&payload.instance_id)?;
    Ok(Value::Null)
}

fn start_instance(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_settings = claude_instance::load_default_settings()?;
        let default_dir = default_user_data_dir_for_launch_mode(&default_settings.launch_mode)?;
        let default_dir_str = default_dir.to_string_lossy().to_string();

        if is_cli_launch_mode(&default_settings.launch_mode) {
            inject_bound_account_for_cli_instance_start(
                None,
                default_settings.bind_account_id.as_deref(),
            )?;
            let updated = claude_instance::update_default_pid(None)?;
            return to_value(default_instance_view(&default_dir, &updated, false, None));
        }

        claude_instance::ensure_claude_launch_path_configured()?;
        if let Some(pid) = claude_instance::resolve_claude_pid(default_settings.last_pid, None) {
            process::close_pid(pid, 20)?;
            let _ = claude_instance::update_default_pid(None)?;
        }
        claude_instance::close_claude(&[default_dir_str.clone()], 20)?;
        inject_bound_account_for_instance_start(
            &default_dir_str,
            default_settings.bind_account_id.as_deref(),
            true,
        )?;

        let extra_args = process::parse_extra_args(&default_settings.extra_args);
        let pid =
            claude_instance::start_claude_default_with_args_with_new_window(&extra_args, false)?;
        let _ = claude_instance::update_default_pid(Some(pid))?;
        let running = claude_instance::resolve_claude_pid(Some(pid), None).is_some();
        return to_value(default_instance_view(
            &default_dir,
            &default_settings,
            running,
            Some(pid),
        ));
    }

    let store = claude_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;

    if is_cli_launch_mode(&instance.launch_mode) {
        inject_bound_account_for_cli_instance_start(
            Some(Path::new(&instance.user_data_dir)),
            instance.bind_account_id.as_deref(),
        )?;
        let updated = claude_instance::update_instance_last_launched(&instance.id)?;
        let initialized =
            claude_instance::is_profile_initialized(Path::new(&updated.user_data_dir));
        return to_value(ClaudeInstanceProfileView::from_profile(
            updated,
            false,
            initialized,
        ));
    }

    claude_instance::ensure_claude_multi_instance_launch_path_configured()?;
    if let Some(pid) =
        claude_instance::resolve_claude_pid(instance.last_pid, Some(&instance.user_data_dir))
    {
        process::close_pid(pid, 20)?;
        let _ = claude_instance::update_instance_pid(&instance.id, None)?;
    }

    claude_instance::close_claude(&[instance.user_data_dir.clone()], 20)?;
    inject_bound_account_for_instance_start(
        &instance.user_data_dir,
        instance.bind_account_id.as_deref(),
        false,
    )?;

    let extra_args = process::parse_extra_args(&instance.extra_args);
    let pid = claude_instance::start_claude_with_args_with_new_window(
        &instance.user_data_dir,
        &extra_args,
        true,
    )?;
    let updated = claude_instance::update_instance_after_start(&instance.id, pid)?;
    let running =
        claude_instance::resolve_claude_pid(Some(pid), Some(&updated.user_data_dir)).is_some();
    let initialized = claude_instance::is_profile_initialized(Path::new(&updated.user_data_dir));
    to_value(ClaudeInstanceProfileView::from_profile(
        updated,
        running,
        initialized,
    ))
}

fn stop_instance(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_settings = claude_instance::load_default_settings()?;
        let default_dir = default_user_data_dir_for_launch_mode(&default_settings.launch_mode)?;

        if is_cli_launch_mode(&default_settings.launch_mode) {
            let updated = claude_instance::update_default_pid(None)?;
            return to_value(default_instance_view(&default_dir, &updated, false, None));
        }

        if let Some(pid) = claude_instance::resolve_claude_pid(default_settings.last_pid, None) {
            process::close_pid(pid, 20)?;
        }

        let updated = claude_instance::update_default_pid(None)?;
        let running = updated
            .last_pid
            .and_then(|pid| claude_instance::resolve_claude_pid(Some(pid), None))
            .is_some();
        return to_value(default_instance_view(
            &default_dir,
            &default_settings,
            running,
            None,
        ));
    }

    let store = claude_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;

    if is_cli_launch_mode(&instance.launch_mode) {
        let updated = claude_instance::update_instance_pid(&instance.id, None)?;
        let initialized =
            claude_instance::is_profile_initialized(Path::new(&updated.user_data_dir));
        return to_value(ClaudeInstanceProfileView::from_profile(
            updated,
            false,
            initialized,
        ));
    }

    if let Some(pid) =
        claude_instance::resolve_claude_pid(instance.last_pid, Some(&instance.user_data_dir))
    {
        process::close_pid(pid, 20)?;
    }

    let updated = claude_instance::update_instance_pid(&instance.id, None)?;
    let initialized = claude_instance::is_profile_initialized(Path::new(&updated.user_data_dir));
    to_value(ClaudeInstanceProfileView::from_profile(
        updated,
        false,
        initialized,
    ))
}

fn open_instance_window(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_settings = claude_instance::load_default_settings()?;
        if is_cli_launch_mode(&default_settings.launch_mode) {
            return Err("Claude CLI 实例不支持窗口定位，请使用启动命令在终端中运行".to_string());
        }
        claude_instance::focus_claude_instance(default_settings.last_pid, None)
            .map_err(|err| format!("定位 Claude 默认实例窗口失败: {}", err))?;
        return Ok(Value::Null);
    }

    let store = claude_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;
    if is_cli_launch_mode(&instance.launch_mode) {
        return Err("Claude CLI 实例不支持窗口定位，请使用启动命令在终端中运行".to_string());
    }

    claude_instance::focus_claude_instance(instance.last_pid, Some(&instance.user_data_dir))
        .map_err(|err| {
            format!(
                "定位 Claude 实例窗口失败: instance_id={}, err={}",
                instance.id, err
            )
        })?;
    Ok(Value::Null)
}

fn close_all_instances() -> Result<Value, String> {
    let store = claude_instance::load_instance_store()?;
    let mut target_dirs = Vec::new();
    if !is_cli_launch_mode(&store.default_settings.launch_mode) {
        let default_dir = claude_instance::get_default_claude_config_dir()?;
        target_dirs.push(default_dir.to_string_lossy().to_string());
    }
    for instance in &store.instances {
        if is_cli_launch_mode(&instance.launch_mode) {
            continue;
        }
        let dir = instance.user_data_dir.trim();
        if !dir.is_empty() {
            target_dirs.push(dir.to_string());
        }
    }

    if !target_dirs.is_empty() {
        claude_instance::close_claude(&target_dirs, 20)?;
    }
    let _ = claude_instance::clear_all_pids();
    Ok(Value::Null)
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
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(target_os = "windows")]
fn resolve_claude_cli_command() -> String {
    use std::os::windows::process::CommandExt;

    if let Some(user_profile) = std::env::var_os("USERPROFILE") {
        let candidate = Path::new(&user_profile)
            .join(".local")
            .join("bin")
            .join("claude.exe");
        if candidate.exists() {
            return format!("& {}", powershell_quote(&candidate.to_string_lossy()));
        }
    }

    if let Ok(output) = Command::new("where")
        .arg("claude")
        .creation_flags(0x08000000)
        .stdin(std::process::Stdio::null())
        .output()
    {
        if output.status.success() {
            if let Some(path) = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
            {
                return format!("& {}", powershell_quote(path));
            }
        }
    }

    "claude".to_string()
}

#[cfg(target_os = "macos")]
fn escape_applescript(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn normalize_cli_working_dir(working_dir: &str) -> Result<String, String> {
    let trimmed = working_dir.trim();
    if trimmed.is_empty() {
        return Err("请选择 Claude CLI 工作目录".to_string());
    }
    let path = Path::new(trimmed);
    if !path.is_dir() {
        return Err(format!("Claude CLI 工作目录不存在: {}", trimmed));
    }
    Ok(trimmed.to_string())
}

fn build_claude_cli_command_for_context(
    working_dir: Option<&str>,
    config_dir: Option<&str>,
    extra_args: &str,
    env: &BTreeMap<String, String>,
) -> String {
    let parsed_args = process::parse_extra_args(extra_args);

    #[cfg(target_os = "windows")]
    {
        let mut command_parts = Vec::new();
        if let Some(dir) = working_dir.map(str::trim).filter(|value| !value.is_empty()) {
            command_parts.push(format!(
                "Set-Location -LiteralPath {}",
                powershell_quote(dir)
            ));
        }
        if let Some(dir) = config_dir.map(str::trim).filter(|value| !value.is_empty()) {
            command_parts.push(format!("$env:CLAUDE_CONFIG_DIR={}", powershell_quote(dir)));
        }
        for (key, value) in env {
            command_parts.push(format!("$env:{}={}", key, powershell_quote(value)));
        }

        let mut command = resolve_claude_cli_command();
        for arg in parsed_args {
            let trimmed = arg.trim();
            if !trimmed.is_empty() {
                command.push(' ');
                command.push_str(&powershell_quote(trimmed));
            }
        }
        command_parts.push(command);
        return command_parts.join("; ");
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut command_parts = Vec::new();
        if let Some(dir) = working_dir.map(str::trim).filter(|value| !value.is_empty()) {
            command_parts.push(format!("cd {}", posix_shell_quote(dir)));
        }

        let mut env_parts = Vec::new();
        if let Some(dir) = config_dir.map(str::trim).filter(|value| !value.is_empty()) {
            env_parts.push(format!("CLAUDE_CONFIG_DIR={}", posix_shell_quote(dir)));
        }
        for (key, value) in env {
            env_parts.push(format!("{}={}", key, posix_shell_quote(value)));
        }

        let mut command = String::new();
        if !env_parts.is_empty() {
            command.push_str(&env_parts.join(" "));
            command.push(' ');
        }
        command.push_str("claude");
        for arg in parsed_args {
            let trimmed = arg.trim();
            if !trimmed.is_empty() {
                command.push(' ');
                command.push_str(&posix_shell_quote(trimmed));
            }
        }
        command_parts.push(command);
        return command_parts.join(" && ");
    }

    #[allow(unreachable_code)]
    "claude".to_string()
}

fn build_claude_cli_command(
    working_dir: &str,
    env: &BTreeMap<String, String>,
) -> Result<String, String> {
    let working_dir = normalize_cli_working_dir(working_dir)?;
    Ok(build_claude_cli_command_for_context(
        Some(&working_dir),
        None,
        "",
        env,
    ))
}

fn execute_claude_cli_command(command: &str, terminal: Option<String>) -> Result<String, String> {
    let config = config::get_user_config();
    let terminal = terminal
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
                escape_applescript(command),
                escape_applescript(command)
            )
        } else if is_terminal_app {
            format!(
                "tell application \"Terminal\"
                    activate
                    do script \"{}\"
                end tell",
                escape_applescript(command)
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
        return Ok(format!("已在 {} 执行 Claude CLI 命令", app_name));
    }

    #[cfg(target_os = "windows")]
    {
        let terminal_key = terminal.to_ascii_lowercase();
        let shell = if terminal_key == "pwsh" {
            "pwsh"
        } else {
            "powershell"
        };
        let mut cmd = if terminal_key == "wt" {
            let mut command_process = Command::new("wt");
            command_process.args([
                shell,
                "-NoExit",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                command,
            ]);
            command_process
        } else {
            let mut command_process = Command::new("cmd");
            command_process.args([
                "/C",
                "start",
                "",
                shell,
                "-NoExit",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                command,
            ]);
            command_process
        };

        cmd.spawn().map_err(|e| format!("打开终端失败: {}", e))?;
        return Ok("已打开 Claude CLI 终端窗口".to_string());
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
            .or_else(|_| Command::new("sh").args(["-lc", command]).spawn())
            .map_err(|e| format!("执行 Claude CLI 命令失败: {}", e))?;
        return Ok("已执行 Claude CLI 命令".to_string());
    }

    #[allow(unreachable_code)]
    Err("Claude CLI 终端执行仅支持 macOS、Windows 和 Linux".to_string())
}

fn prepare_claude_cli_launch(
    account_id: &str,
    working_dir: &str,
) -> Result<(cockpit_core::models::claude::ClaudeAccount, String, String), String> {
    let account = claude_account::load_account(account_id)
        .ok_or_else(|| format!("Claude account not found: {}", account_id))?;
    if matches!(
        account.auth_mode,
        ClaudeAuthMode::DesktopOAuth | ClaudeAuthMode::DesktopGateway
    ) {
        return Err(
            "Claude 登录态不能启动 Claude Code CLI，请使用 OAuth / Setup Token 账号。".to_string(),
        );
    }
    let normalized_working_dir = normalize_cli_working_dir(working_dir)?;
    claude_account::inject_to_claude_config(account_id, None)?;
    let command = build_claude_cli_command(&normalized_working_dir, &BTreeMap::new())?;
    provider_current_state::set_current_account_id("claude_code_account", Some(account_id))?;
    Ok((account, normalized_working_dir, command))
}

fn get_cli_launch_command(payload: Value) -> Result<Value, String> {
    let payload: CliLaunchPayload = parse_payload(payload)?;
    let (account, normalized_working_dir, command) =
        prepare_claude_cli_launch(&payload.account_id, &payload.working_dir)?;
    to_value(ClaudeCliLaunchInfo {
        account_id: account.id,
        account_email: account.email,
        working_dir: normalized_working_dir,
        launch_command: command,
    })
}

fn execute_cli_launch_command(payload: Value) -> Result<Value, String> {
    let payload: CliLaunchPayload = parse_payload(payload)?;
    let (_account, _normalized_working_dir, command) =
        prepare_claude_cli_launch(&payload.account_id, &payload.working_dir)?;
    to_value(execute_claude_cli_command(&command, payload.terminal)?)
}

fn resolve_cli_launch_context(instance_id: &str) -> Result<ClaudeCliLaunchContext, String> {
    if instance_id == DEFAULT_INSTANCE_ID {
        let default_settings = claude_instance::load_default_settings()?;
        if !is_cli_launch_mode(&default_settings.launch_mode) {
            return Err("当前实例未启用 CLI 启动方式".to_string());
        }
        let default_dir = claude_instance::get_default_claude_cli_config_dir()?;
        return Ok(ClaudeCliLaunchContext {
            user_data_dir: default_dir.to_string_lossy().to_string(),
            working_dir: default_settings.working_dir,
            extra_args: default_settings.extra_args,
            use_config_env: false,
        });
    }

    let store = claude_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == instance_id)
        .ok_or("实例不存在")?;
    if !is_cli_launch_mode(&instance.launch_mode) {
        return Err("当前实例未启用 CLI 启动方式".to_string());
    }
    Ok(ClaudeCliLaunchContext {
        user_data_dir: instance.user_data_dir,
        working_dir: instance.working_dir,
        extra_args: instance.extra_args,
        use_config_env: true,
    })
}

fn build_cli_launch_command(context: &ClaudeCliLaunchContext) -> String {
    let env = BTreeMap::new();
    build_claude_cli_command_for_context(
        context.working_dir.as_deref(),
        context
            .use_config_env
            .then_some(context.user_data_dir.as_str()),
        &context.extra_args,
        &env,
    )
}

fn prepare_cli_instance_config(
    instance_id: &str,
    context: &ClaudeCliLaunchContext,
) -> Result<(), String> {
    if instance_id == DEFAULT_INSTANCE_ID {
        let default_settings = claude_instance::load_default_settings()?;
        return inject_bound_account_for_cli_instance_start(
            None,
            default_settings.bind_account_id.as_deref(),
        );
    }

    let store = claude_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == instance_id)
        .ok_or("Claude instance not found")?;
    inject_bound_account_for_cli_instance_start(
        Some(Path::new(&context.user_data_dir)),
        instance.bind_account_id.as_deref(),
    )
}

fn get_instance_launch_command(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    let context = resolve_cli_launch_context(&payload.instance_id)?;
    prepare_cli_instance_config(&payload.instance_id, &context)?;
    to_value(ClaudeInstanceLaunchInfo {
        instance_id: payload.instance_id,
        launch_command: build_cli_launch_command(&context),
        user_data_dir: context.user_data_dir,
    })
}

fn execute_instance_launch_command(payload: Value) -> Result<Value, String> {
    let payload: InstanceExecutePayload = parse_payload(payload)?;
    let context = resolve_cli_launch_context(&payload.instance_id)?;
    prepare_cli_instance_config(&payload.instance_id, &context)?;
    let command = build_cli_launch_command(&context);
    to_value(execute_claude_cli_command(&command, payload.terminal)?)
}

fn handle_rpc(runtime: &Runtime, request: RpcRequest) -> Result<Value, String> {
    match request.method.as_str() {
        "health.check" => Ok(json!({ "status": "ok" })),
        "adapter.shutdown" => Ok(Value::Null),
        "accounts.list" => to_value(claude_account::list_accounts_checked()?),
        "accounts.current" => to_value(json!({
            "desktopAccountId": provider_current_state::get_current_account_id("claude_desktop_account")?,
            "codeAccountId": provider_current_state::get_current_account_id("claude_code_account")?,
        })),
        "accounts.delete" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            claude_account::remove_account(&payload.account_id)?;
            Ok(Value::Null)
        }
        "accounts.deleteMany" => {
            let payload: AccountIdsPayload = parse_payload(request.payload)?;
            claude_account::remove_accounts(&payload.account_ids)?;
            Ok(Value::Null)
        }
        "accounts.importJson" => {
            let payload: JsonImportPayload = parse_payload(request.payload)?;
            to_value(claude_account::import_from_json(&payload.json_content)?)
        }
        "accounts.importApiKey" => {
            let payload: ApiKeyProviderPayload = parse_payload(request.payload)?;
            to_value(claude_account::import_api_key(
                &payload.api_key,
                payload.account_name.as_deref(),
                api_key_provider_config(&payload),
            )?)
        }
        "accounts.importDesktopGateway" => {
            let payload: DesktopGatewayImportPayload = parse_payload(request.payload)?;
            to_value(claude_account::import_desktop_gateway(
                &payload.api_key,
                payload.account_name.as_deref(),
                desktop_gateway_provider_config(&payload),
                payload.auth_scheme.as_deref(),
                payload.desktop_gateway_models,
                payload.desktop_gateway_connection_mode.as_deref(),
                payload.desktop_gateway_upstream_models,
                payload.desktop_gateway_model_mappings,
            )?)
        }
        "accounts.updateDesktopGateway" => {
            let payload: DesktopGatewayUpdatePayload = parse_payload(request.payload)?;
            to_value(claude_account::update_desktop_gateway(
                &payload.account_id,
                &payload.api_key,
                payload.account_name.as_deref(),
                desktop_gateway_update_provider_config(&payload),
                payload.auth_scheme.as_deref(),
                payload.desktop_gateway_models,
                payload.desktop_gateway_connection_mode.as_deref(),
                payload.desktop_gateway_upstream_models,
                payload.desktop_gateway_model_mappings,
            )?)
        }
        "accounts.importCliLocal" => to_value(claude_account::import_cli_from_local()?),
        "accounts.export" => {
            let payload: AccountIdsPayload = parse_payload(request.payload)?;
            to_value(claude_account::export_accounts(&payload.account_ids)?)
        }
        "accounts.refresh" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(claude_account::refresh_account_quota(&payload.account_id))?)
        }
        "accounts.refreshAll" => {
            let results = runtime.block_on(claude_account::refresh_all_quotas())?;
            let success_count = results.iter().filter(|(_, item)| item.is_ok()).count();
            to_value(success_count as i32)
        }
        "accounts.updateTags" => {
            let payload: TagsPayload = parse_payload(request.payload)?;
            to_value(claude_account::update_account_tags(
                &payload.account_id,
                payload.tags,
            )?)
        }
        "accounts.updatePlan" => {
            let payload: PlanPayload = parse_payload(request.payload)?;
            to_value(claude_account::update_account_plan(
                &payload.account_id,
                payload.plan_type.as_deref(),
            )?)
        }
        "accounts.updateNote" => {
            let payload: NotePayload = parse_payload(request.payload)?;
            to_value(claude_account::update_account_note(
                &payload.account_id,
                payload.note.as_deref(),
            )?)
        }
        "accounts.indexPath" => to_value(claude_account::accounts_index_path_string()?),
        "gateway.listModels" => {
            let payload: GatewayModelsPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(claude_account::list_desktop_gateway_models(
                    &payload.api_base_url,
                    &payload.api_key,
                    payload.auth_scheme.as_deref(),
                ))?,
            )
        }
        "oauth.start" => to_value(claude_account::start_oauth_login()?),
        "oauth.complete" => {
            let payload: OAuthCompletePayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(claude_account::complete_oauth_login(
                &payload.login_id,
                &payload.callback_or_code,
                payload.email_hint.as_deref(),
            ))?)
        }
        "oauth.cancel" => {
            let payload: LoginCancelPayload = parse_payload(request.payload)?;
            claude_account::cancel_oauth_login(payload.login_id.as_deref())?;
            Ok(Value::Null)
        }
        "desktopLogin.start" => {
            let payload: DesktopLoginStartPayload = parse_payload(request.payload)?;
            to_value(claude_account::start_desktop_login(
                None,
                payload.progress_id,
            )?)
        }
        "desktopLogin.complete" => {
            let payload: DesktopLoginCompletePayload = parse_payload(request.payload)?;
            to_value(claude_account::complete_desktop_login(
                &payload.login_id,
                payload.account_name.as_deref(),
            )?)
        }
        "desktopLogin.cancel" => {
            let payload: LoginCancelPayload = parse_payload(request.payload)?;
            claude_account::cancel_desktop_login(payload.login_id.as_deref())?;
            Ok(Value::Null)
        }
        "desktopLogin.openVerificationWindow" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            claude_account::open_desktop_verification_window(&payload.account_id)?;
            Ok(Value::Null)
        }
        "cli.getLaunchCommand" => get_cli_launch_command(request.payload),
        "cli.executeLaunchCommand" => execute_cli_launch_command(request.payload),
        "instances.store.get" => to_value(claude_instance::load_instance_store()?),
        "instances.store.replace" => {
            let payload: InstanceStorePayload = parse_payload(request.payload)?;
            let store = sanitize_instance_store(&payload.store);
            claude_instance::save_instance_store(&store)?;
            Ok(Value::Null)
        }
        "instance.getDefaults" => to_value(claude_instance::get_instance_defaults()?),
        "instance.list" => to_value(list_instances()?),
        "instance.create" => create_instance(request.payload),
        "instance.update" => update_instance(request.payload),
        "instance.delete" => delete_instance(request.payload),
        "instance.start" => start_instance(request.payload),
        "instance.stop" => stop_instance(request.payload),
        "instance.openWindow" => open_instance_window(request.payload),
        "instance.closeAll" => close_all_instances(),
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
        "runtime.detectLaunchPath" => {
            let payload: DetectLaunchPathPayload = parse_payload(request.payload)?;
            to_value(claude_instance::detect_and_save_claude_launch_path(
                payload.force.unwrap_or(false),
            ))
        }
        "runtime.scanLaunchTargets" => {
            let payload: ScanLaunchTargetsPayload = parse_payload(request.payload)?;
            let roots = payload
                .scan_roots
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            to_value(claude_instance::scan_claude_desktop_launch_targets(roots))
        }
        "switch.inject" => switch_inject(request.payload),
        other => Err(format!("未知 Claude adapter 方法: {}", other)),
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
            "error": { "message": format!("序列化 Claude adapter HTTP 响应失败: {}", error) }
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
            error_response("Claude adapter 路由不存在".to_string()),
        );
        return;
    }
    if !is_authorized(&request, token) {
        write_json_response(
            request,
            401,
            error_response("Claude adapter token 无效".to_string()),
        );
        return;
    }

    let mut body = String::new();
    if let Err(error) = request.as_reader().read_to_string(&mut body) {
        write_json_response(
            request,
            400,
            error_response(format!("读取 Claude adapter 请求失败: {}", error)),
        );
        return;
    }

    let rpc_request = match serde_json::from_str::<RpcRequest>(&body) {
        Ok(value) => value,
        Err(error) => {
            write_json_response(
                request,
                400,
                error_response(format!("解析 Claude adapter 请求 JSON 失败: {}", error)),
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
    let server = Server::http("127.0.0.1:0").expect("bind claude adapter server");
    let address = server.server_addr().to_string();
    let port = address
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .expect("parse claude adapter port");
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
