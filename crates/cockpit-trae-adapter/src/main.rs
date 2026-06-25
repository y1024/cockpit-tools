use cockpit_core::models::trae::{TraeAccount, TraeImportPayload};
use cockpit_core::models::{DefaultInstanceSettings, InstanceProfileView, InstanceStore};
use cockpit_core::modules::{process, trae_account, trae_instance, trae_oauth};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
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
    serde_json::to_value(value).map_err(|error| format!("序列化 Trae adapter 响应失败: {}", error))
}

fn parse_payload<T: for<'de> Deserialize<'de>>(payload: Value) -> Result<T, String> {
    serde_json::from_value(payload)
        .map_err(|error| format!("解析 Trae adapter 请求失败: {}", error))
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
    error.starts_with("APP_PATH_NOT_FOUND:") || error.contains("启动 Trae 失败")
}

fn resolve_running_pid(last_pid: Option<u32>) -> Option<u32> {
    let pid = last_pid?;
    if process::is_pid_running(pid) {
        Some(pid)
    } else {
        None
    }
}

fn default_instance_view(
    default_dir: &Path,
    settings: DefaultInstanceSettings,
) -> InstanceProfileView {
    let default_dir_str = default_dir.to_string_lossy().to_string();
    let running_pid = resolve_running_pid(settings.last_pid);

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

async fn refresh_trae_account_after_login(account: TraeAccount) -> TraeAccount {
    let account_id = account.id.clone();
    match trae_account::refresh_account_async(&account_id).await {
        Ok(refreshed) => refreshed,
        Err(_) => account,
    }
}

fn import_local(runtime: &Runtime) -> Result<Value, String> {
    match trae_account::import_from_local()? {
        Some(mut account) => {
            if let Ok(refreshed) =
                runtime.block_on(trae_account::refresh_account_async(&account.id))
            {
                account = refreshed;
            }
            to_value(vec![account])
        }
        None => Err("未找到本地 Trae 登录信息".to_string()),
    }
}

fn add_with_token(payload: Value) -> Result<Value, String> {
    let payload: TokenPayload = parse_payload(payload)?;
    let account_payload = TraeImportPayload {
        email: "unknown".to_string(),
        user_id: None,
        nickname: None,
        access_token: payload.access_token,
        refresh_token: None,
        token_type: None,
        expires_at: None,
        plan_type: None,
        plan_reset_at: None,
        trae_auth_raw: None,
        trae_profile_raw: None,
        trae_entitlement_raw: None,
        trae_usage_raw: None,
        trae_server_raw: None,
        trae_usertag_raw: None,
        status: None,
        status_reason: None,
    };
    to_value(trae_account::upsert_account(account_payload)?)
}

fn complete_oauth(runtime: &Runtime, payload: Value) -> Result<Value, String> {
    let payload: LoginIdPayload = parse_payload(payload)?;
    let account_payload = runtime.block_on(trae_oauth::complete_login(&payload.login_id))?;
    let account = trae_account::upsert_account(account_payload)?;
    to_value(runtime.block_on(refresh_trae_account_after_login(account)))
}

fn inject_bound_account_for_instance_start(
    runtime: &Runtime,
    user_data_dir: &str,
    bind_account_id: Option<&str>,
) -> Result<(), String> {
    let Some(bind_id) = bind_account_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    let account = runtime
        .block_on(trae_account::refresh_account_async(bind_id))
        .map_err(|err| format!("Trae 实例启动前刷新账号失败({}): {}", bind_id, err))?;
    let storage_path = trae_instance::build_storage_json_path(user_data_dir);
    trae_account::inject_to_trae_at_path(storage_path.as_path(), bind_id)?;
    eprintln!(
        "[TraeAdapter] injected bound account: account_id={}, email={}, user_data_dir={}",
        bind_id, account.email, user_data_dir
    );
    Ok(())
}

fn verify_bound_account_after_start(
    runtime: &Runtime,
    user_data_dir: &str,
    bind_account_id: Option<&str>,
) {
    let Some(account_id) = bind_account_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };

    match runtime.block_on(trae_account::check_login_then_refresh_if_needed(account_id)) {
        Ok(true) => {
            let storage_path = trae_instance::build_storage_json_path(user_data_dir);
            if let Err(err) =
                trae_account::inject_to_trae_at_path(storage_path.as_path(), account_id)
            {
                eprintln!(
                    "[TraeAdapter] refreshed account after start but failed to write profile: account_id={}, error={}",
                    account_id, err
                );
            }
        }
        Ok(false) => {}
        Err(err) => {
            eprintln!(
                "[TraeAdapter] strict account check after start failed: account_id={}, error={}",
                account_id, err
            );
        }
    }
}

