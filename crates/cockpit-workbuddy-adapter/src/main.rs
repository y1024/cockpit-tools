use cockpit_core::models::workbuddy::{WorkbuddyAccount, WorkbuddyOAuthCompletePayload};
use cockpit_core::models::{DefaultInstanceSettings, InstanceProfileView, InstanceStore};
use cockpit_core::modules::{
    account_store, codebuddy_cn_oauth, process, workbuddy_account, workbuddy_instance,
    workbuddy_oauth,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tiny_http::{Header, Method, Response, Server, StatusCode};
use tokio::runtime::Runtime;
use uuid::Uuid;

const DEFAULT_INSTANCE_ID: &str = "__default__";
const PLATFORM_ID: &str = "workbuddy";

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
#[serde(rename_all = "camelCase")]
struct SwitchResult {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    restart_error: Option<String>,
    path_missing: bool,
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
        .map_err(|error| format!("序列化 WorkBuddy adapter 响应失败: {}", error))
}

fn parse_payload<T: for<'de> Deserialize<'de>>(payload: Value) -> Result<T, String> {
    serde_json::from_value(payload)
        .map_err(|error| format!("解析 WorkBuddy adapter 请求失败: {}", error))
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
    let path = Path::new(user_data_dir);
    if !path.exists() {
        return false;
    }
    match std::fs::read_dir(path) {
        Ok(mut iter) => iter.next().is_some(),
        Err(_) => false,
    }
}

fn path_missing_error(error: &str) -> bool {
    error.starts_with("APP_PATH_NOT_FOUND:") || error.contains("启动 WorkBuddy 失败")
}

fn resolve_running_pid(last_pid: Option<u32>, user_data_dir: Option<&str>) -> Option<u32> {
    process::resolve_workbuddy_pid(last_pid, user_data_dir)
}

fn default_instance_view(
    default_dir: &Path,
    settings: DefaultInstanceSettings,
) -> InstanceProfileView {
    let default_dir_str = default_dir.to_string_lossy().to_string();
    let running_pid = resolve_running_pid(settings.last_pid, None);

    InstanceProfileView {
        id: DEFAULT_INSTANCE_ID.to_string(),
        name: String::new(),
        user_data_dir: default_dir_str,
        working_dir: None,
        extra_args: settings.extra_args,
        bind_account_id: settings.bind_account_id,
        created_at: 0,
        last_launched_at: None,
        last_pid: running_pid,
        running: running_pid.is_some(),
        initialized: is_profile_initialized(&default_dir.to_string_lossy()),
        is_default: true,
        follow_local_account: false,
    }
}

async fn refresh_workbuddy_account_after_login(account: WorkbuddyAccount) -> WorkbuddyAccount {
    let account_id = account.id.clone();
    match workbuddy_account::refresh_account_token(&account_id).await {
        Ok(refreshed) => refreshed,
        Err(_) => account,
    }
}

fn merge_local_payload(
    local_payload: WorkbuddyOAuthCompletePayload,
    remote_payload: Result<WorkbuddyOAuthCompletePayload, String>,
) -> WorkbuddyOAuthCompletePayload {
    let Ok(mut payload) = remote_payload else {
        return local_payload;
    };
    if payload.uid.is_none() {
        payload.uid = local_payload.uid.clone();
    }
    if payload.nickname.is_none() {
        payload.nickname = local_payload.nickname.clone();
    }
    if payload.refresh_token.is_none() {
        payload.refresh_token = local_payload.refresh_token.clone();
    }
    if payload.domain.is_none() {
        payload.domain = local_payload.domain.clone();
    }
    if payload.token_type.is_none() {
        payload.token_type = local_payload.token_type.clone();
    }
    if payload.expires_at.is_none() {
        payload.expires_at = local_payload.expires_at;
    }
    if payload.auth_raw.is_none() {
        payload.auth_raw = local_payload.auth_raw.clone();
    }
    if payload.profile_raw.is_none() {
        payload.profile_raw = local_payload.profile_raw.clone();
    }
    if payload.email.trim().is_empty() || payload.email == "unknown" {
        payload.email = local_payload.email.clone();
    }
    payload
}

fn import_local(runtime: &Runtime) -> Result<Value, String> {
    let Some(local_payload) = workbuddy_account::import_payload_from_local()? else {
        return Err("未在本机 WorkBuddy 客户端中找到登录信息".to_string());
    };
    let local_access_token = local_payload.access_token.clone();
    let remote_payload = runtime.block_on(workbuddy_oauth::build_payload_from_token(
        &local_access_token,
    ));
    let payload = merge_local_payload(local_payload, remote_payload);
    let mut account = workbuddy_account::upsert_account(payload)?;

    for existing in workbuddy_account::list_accounts() {
        if existing.id == account.id || existing.access_token != account.access_token {
            continue;
        }
        let is_placeholder = existing.email.trim().eq_ignore_ascii_case("unknown")
            || existing.email.trim().is_empty()
            || existing
                .uid
                .as_deref()
                .map(|value| value.trim().is_empty())
                .unwrap_or(true);
        if is_placeholder {
            let _ = workbuddy_account::remove_account(&existing.id);
        }
    }

    account = runtime.block_on(refresh_workbuddy_account_after_login(account));
    to_value(vec![account])
}

fn add_with_token(runtime: &Runtime, payload: Value) -> Result<Value, String> {
    let payload: TokenPayload = parse_payload(payload)?;
    let account_payload = runtime.block_on(workbuddy_oauth::build_payload_from_token(
        &payload.access_token,
    ))?;
    to_value(workbuddy_account::upsert_account(account_payload)?)
}

fn complete_oauth(runtime: &Runtime, payload: Value) -> Result<Value, String> {
    let payload: LoginIdPayload = parse_payload(payload)?;
    let result: Result<WorkbuddyAccount, String> = runtime.block_on(async {
        let account_payload = workbuddy_oauth::complete_login(&payload.login_id).await?;
        let mut account = workbuddy_account::upsert_account(account_payload)?;
        account = refresh_workbuddy_account_after_login(account).await;
        Ok(account)
    });
    let _ = workbuddy_oauth::clear_pending_oauth_login(&payload.login_id);
    let account = result?;
    let _ = workbuddy_account::run_quota_alert_if_needed();
    to_value(account)
}

fn inject_bound_account_for_instance_start(bind_account_id: Option<&str>) -> Result<(), String> {
    let Some(bind_id) = bind_account_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    let account = workbuddy_account::load_account(bind_id)
        .ok_or_else(|| format!("绑定账号不存在: {}", bind_id))?;
    workbuddy_account::write_account_to_default_client(&account)
}

fn list_instances() -> Result<Vec<InstanceProfileView>, String> {
    let store = workbuddy_instance::load_instance_store()?;
    let default_dir = workbuddy_instance::get_default_workbuddy_user_data_dir()?;
    let default_dir_str = default_dir.to_string_lossy().to_string();
    let default_settings = store.default_settings.clone();

    let mut result: Vec<InstanceProfileView> = store
        .instances
        .into_iter()
        .map(|instance| {
            let running_pid = resolve_running_pid(instance.last_pid, Some(&instance.user_data_dir));
            let running = running_pid.is_some();
            let initialized = is_profile_initialized(&instance.user_data_dir);
            let mut view = InstanceProfileView::from_profile(instance, running, initialized);
            view.last_pid = running_pid;
            view
        })
        .collect();

    let default_pid = resolve_running_pid(default_settings.last_pid, None);
    result.push(InstanceProfileView {
        id: DEFAULT_INSTANCE_ID.to_string(),
        name: String::new(),
        user_data_dir: default_dir_str,
        working_dir: None,
        extra_args: default_settings.extra_args,
        bind_account_id: default_settings.bind_account_id,
        created_at: 0,
        last_launched_at: None,
        last_pid: default_pid,
        running: default_pid.is_some(),
        initialized: is_profile_initialized(&default_dir.to_string_lossy()),
        is_default: true,
        follow_local_account: false,
    });

    Ok(result)
}

fn create_instance(payload: Value) -> Result<Value, String> {
    let payload: CreateInstancePayload = parse_payload(payload)?;
    let instance = workbuddy_instance::create_instance(workbuddy_instance::CreateInstanceParams {
        working_dir: None,
        name: payload.name,
        user_data_dir: payload.user_data_dir,
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
        let default_dir = workbuddy_instance::get_default_workbuddy_user_data_dir()?;
        let updated = workbuddy_instance::update_default_settings(
            payload.bind_account_id,
            payload.extra_args,
            payload.follow_local_account,
        )?;
        return to_value(default_instance_view(&default_dir, updated));
    }

    let instance = workbuddy_instance::update_instance(workbuddy_instance::UpdateInstanceParams {
        working_dir: None,
        instance_id: payload.instance_id,
        name: payload.name,
        extra_args: payload.extra_args,
        bind_account_id: payload.bind_account_id,
    })?;
    let running_pid = resolve_running_pid(instance.last_pid, Some(&instance.user_data_dir));
    let running = running_pid.is_some();
    let initialized = is_profile_initialized(&instance.user_data_dir);
    let mut view = InstanceProfileView::from_profile(instance, running, initialized);
    view.last_pid = running_pid;
    to_value(view)
}

fn delete_instance(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        return Err("默认实例不可删除".to_string());
    }
    workbuddy_instance::delete_instance(&payload.instance_id)?;
    Ok(Value::Null)
}

fn start_instance(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    process::ensure_workbuddy_launch_path_configured()?;

    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = workbuddy_instance::get_default_workbuddy_user_data_dir()?;
        let default_dir_str = default_dir.to_string_lossy().to_string();
        let default_settings = workbuddy_instance::load_default_settings()?;
        if let Some(pid) = resolve_running_pid(default_settings.last_pid, None) {
            process::close_pid(pid, 20)?;
            let _ = workbuddy_instance::update_default_pid(None)?;
        }
        inject_bound_account_for_instance_start(default_settings.bind_account_id.as_deref())?;
        let extra_args = process::parse_extra_args(&default_settings.extra_args);
        let pid = process::start_workbuddy_default_with_args_with_new_window(&extra_args, true)?;
        let _ = workbuddy_instance::update_default_pid(Some(pid))?;
        let running_pid = resolve_running_pid(Some(pid), None);

        return to_value(InstanceProfileView {
            id: DEFAULT_INSTANCE_ID.to_string(),
            name: String::new(),
            user_data_dir: default_dir_str,
            working_dir: None,
            extra_args: default_settings.extra_args,
            bind_account_id: default_settings.bind_account_id,
            created_at: 0,
            last_launched_at: None,
            last_pid: running_pid,
            running: running_pid.is_some(),
            initialized: is_profile_initialized(&default_dir.to_string_lossy()),
            is_default: true,
            follow_local_account: false,
        });
    }

    let store = workbuddy_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;

    if let Some(pid) = resolve_running_pid(instance.last_pid, Some(&instance.user_data_dir)) {
        process::close_pid(pid, 20)?;
        let _ = workbuddy_instance::update_instance_pid(&instance.id, None)?;
    }
    inject_bound_account_for_instance_start(instance.bind_account_id.as_deref())?;
    let extra_args = process::parse_extra_args(&instance.extra_args);
    let pid = process::start_workbuddy_with_args_with_new_window(
        &instance.user_data_dir,
        &extra_args,
        true,
    )?;
    let updated = workbuddy_instance::update_instance_after_start(&instance.id, pid)?;
    let running_pid = resolve_running_pid(Some(pid), Some(&updated.user_data_dir));
    let initialized = is_profile_initialized(&updated.user_data_dir);
    let mut view = InstanceProfileView::from_profile(updated, running_pid.is_some(), initialized);
    view.last_pid = running_pid;
    to_value(view)
}

