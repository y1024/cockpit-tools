use cockpit_core::models::{DefaultInstanceSettings, InstanceProfileView, InstanceStore};
use cockpit_core::modules::{kiro_account, kiro_instance, kiro_oauth, logger, process};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
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
struct DetectLaunchPathPayload {
    force: Option<bool>,
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
    serde_json::to_value(value).map_err(|error| format!("序列化 Kiro adapter 响应失败: {}", error))
}

fn parse_payload<T: for<'de> Deserialize<'de>>(payload: Value) -> Result<T, String> {
    serde_json::from_value(payload)
        .map_err(|error| format!("解析 Kiro adapter 请求失败: {}", error))
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
    error.starts_with("APP_PATH_NOT_FOUND:") || error.contains("启动 Kiro 失败")
}

fn default_instance_view(
    default_dir: &Path,
    settings: DefaultInstanceSettings,
) -> InstanceProfileView {
    let default_dir_str = default_dir.to_string_lossy().to_string();
    let running = settings
        .last_pid
        .and_then(|pid| kiro_instance::resolve_kiro_pid(Some(pid), None))
        .is_some();

    InstanceProfileView {
        id: DEFAULT_INSTANCE_ID.to_string(),
        name: String::new(),
        user_data_dir: default_dir_str,
        working_dir: None,
        extra_args: settings.extra_args,
        bind_account_id: settings.bind_account_id,
        created_at: 0,
        last_launched_at: None,
        last_pid: settings.last_pid,
        running,
        initialized: is_profile_initialized(&default_dir.to_string_lossy()),
        is_default: true,
        follow_local_account: false,
    }
}

async fn refresh_kiro_account_after_login(
    account: cockpit_core::models::kiro::KiroAccount,
) -> cockpit_core::models::kiro::KiroAccount {
    let account_id = account.id.clone();
    match kiro_account::refresh_account_token(&account_id).await {
        Ok(refreshed) => refreshed,
        Err(_) => account,
    }
}

fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn kiro_account_expires_soon(account: &cockpit_core::models::kiro::KiroAccount) -> bool {
    account
        .expires_at
        .map(|value| value <= current_unix_timestamp() + 5 * 60)
        .unwrap_or(true)
}

fn keepalive_due_kiro_accounts(runtime: &Runtime) -> Result<i32, String> {
    let accounts = kiro_account::list_accounts_checked()?;
    let current_id = kiro_account::resolve_current_account_id(&accounts);
    let mut refreshed = 0i32;

    for account in accounts {
        if !kiro_account_expires_soon(&account) {
            continue;
        }

        match runtime.block_on(kiro_account::refresh_account_token(&account.id)) {
            Ok(updated) => {
                if current_id.as_deref() == Some(updated.id.as_str()) {
                    match kiro_instance::get_default_kiro_user_data_dir() {
                        Ok(user_data_dir) => {
                            if let Err(err) = kiro_instance::inject_account_to_profile(
                                user_data_dir.as_path(),
                                &updated.id,
                            ) {
                                logger::log_warn(&format!(
                                    "[TokenKeeper][Kiro] 当前本地登录回写失败: account_id={}, error={}",
                                    updated.id, err
                                ));
                            }
                        }
                        Err(err) => {
                            logger::log_warn(&format!(
                                "[TokenKeeper][Kiro] 获取默认用户目录失败，跳过本地回写: {}",
                                err
                            ));
                        }
                    }
                }
                refreshed += 1;
                logger::log_info(&format!(
                    "[TokenKeeper][Kiro] Token 保活成功: account_id={}, email={}",
                    updated.id, updated.email
                ));
            }
            Err(err) => {
                logger::log_warn(&format!(
                    "[TokenKeeper][Kiro] Token 保活失败: account_id={}, error={}",
                    account.id, err
                ));
            }
        }
    }

    Ok(refreshed)
}

fn import_local(runtime: &Runtime) -> Result<Value, String> {
    let payload = kiro_oauth::build_payload_from_local_files()?;
    let payload = runtime.block_on(kiro_oauth::enrich_payload_with_runtime_usage(payload));
    to_value(vec![kiro_account::upsert_account(payload)?])
}

fn add_with_token(runtime: &Runtime, payload: Value) -> Result<Value, String> {
    let payload: TokenPayload = parse_payload(payload)?;
    let account_payload =
        runtime.block_on(kiro_oauth::build_payload_from_token(&payload.access_token))?;
    to_value(kiro_account::upsert_account(account_payload)?)
}

fn complete_oauth(runtime: &Runtime, payload: Value) -> Result<Value, String> {
    let payload: LoginIdPayload = parse_payload(payload)?;
    let account_payload = runtime.block_on(kiro_oauth::complete_login(&payload.login_id))?;
    let account = kiro_account::upsert_account(account_payload)?;
    to_value(runtime.block_on(refresh_kiro_account_after_login(account)))
}