fn list_instances() -> Result<Vec<InstanceProfileView>, String> {
    let store = trae_instance::load_instance_store()?;
    let default_dir = trae_instance::get_default_trae_user_data_dir()?;
    let default_dir_str = default_dir.to_string_lossy().to_string();
    let default_settings = store.default_settings.clone();

    let mut result: Vec<InstanceProfileView> = store
        .instances
        .into_iter()
        .map(|instance| {
            let resolved_pid = resolve_running_pid(instance.last_pid);
            let running = resolved_pid.is_some();
            let initialized = is_profile_initialized(&instance.user_data_dir);
            let mut view = InstanceProfileView::from_profile(instance, running, initialized);
            view.last_pid = resolved_pid;
            view
        })
        .collect();

    let default_pid = resolve_running_pid(default_settings.last_pid);
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
    let instance = trae_instance::create_instance(trae_instance::CreateInstanceParams {
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
        let default_dir = trae_instance::get_default_trae_user_data_dir()?;
        let updated = trae_instance::update_default_settings(
            payload.bind_account_id,
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
        let store = trae_instance::load_instance_store()?;
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

    let instance = trae_instance::update_instance(trae_instance::UpdateInstanceParams {
        working_dir: None,
        instance_id: payload.instance_id,
        name: payload.name,
        extra_args: payload.extra_args,
        bind_account_id: payload.bind_account_id,
    })?;
    let running_pid = resolve_running_pid(instance.last_pid);
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
    trae_instance::delete_instance(&payload.instance_id)?;
    Ok(Value::Null)
}

fn start_instance(runtime: &Runtime, payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    process::ensure_trae_launch_path_configured()?;

    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = trae_instance::get_default_trae_user_data_dir()?;
        let default_dir_str = default_dir.to_string_lossy().to_string();
        let default_settings = trae_instance::load_default_settings()?;
        if let Some(pid) = resolve_running_pid(default_settings.last_pid) {
            process::close_pid(pid, 20)?;
            let _ = trae_instance::update_default_pid(None)?;
        }
        inject_bound_account_for_instance_start(
            runtime,
            &default_dir_str,
            default_settings.bind_account_id.as_deref(),
        )?;
        let extra_args = process::parse_extra_args(&default_settings.extra_args);
        let pid = process::start_trae_default_with_args_with_new_window(&extra_args, true)?;
        let _ = trae_instance::update_default_pid(Some(pid))?;
        verify_bound_account_after_start(
            runtime,
            &default_dir_str,
            default_settings.bind_account_id.as_deref(),
        );
        let running_pid = resolve_running_pid(Some(pid));

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

    let store = trae_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;

    if let Some(pid) = resolve_running_pid(instance.last_pid) {
        process::close_pid(pid, 20)?;
        let _ = trae_instance::update_instance_pid(&instance.id, None)?;
    }
    inject_bound_account_for_instance_start(
        runtime,
        &instance.user_data_dir,
        instance.bind_account_id.as_deref(),
    )?;
    let extra_args = process::parse_extra_args(&instance.extra_args);
    let pid =
        process::start_trae_with_args_with_new_window(&instance.user_data_dir, &extra_args, true)?;
    verify_bound_account_after_start(
        runtime,
        &instance.user_data_dir,
        instance.bind_account_id.as_deref(),
    );
    let updated = trae_instance::update_instance_after_start(&instance.id, pid)?;
    let running_pid = resolve_running_pid(Some(pid));
    let initialized = is_profile_initialized(&updated.user_data_dir);
    let mut view = InstanceProfileView::from_profile(updated, running_pid.is_some(), initialized);
    view.last_pid = running_pid;
    to_value(view)
}

fn stop_instance(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = trae_instance::get_default_trae_user_data_dir()?;
        let default_dir_str = default_dir.to_string_lossy().to_string();
        let default_settings = trae_instance::load_default_settings()?;
        if let Some(pid) = resolve_running_pid(default_settings.last_pid) {
            process::close_pid(pid, 20)?;
        }
        let updated = trae_instance::update_default_pid(None)?;
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
            running: resolve_running_pid(updated.last_pid).is_some(),
            initialized: is_profile_initialized(&default_dir.to_string_lossy()),
            is_default: true,
            follow_local_account: false,
        });
    }

    let store = trae_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;
    if let Some(pid) = resolve_running_pid(instance.last_pid) {
        process::close_pid(pid, 20)?;
    }
    let updated = trae_instance::update_instance_pid(&instance.id, None)?;
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
        let default_settings: DefaultInstanceSettings = trae_instance::load_default_settings()?;
        let pid = resolve_running_pid(default_settings.last_pid).ok_or("默认实例未运行")?;
        process::focus_process_pid(pid)
            .map_err(|err| format!("定位 Trae 默认实例窗口失败: {}", err))?;
        return Ok(Value::Null);
    }

    let store = trae_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;
    let pid = resolve_running_pid(instance.last_pid).ok_or("实例未运行")?;
    process::focus_process_pid(pid).map_err(|err| {
        format!(
            "定位 Trae 实例窗口失败: instance_id={}, err={}",
            instance.id, err
        )
    })?;
    Ok(Value::Null)
}