fn stop_instance(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = workbuddy_instance::get_default_workbuddy_user_data_dir()?;
        let default_dir_str = default_dir.to_string_lossy().to_string();
        let default_settings = workbuddy_instance::load_default_settings()?;
        if let Some(pid) = resolve_running_pid(default_settings.last_pid, None) {
            process::close_pid(pid, 20)?;
        }
        let _ = workbuddy_instance::update_default_pid(None)?;
        return to_value(InstanceProfileView {
            id: DEFAULT_INSTANCE_ID.to_string(),
            name: String::new(),
            user_data_dir: default_dir_str,
            working_dir: None,
            extra_args: default_settings.extra_args,
            bind_account_id: default_settings.bind_account_id,
            created_at: 0,
            last_launched_at: None,
            last_pid: None,
            running: false,
            initialized: is_profile_initialized(&default_dir.to_string_lossy()),
            is_default: true,
            follow_local_account: false,
        });
    }

    let store = workbuddy_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;
    if let Some(pid) = resolve_running_pid(instance.last_pid, Some(&instance.user_data_dir)) {
        process::close_pid(pid, 20)?;
    }
    let updated = workbuddy_instance::update_instance_pid(&instance.id, None)?;
    let initialized = is_profile_initialized(&updated.user_data_dir);
    to_value(InstanceProfileView::from_profile(
        updated,
        false,
        initialized,
    ))
}