fn inject_bound_account_for_instance_start(
    user_data_dir: &str,
    bind_account_id: Option<&str>,
) -> Result<(), String> {
    let Some(bind_id) = bind_account_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    let _account = kiro_account::load_account(bind_id)
        .ok_or_else(|| format!("绑定账号不存在: {}", bind_id))?;
    kiro_instance::close_kiro(&[user_data_dir.to_string()], 20)?;
    kiro_instance::inject_account_to_profile(Path::new(user_data_dir), bind_id)
}

fn list_instances() -> Result<Vec<InstanceProfileView>, String> {
    let store = kiro_instance::load_instance_store()?;
    let default_dir = kiro_instance::get_default_kiro_user_data_dir()?;
    let default_dir_str = default_dir.to_string_lossy().to_string();
    let default_settings = store.default_settings.clone();
    let process_entries = kiro_instance::collect_kiro_process_entries();

    let mut result: Vec<InstanceProfileView> = store
        .instances
        .into_iter()
        .map(|instance| {
            let resolved_pid = kiro_instance::resolve_kiro_pid_from_entries(
                instance.last_pid,
                Some(&instance.user_data_dir),
                &process_entries,
            );
            let running = resolved_pid.is_some();
            let initialized = is_profile_initialized(&instance.user_data_dir);
            let mut view = InstanceProfileView::from_profile(instance, running, initialized);
            view.last_pid = resolved_pid;
            view
        })
        .collect();

    let default_pid = kiro_instance::resolve_kiro_pid_from_entries(
        default_settings.last_pid,
        None,
        &process_entries,
    );
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
    let instance = kiro_instance::create_instance(kiro_instance::CreateInstanceParams {
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
        let default_dir = kiro_instance::get_default_kiro_user_data_dir()?;
        let updated = kiro_instance::update_default_settings(
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
        let store = kiro_instance::load_instance_store()?;
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

    let instance = kiro_instance::update_instance(kiro_instance::UpdateInstanceParams {
        working_dir: None,
        instance_id: payload.instance_id,
        name: payload.name,
        extra_args: payload.extra_args,
        bind_account_id: payload.bind_account_id,
    })?;
    let running = instance
        .last_pid
        .and_then(|pid| kiro_instance::resolve_kiro_pid(Some(pid), Some(&instance.user_data_dir)))
        .is_some();
    let initialized = is_profile_initialized(&instance.user_data_dir);
    to_value(InstanceProfileView::from_profile(
        instance,
        running,
        initialized,
    ))
}

fn delete_instance(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        return Err("默认实例不可删除".to_string());
    }
    kiro_instance::delete_instance(&payload.instance_id)?;
    Ok(Value::Null)
}

fn start_instance(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    kiro_instance::ensure_kiro_launch_path_configured()?;

    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = kiro_instance::get_default_kiro_user_data_dir()?;
        let default_dir_str = default_dir.to_string_lossy().to_string();
        let default_settings = kiro_instance::load_default_settings()?;

        if let Some(pid) = kiro_instance::resolve_kiro_pid(default_settings.last_pid, None) {
            process::close_pid(pid, 20)?;
            let _ = kiro_instance::update_default_pid(None)?;
        }

        kiro_instance::close_kiro(&[default_dir_str.clone()], 20)?;
        inject_bound_account_for_instance_start(
            &default_dir_str,
            default_settings.bind_account_id.as_deref(),
        )?;

        let extra_args = process::parse_extra_args(&default_settings.extra_args);
        let pid = kiro_instance::start_kiro_default_with_args_with_new_window(&extra_args, true)?;
        let _ = kiro_instance::update_default_pid(Some(pid))?;

        return to_value(InstanceProfileView {
            id: DEFAULT_INSTANCE_ID.to_string(),
            name: String::new(),
            user_data_dir: default_dir_str,
            working_dir: None,
            extra_args: default_settings.extra_args,
            bind_account_id: default_settings.bind_account_id,
            created_at: 0,
            last_launched_at: None,
            last_pid: Some(pid),
            running: kiro_instance::resolve_kiro_pid(Some(pid), None).is_some(),
            initialized: is_profile_initialized(&default_dir.to_string_lossy()),
            is_default: true,
            follow_local_account: false,
        });
    }

    let store = kiro_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;

    if let Some(pid) =
        kiro_instance::resolve_kiro_pid(instance.last_pid, Some(&instance.user_data_dir))
    {
        process::close_pid(pid, 20)?;
        let _ = kiro_instance::update_instance_pid(&instance.id, None)?;
    }

    kiro_instance::close_kiro(&[instance.user_data_dir.clone()], 20)?;
    inject_bound_account_for_instance_start(
        &instance.user_data_dir,
        instance.bind_account_id.as_deref(),
    )?;

    let extra_args = process::parse_extra_args(&instance.extra_args);
    let pid = kiro_instance::start_kiro_with_args_with_new_window(
        &instance.user_data_dir,
        &extra_args,
        true,
    )?;
    let updated = kiro_instance::update_instance_after_start(&instance.id, pid)?;
    let running =
        kiro_instance::resolve_kiro_pid(Some(pid), Some(&updated.user_data_dir)).is_some();
    let initialized = is_profile_initialized(&updated.user_data_dir);
    to_value(InstanceProfileView::from_profile(
        updated,
        running,
        initialized,
    ))
}

fn stop_instance(payload: Value) -> Result<Value, String> {
    let payload: InstanceIdPayload = parse_payload(payload)?;
    if payload.instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = kiro_instance::get_default_kiro_user_data_dir()?;
        let default_settings = kiro_instance::load_default_settings()?;
        if let Some(pid) = kiro_instance::resolve_kiro_pid(default_settings.last_pid, None) {
            process::close_pid(pid, 20)?;
        }
        let updated = kiro_instance::update_default_pid(None)?;
        return to_value(default_instance_view(&default_dir, updated));
    }

    let store = kiro_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;
    if let Some(pid) =
        kiro_instance::resolve_kiro_pid(instance.last_pid, Some(&instance.user_data_dir))
    {
        process::close_pid(pid, 20)?;
    }
    let updated = kiro_instance::update_instance_pid(&instance.id, None)?;
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
        let default_settings: DefaultInstanceSettings = kiro_instance::load_default_settings()?;
        kiro_instance::focus_kiro_instance(default_settings.last_pid, None)
            .map_err(|err| format!("定位 Kiro 默认实例窗口失败: {}", err))?;
        return Ok(Value::Null);
    }

    let store = kiro_instance::load_instance_store()?;
    let instance = store
        .instances
        .into_iter()
        .find(|item| item.id == payload.instance_id)
        .ok_or("实例不存在")?;
    kiro_instance::focus_kiro_instance(instance.last_pid, Some(&instance.user_data_dir)).map_err(
        |err| {
            format!(
                "定位 Kiro 实例窗口失败: instance_id={}, err={}",
                instance.id, err
            )
        },
    )?;
    Ok(Value::Null)
}