fn close_all_instances() -> Result<Value, String> {
    let store = trae_instance::load_instance_store()?;
    let default_settings = trae_instance::load_default_settings()?;
    if let Some(pid) = resolve_running_pid(default_settings.last_pid) {
        let _ = process::close_pid(pid, 20);
    }
    for instance in &store.instances {
        if let Some(pid) = resolve_running_pid(instance.last_pid) {
            let _ = process::close_pid(pid, 20);
        }
    }
    let _ = trae_instance::clear_all_pids();
    Ok(Value::Null)
}

fn switch_inject(runtime: &Runtime, payload: Value) -> Result<Value, String> {
    let payload: AccountIdPayload = parse_payload(payload)?;
    let _ = trae_account::load_account(&payload.account_id)
        .ok_or_else(|| format!("Trae account not found: {}", payload.account_id))?;
    let account = runtime
        .block_on(trae_account::refresh_account_async(&payload.account_id))
        .map_err(|err| format!("Trae 切号前刷新失败: {}", err))?;
    if let Err(err) = process::close_trae(20) {
        return Err(format!(
            "Trae 正在运行且未能正常关闭（{}）。请先关闭 Trae 后重试切号。",
            err
        ));
    }
    trae_account::inject_to_trae(&payload.account_id)?;
    let _ = trae_instance::update_default_settings(
        Some(Some(payload.account_id.clone())),
        None,
        Some(false),
    )?;
    match start_instance(runtime, json!({ "instanceId": DEFAULT_INSTANCE_ID })) {
        Ok(_) => to_value(SwitchResult {
            message: format!("切换完成: {}", account.email),
            restart_error: None,
            path_missing: false,
        }),
        Err(error) if path_missing_error(&error) => to_value(SwitchResult {
            message: format!("切换完成，但 Trae 启动失败: {}", error),
            restart_error: Some(error),
            path_missing: true,
        }),
        Err(error) => Err(error),
    }
}