fn open_instance_window(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_settings: DefaultInstanceSettings =
            workbuddy_instance::load_default_settings()?;
        let pid = resolve_running_pid(default_settings.last_pid, None).ok_or("默认实例未运行")?;
        process::focus_process_pid(pid)
            .map_err(|error| format!("定位 WorkBuddy 默认实例窗口失败：{}", error))?;
        return Ok(Value::Null);
    }

    let store = workbuddy_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;
    let pid = resolve_running_pid(instance.last_pid, Some(&instance.user_data_dir))
        .ok_or("实例未运行")?;
    process::focus_process_pid(pid).map_err(|error| {
        format!(
            "定位 WorkBuddy 实例窗口失败：instance_id={}, err={}",
            instance.id, error
        )
    })?;
    Ok(Value::Null)
}

fn close_all_instances() -> Result<Value, String> {
    let store = workbuddy_instance::load_instance_store()?;
    let default_settings = workbuddy_instance::load_default_settings()?;
    if let Some(pid) = resolve_running_pid(default_settings.last_pid, None) {
        let _ = process::close_pid(pid, 20);
    }
    for instance in &store.instances {
        if let Some(pid) = resolve_running_pid(instance.last_pid, Some(&instance.user_data_dir)) {
            let _ = process::close_pid(pid, 20);
        }
    }
    let _ = workbuddy_instance::clear_all_pids();
    Ok(Value::Null)
}