fn close_all_instances() -> Result<Value, String> {
    let store = kiro_instance::load_instance_store()?;
    let default_dir = kiro_instance::get_default_kiro_user_data_dir()?;
    let mut target_dirs: Vec<String> = vec![default_dir.to_string_lossy().to_string()];
    for instance in &store.instances {
        let dir = instance.user_data_dir.trim();
        if !dir.is_empty() {
            target_dirs.push(dir.to_string());
        }
    }
    kiro_instance::close_kiro(&target_dirs, 20)?;
    let _ = kiro_instance::clear_all_pids();
    Ok(Value::Null)
}

fn switch_inject(payload: Value) -> Result<Value, String> {
    let payload: AccountIdPayload = parse_payload(payload)?;
    let account = kiro_account::load_account(&payload.account_id)
        .ok_or_else(|| format!("Kiro account not found: {}", payload.account_id))?;
    let _ = kiro_instance::update_default_settings(
        Some(Some(payload.account_id.clone())),
        None,
        Some(false),
    )?;
    match start_instance(json!({ "instanceId": DEFAULT_INSTANCE_ID })) {
        Ok(_) => to_value(SwitchResult {
            message: format!("切换完成: {}", account.email),
            restart_error: None,
            path_missing: false,
        }),
        Err(error) if path_missing_error(&error) => to_value(SwitchResult {
            message: format!("切换完成，但 Kiro 启动失败: {}", error),
            restart_error: Some(error),
            path_missing: true,
        }),
        Err(error) => Err(error),
    }
}