fn inject_default_profile(payload: Value) -> Result<Value, String> {
    let payload: AccountIdPayload = parse_payload(payload)?;
    if process::is_trae_running() {
        eprintln!(
            "[TraeAdapter] skipped default profile injection because Trae is running: account_id={}",
            payload.account_id
        );
        return Ok(Value::Null);
    }
    let default_dir = trae_instance::get_default_trae_user_data_dir()?;
    let default_dir_str = default_dir.to_string_lossy().to_string();
    let storage_path = trae_instance::build_storage_json_path(&default_dir_str);
    trae_account::inject_to_trae_at_path(storage_path.as_path(), &payload.account_id)?;
    Ok(Value::Null)
}

fn should_refresh_token(payload: Value) -> Result<Value, String> {
    let payload: AccountIdPayload = parse_payload(payload)?;
    let account = trae_account::load_account(&payload.account_id)
        .ok_or_else(|| format!("Trae account not found: {}", payload.account_id))?;
    to_value(trae_account::should_refresh_token_by_official_window(
        &account,
    ))
}

fn check_login(runtime: &Runtime, payload: Value) -> Result<Value, String> {
    let payload: AccountIdPayload = parse_payload(payload)?;
    to_value(runtime.block_on(trae_account::check_login_token(&payload.account_id))?)
}

fn refresh_account_with_protection(runtime: &Runtime, payload: Value) -> Result<Value, String> {
    let payload: AccountIdPayload = parse_payload(payload)?;
    if let Ok(accounts) = trae_account::list_accounts_checked() {
        let protection_map =
            trae_account::resolve_running_account_refresh_protection_map(&accounts);
        if let Some(storage_path) = protection_map.get(payload.account_id.as_str()) {
            let account = runtime.block_on(trae_account::refresh_account_usage_only_async(
                &payload.account_id,
                storage_path.as_deref(),
            ))?;
            return to_value(account);
        }
    }

    let account = runtime.block_on(trae_account::refresh_account_async(&payload.account_id))?;
    to_value(account)
}