fn switch_inject(payload: Value) -> Result<Value, String> {
    let payload: AccountIdPayload = parse_payload(payload)?;
    let account = workbuddy_account::load_account(&payload.account_id)
        .ok_or_else(|| format!("WorkBuddy account not found: {}", payload.account_id))?;
    workbuddy_account::write_account_to_default_client(&account)?;
    let _ = workbuddy_instance::update_default_settings(
        Some(Some(payload.account_id.clone())),
        None,
        Some(false),
    )?;
    let _ = account_store::set_current_account_id(PLATFORM_ID, Some(&payload.account_id));

    match start_instance(json!({ "instanceId": DEFAULT_INSTANCE_ID })) {
        Ok(_) => to_value(SwitchResult {
            message: format!("切换完成：{}", account.email),
            restart_error: None,
            path_missing: false,
        }),
        Err(error) if path_missing_error(&error) => to_value(SwitchResult {
            message: format!("切换完成，但 WorkBuddy 启动失败：{}", error),
            restart_error: Some(error),
            path_missing: true,
        }),
        Err(error) => Err(error),
    }
}

fn inject_default_profile(payload: Value) -> Result<Value, String> {
    let payload: AccountIdPayload = parse_payload(payload)?;
    workbuddy_account::sync_account_to_default_client(&payload.account_id)?;
    Ok(Value::Null)
}

fn refresh_account(runtime: &Runtime, payload: Value) -> Result<Value, String> {
    let payload: AccountIdPayload = parse_payload(payload)?;
    let account = runtime.block_on(workbuddy_account::refresh_account_token(
        &payload.account_id,
    ))?;
    let _ = workbuddy_account::run_quota_alert_if_needed();
    to_value(account)
}

fn refresh_all_accounts(runtime: &Runtime) -> Result<Value, String> {
    let results = runtime.block_on(workbuddy_account::refresh_all_tokens())?;
    let success_count = results.iter().filter(|(_, result)| result.is_ok()).count() as i32;
    if success_count > 0 {
        let _ = workbuddy_account::run_quota_alert_if_needed();
    }
    to_value(success_count)
}

fn get_checkin_status(runtime: &Runtime, payload: Value) -> Result<Value, String> {
    let payload: AccountIdPayload = parse_payload(payload)?;
    let account = workbuddy_account::load_account(&payload.account_id)
        .ok_or_else(|| format!("账号不存在: {}", payload.account_id))?;
    to_value(runtime.block_on(codebuddy_cn_oauth::get_checkin_status(
        &account.access_token,
        account.uid.as_deref(),
        account.enterprise_id.as_deref(),
        account.domain.as_deref(),
    ))?)
}

fn perform_checkin(runtime: &Runtime, payload: Value) -> Result<Value, String> {
    let payload: AccountIdPayload = parse_payload(payload)?;
    let account = workbuddy_account::load_account(&payload.account_id)
        .ok_or_else(|| format!("账号不存在: {}", payload.account_id))?;
    let response = runtime.block_on(codebuddy_cn_oauth::perform_checkin(
        &account.access_token,
        account.uid.as_deref(),
        account.enterprise_id.as_deref(),
        account.domain.as_deref(),
    ))?;

    if response.success {
        let now = chrono::Utc::now().timestamp();
        let streak = account.checkin_streak.unwrap_or(0).saturating_add(1);
        workbuddy_account::update_checkin_info(
            &payload.account_id,
            Some(now),
            streak,
            response.reward.clone(),
        )?;
    }

    to_value(response)
}