fn handle_rpc(runtime: &Runtime, request: RpcRequest) -> Result<Value, String> {
    match request.method.as_str() {
        "health.check" => Ok(json!({ "status": "ok" })),
        "adapter.shutdown" => Ok(Value::Null),
        "accounts.list" => to_value(kiro_account::list_accounts_checked()?),
        "accounts.current" => to_value(kiro_account::resolve_current_account_id(
            &kiro_account::list_accounts_checked()?,
        )),
        "accounts.delete" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            kiro_account::remove_account(&payload.account_id)?;
            Ok(Value::Null)
        }
        "accounts.deleteMany" => {
            let payload: AccountIdsPayload = parse_payload(request.payload)?;
            kiro_account::remove_accounts(&payload.account_ids)?;
            Ok(Value::Null)
        }
        "accounts.importJson" => {
            let payload: JsonImportPayload = parse_payload(request.payload)?;
            to_value(kiro_account::import_from_json(&payload.json_content)?)
        }
        "accounts.importLocal" => import_local(runtime),
        "accounts.addToken" => add_with_token(runtime, request.payload),
        "accounts.export" => {
            let payload: AccountIdsPayload = parse_payload(request.payload)?;
            to_value(kiro_account::export_accounts(&payload.account_ids)?)
        }
        "accounts.indexPath" => to_value(kiro_account::accounts_index_path_string()?),
        "accounts.keepaliveDue" => to_value(keepalive_due_kiro_accounts(runtime)?),
        "accounts.refresh" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            let account =
                runtime.block_on(kiro_account::refresh_account_token(&payload.account_id))?;
            let _ = kiro_account::run_quota_alert_if_needed();
            to_value(account)
        }
        "accounts.refreshAll" => {
            let results = runtime.block_on(kiro_account::refresh_all_tokens())?;
            let success_count = results.iter().filter(|(_, item)| item.is_ok()).count();
            if success_count > 0 {
                let _ = kiro_account::run_quota_alert_if_needed();
            }
            to_value(success_count as i32)
        }
        "accounts.updateTags" => {
            let payload: TagsPayload = parse_payload(request.payload)?;
            to_value(kiro_account::update_account_tags(
                &payload.account_id,
                payload.tags,
            )?)
        }
        "quota.alertPayload" => {
            let _ = kiro_account::run_quota_alert_if_needed();
            Ok(Value::Null)
        }
        "oauth.start" => to_value(runtime.block_on(kiro_oauth::start_login())?),
        "oauth.complete" => complete_oauth(runtime, request.payload),
        "oauth.cancel" => {
            let payload: LoginCancelPayload = parse_payload(request.payload)?;
            kiro_oauth::cancel_login(payload.login_id.as_deref())?;
            Ok(Value::Null)
        }
        "oauth.submitCallbackUrl" => {
            let payload: CallbackUrlPayload = parse_payload(request.payload)?;
            kiro_oauth::submit_callback_url(&payload.login_id, &payload.callback_url)?;
            Ok(Value::Null)
        }
        "oauth.restorePendingListener" => {
            kiro_oauth::restore_pending_oauth_listener();
            Ok(Value::Null)
        }
        "switch.inject" => switch_inject(request.payload),
        "instances.store.get" => to_value(kiro_instance::load_instance_store()?),
        "instances.store.replace" => {
            let payload: InstanceStorePayload = parse_payload(request.payload)?;
            let store = sanitize_instance_store(&payload.store);
            kiro_instance::save_instance_store(&store)?;
            Ok(Value::Null)
        }
        "instance.getDefaults" => to_value(kiro_instance::get_instance_defaults()?),
        "instance.list" => to_value(list_instances()?),
        "instance.create" => create_instance(request.payload),
        "instance.update" => update_instance(request.payload),
        "instance.delete" => delete_instance(request.payload),
        "instance.start" => start_instance(request.payload),
        "instance.stop" => stop_instance(request.payload),
        "instance.closeAll" => close_all_instances(),
        "instance.openWindow" => open_instance_window(request.payload),
        "runtime.detectLaunchPath" => {
            let payload: DetectLaunchPathPayload = parse_payload(request.payload)?;
            to_value(kiro_instance::detect_and_save_kiro_launch_path(
                payload.force.unwrap_or(false),
            ))
        }
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
        other => Err(format!("未知 Kiro adapter 方法: {}", other)),
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
            "error": { "message": format!("序列化 Kiro adapter HTTP 响应失败: {}", error) }
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
            error_response("Kiro adapter 路由不存在".to_string()),
        );
        return;
    }
    if !is_authorized(&request, token) {
        write_json_response(
            request,
            401,
            error_response("Kiro adapter token 无效".to_string()),
        );
        return;
    }

    let mut body = String::new();
    if let Err(error) = request.as_reader().read_to_string(&mut body) {
        write_json_response(
            request,
            400,
            error_response(format!("读取 Kiro adapter 请求失败: {}", error)),
        );
        return;
    }

    let rpc_request = match serde_json::from_str::<RpcRequest>(&body) {
        Ok(value) => value,
        Err(error) => {
            write_json_response(
                request,
                400,
                error_response(format!("解析 Kiro adapter 请求 JSON 失败: {}", error)),
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
    let server = Server::http("127.0.0.1:0").expect("bind kiro adapter server");
    let address = server.server_addr().to_string();
    let port = address
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .expect("parse kiro adapter port");
    let token = Uuid::new_v4().simple().to_string();
    let shutdown = Arc::new(AtomicBool::new(false));

    kiro_oauth::restore_pending_oauth_listener();

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