fn handle_rpc(runtime: &Runtime, request: RpcRequest) -> Result<Value, String> {
    match request.method.as_str() {
        "health.check" => Ok(json!({ "status": "ok" })),
        "adapter.shutdown" => Ok(Value::Null),
        "accounts.list" => to_value(trae_account::list_accounts_checked()?),
        "accounts.current" => to_value(trae_account::resolve_current_account_id(
            &trae_account::list_accounts_checked()?,
        )),
        "accounts.delete" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            trae_account::remove_account(&payload.account_id)?;
            Ok(Value::Null)
        }
        "accounts.deleteMany" => {
            let payload: AccountIdsPayload = parse_payload(request.payload)?;
            trae_account::remove_accounts(&payload.account_ids)?;
            Ok(Value::Null)
        }
        "accounts.importJson" => {
            let payload: JsonImportPayload = parse_payload(request.payload)?;
            to_value(trae_account::import_from_json(&payload.json_content)?)
        }
        "accounts.importLocal" => import_local(runtime),
        "accounts.addToken" => add_with_token(request.payload),
        "accounts.export" => {
            let payload: AccountIdsPayload = parse_payload(request.payload)?;
            to_value(trae_account::export_accounts(&payload.account_ids)?)
        }
        "accounts.indexPath" => to_value(trae_account::accounts_index_path_string()?),
        "accounts.shouldRefreshToken" => should_refresh_token(request.payload),
        "accounts.checkLogin" => check_login(runtime, request.payload),
        "accounts.refresh" => refresh_account_with_protection(runtime, request.payload),
        "accounts.refreshAll" => {
            let results = runtime.block_on(trae_account::refresh_all_tokens())?;
            let success_count = results.iter().filter(|(_, item)| item.is_ok()).count();
            to_value(success_count as i32)
        }
        "accounts.updateTags" => {
            let payload: TagsPayload = parse_payload(request.payload)?;
            to_value(trae_account::update_account_tags(
                &payload.account_id,
                payload.tags,
            )?)
        }
        "quota.alertPayload" => Ok(Value::Null),
        "oauth.start" => to_value(runtime.block_on(trae_oauth::start_login())?),
        "oauth.complete" => complete_oauth(runtime, request.payload),
        "oauth.cancel" => {
            let payload: LoginCancelPayload = parse_payload(request.payload)?;
            trae_oauth::cancel_login(payload.login_id.as_deref())?;
            Ok(Value::Null)
        }
        "oauth.submitCallbackUrl" => {
            let payload: CallbackUrlPayload = parse_payload(request.payload)?;
            trae_oauth::submit_callback_url(&payload.login_id, &payload.callback_url)?;
            Ok(Value::Null)
        }
        "oauth.restorePendingListener" => {
            trae_oauth::restore_pending_oauth_listener();
            Ok(Value::Null)
        }
        "switch.inject" => switch_inject(runtime, request.payload),
        "switch.injectDefaultProfile" => inject_default_profile(request.payload),
        "instances.store.get" => to_value(trae_instance::load_instance_store()?),
        "instances.store.replace" => {
            let payload: InstanceStorePayload = parse_payload(request.payload)?;
            let store = sanitize_instance_store(&payload.store);
            trae_instance::save_instance_store(&store)?;
            Ok(Value::Null)
        }
        "instance.getDefaults" => to_value(trae_instance::get_instance_defaults()?),
        "instance.list" => to_value(list_instances()?),
        "instance.create" => create_instance(request.payload),
        "instance.update" => update_instance(request.payload),
        "instance.delete" => delete_instance(request.payload),
        "instance.start" => start_instance(runtime, request.payload),
        "instance.stop" => stop_instance(request.payload),
        "instance.closeAll" => close_all_instances(),
        "instance.openWindow" => open_instance_window(request.payload),
        "runtime.status" => to_value(list_instances()?),
        "runtime.startDefault" => {
            start_instance(runtime, json!({ "instanceId": DEFAULT_INSTANCE_ID }))
        }
        "runtime.stopDefault" => stop_instance(json!({ "instanceId": DEFAULT_INSTANCE_ID })),
        "runtime.restartDefault" => {
            let _ = stop_instance(json!({ "instanceId": DEFAULT_INSTANCE_ID }));
            start_instance(runtime, json!({ "instanceId": DEFAULT_INSTANCE_ID }))
        }
        "runtime.focusDefault" => {
            open_instance_window(json!({ "instanceId": DEFAULT_INSTANCE_ID }))
        }
        other => Err(format!("未知 Trae adapter 方法: {}", other)),
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
            "error": { "message": format!("序列化 Trae adapter HTTP 响应失败: {}", error) }
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
            error_response("Trae adapter 路由不存在".to_string()),
        );
        return;
    }
    if !is_authorized(&request, token) {
        write_json_response(
            request,
            401,
            error_response("Trae adapter token 无效".to_string()),
        );
        return;
    }

    let mut body = String::new();
    if let Err(error) = request.as_reader().read_to_string(&mut body) {
        write_json_response(
            request,
            400,
            error_response(format!("读取 Trae adapter 请求失败: {}", error)),
        );
        return;
    }

    let rpc_request = match serde_json::from_str::<RpcRequest>(&body) {
        Ok(value) => value,
        Err(error) => {
            write_json_response(
                request,
                400,
                error_response(format!("解析 Trae adapter 请求 JSON 失败: {}", error)),
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
    let server = Server::http("127.0.0.1:0").expect("bind trae adapter server");
    let address = server.server_addr().to_string();
    let port = address
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .expect("parse trae adapter port");
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