fn handle_rpc(runtime: &Runtime, request: RpcRequest) -> Result<Value, String> {
    match request.method.as_str() {
        "health.check" => Ok(json!({ "status": "ok" })),
        "adapter.shutdown" => Ok(Value::Null),
        "accounts.list" => to_value(workbuddy_account::list_accounts_checked()?),
        "accounts.current" => to_value(workbuddy_account::resolve_current_account_id(
            &workbuddy_account::list_accounts_checked()?,
        )),
        "accounts.delete" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            workbuddy_account::remove_account(&payload.account_id)?;
            Ok(Value::Null)
        }
        "accounts.deleteMany" => {
            let payload: AccountIdsPayload = parse_payload(request.payload)?;
            workbuddy_account::remove_accounts(&payload.account_ids)?;
            Ok(Value::Null)
        }
        "accounts.importJson" => {
            let payload: JsonImportPayload = parse_payload(request.payload)?;
            to_value(workbuddy_account::import_from_json(&payload.json_content)?)
        }
        "accounts.importLocal" => import_local(runtime),
        "accounts.addToken" => add_with_token(runtime, request.payload),
        "accounts.export" => {
            let payload: AccountIdsPayload = parse_payload(request.payload)?;
            to_value(workbuddy_account::export_accounts(&payload.account_ids)?)
        }
        "accounts.indexPath" => to_value(workbuddy_account::accounts_index_path_string()?),
        "accounts.refresh" => refresh_account(runtime, request.payload),
        "accounts.refreshAll" => refresh_all_accounts(runtime),
        "accounts.syncToCodebuddyCn" => {
            to_value(workbuddy_account::sync_accounts_to_codebuddy_cn()?)
        }
        "accounts.updateTags" => {
            let payload: TagsPayload = parse_payload(request.payload)?;
            to_value(workbuddy_account::update_account_tags(
                &payload.account_id,
                payload.tags,
            )?)
        }
        "quota.alertPayload" => Ok(Value::Null),
        "oauth.start" => to_value(runtime.block_on(workbuddy_oauth::start_login())?),
        "oauth.complete" => complete_oauth(runtime, request.payload),
        "oauth.cancel" => {
            let payload: LoginCancelPayload = parse_payload(request.payload)?;
            workbuddy_oauth::cancel_login(payload.login_id.as_deref())?;
            Ok(Value::Null)
        }
        "oauth.restorePendingListener" => Ok(Value::Null),
        "switch.inject" => switch_inject(request.payload),
        "switch.injectDefaultProfile" => inject_default_profile(request.payload),
        "checkin.status" => get_checkin_status(runtime, request.payload),
        "checkin.perform" => perform_checkin(runtime, request.payload),
        "instances.store.get" => to_value(workbuddy_instance::load_instance_store()?),
        "instances.store.replace" => {
            let payload: InstanceStorePayload = parse_payload(request.payload)?;
            let store = sanitize_instance_store(&payload.store);
            workbuddy_instance::save_instance_store(&store)?;
            Ok(Value::Null)
        }
        "instance.getDefaults" => to_value(workbuddy_instance::get_instance_defaults()?),
        "instance.list" => to_value(list_instances()?),
        "instance.create" => create_instance(request.payload),
        "instance.update" => update_instance(request.payload),
        "instance.delete" => delete_instance(request.payload),
        "instance.start" => start_instance(request.payload),
        "instance.stop" => stop_instance(request.payload),
        "instance.closeAll" => close_all_instances(),
        "instance.openWindow" => open_instance_window(request.payload),
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
        other => Err(format!("未知 WorkBuddy adapter 方法: {}", other)),
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
            "error": { "message": format!("序列化 WorkBuddy adapter HTTP 响应失败: {}", error) }
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
            error_response("WorkBuddy adapter 路由不存在".to_string()),
        );
        return;
    }
    if !is_authorized(&request, token) {
        write_json_response(
            request,
            401,
            error_response("WorkBuddy adapter token 无效".to_string()),
        );
        return;
    }

    let mut body = String::new();
    if let Err(error) = request.as_reader().read_to_string(&mut body) {
        write_json_response(
            request,
            400,
            error_response(format!("读取 WorkBuddy adapter 请求失败: {}", error)),
        );
        return;
    }

    let rpc_request = match serde_json::from_str::<RpcRequest>(&body) {
        Ok(value) => value,
        Err(error) => {
            write_json_response(
                request,
                400,
                error_response(format!("解析 WorkBuddy adapter 请求 JSON 失败: {}", error)),
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
    let server = Server::http("127.0.0.1:0").expect("bind workbuddy adapter server");
    let address = server.server_addr().to_string();
    let port = address
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .expect("parse workbuddy adapter port");
    let token = Uuid::new_v4().simple().to_string();
    let shutdown = Arc::new(AtomicBool::new(false));

    println!(
        "{}",
        json!({
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
