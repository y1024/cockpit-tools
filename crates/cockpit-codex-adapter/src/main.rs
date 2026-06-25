use cockpit_core::models::codex::{
    CodexAccount, CodexApiProviderMode, CodexAppSpeed, CodexQuota, CodexTokens,
};
use cockpit_core::models::codex_local_access::{
    CodexLocalAccessAccountModelRule, CodexLocalAccessChatMessage,
    CodexLocalAccessClientBaseUrlHost, CodexLocalAccessCustomRoutingRule,
    CodexLocalAccessGatewayMode, CodexLocalAccessImageGenerationMode, CodexLocalAccessModelAlias,
    CodexLocalAccessModelPricing, CodexLocalAccessRequestKind, CodexLocalAccessRoutingStrategy,
    CodexLocalAccessScope, CodexLocalAccessState, CodexLocalAccessTimeoutPreset,
    CodexLocalAccessTimeouts,
};
use cockpit_core::models::{
    DefaultInstanceSettings, InstanceLaunchMode, InstanceProfile, InstanceStore,
};
use cockpit_core::modules::{
    account, codex_account, codex_config_format, codex_instance, codex_local_access,
    codex_model_injector, codex_model_provider, codex_oauth, codex_quota, codex_session_manager,
    codex_session_visibility, codex_speed, codex_thread_sync, codex_wakeup, codex_wakeup_scheduler,
    config, instance, logger, openclaw_auth, opencode_auth, process, wakeup, wakeup_history,
    wakeup_scheduler, wakeup_verification,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tiny_http::{Header, Method, Response, Server, StatusCode};
use tokio::runtime::Runtime;
use uuid::Uuid;

const CODEX_GROUPS_FILE: &str = "codex_account_groups.json";
const CODEX_MODEL_PROVIDERS_FILE: &str = "codex_model_providers.json";
const DEFAULT_INSTANCE_ID: &str = "__default__";
const HOST_EVENT_TIMEOUT: Duration = Duration::from_secs(10);
const CODEX_LOCAL_ACCESS_CHAT_TEST_STREAM_EVENT: &str = "codex-local-access-chat-test-stream";
const CODEX_MODEL_PROVIDER_CHAT_TEST_PROGRESS_EVENT: &str = "codex://model-provider-test-progress";
const CODEX_KEEPALIVE_FAILURE_BACKOFF_SECONDS: i64 = 15 * 60;

static CODEX_KEEPALIVE_NEXT_ALLOWED: std::sync::LazyLock<Mutex<HashMap<String, i64>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn now_unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexLaunchCredentialChange {
    from: String,
    to: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexInstanceProfileView {
    id: String,
    name: String,
    user_data_dir: String,
    working_dir: Option<String>,
    extra_args: String,
    bind_account_id: Option<String>,
    launch_mode: InstanceLaunchMode,
    app_speed: CodexAppSpeed,
    created_at: i64,
    last_launched_at: Option<i64>,
    last_pid: Option<u32>,
    running: bool,
    initialized: bool,
    is_default: bool,
    follow_local_account: bool,
    auto_sync_threads: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    codex_launch_credential_change: Option<CodexLaunchCredentialChange>,
}

impl CodexInstanceProfileView {
    fn from_profile(profile: InstanceProfile, running: bool, initialized: bool) -> Self {
        Self {
            id: profile.id,
            name: profile.name,
            user_data_dir: profile.user_data_dir,
            working_dir: profile.working_dir,
            extra_args: profile.extra_args,
            bind_account_id: profile.bind_account_id,
            launch_mode: profile.launch_mode,
            app_speed: profile.app_speed,
            created_at: profile.created_at,
            last_launched_at: profile.last_launched_at,
            last_pid: profile.last_pid,
            running,
            initialized,
            is_default: false,
            follow_local_account: false,
            auto_sync_threads: false,
            codex_launch_credential_change: None,
        }
    }

    fn with_launch_credential_change(
        mut self,
        change: Option<CodexLaunchCredentialChange>,
    ) -> Self {
        self.codex_launch_credential_change = change;
        self
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexInstanceLaunchInfo {
    instance_id: String,
    user_data_dir: String,
    launch_command: String,
}

#[derive(Clone)]
struct CodexLaunchCredentialSnapshot {
    kind: String,
    source: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessActivateResult {
    state: CodexLocalAccessState,
    launch_on_switch: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexSwitchPostActions {
    codex_launch_on_switch: bool,
    opencode_restart_app_path: Option<String>,
    restart_specified_app_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SwitchCodexAccountResult {
    account: CodexAccount,
    post_actions: CodexSwitchPostActions,
}

struct CodexLaunchContext {
    user_data_dir: String,
    working_dir: Option<String>,
    extra_args: String,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountIdPayload {
    account_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SwitchCodexAccountPayload {
    account_id: String,
    auto_repair_mode: Option<codex_session_visibility::CodexSessionVisibilityAutoRepairMode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuthCompletePayload {
    login_id: String,
    reauth_account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuthCancelPayload {
    login_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuthCallbackPayload {
    login_id: String,
    callback_url: String,
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
struct SessionIdsPayload {
    session_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionSearchPayload {
    title_query: Option<String>,
    content_query: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncSessionsToInstancePayload {
    session_ids: Vec<String>,
    target_instance_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepairSessionVisibilityPayload {
    mode: Option<codex_session_visibility::CodexSessionVisibilityRepairMode>,
    run_id: Option<String>,
    target_provider: Option<String>,
    target_instance_id: Option<String>,
    repair_instance_ids: Option<Vec<String>>,
    session_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstanceIdPayload {
    instance_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartCodexInstancePayload {
    instance_id: String,
    skip_default_bind_account_injection: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstanceQuickConfigPayload {
    instance_id: String,
    model_context_window: Option<i64>,
    auto_compact_token_limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateCodexInstancePayload {
    name: String,
    user_data_dir: String,
    working_dir: Option<String>,
    extra_args: Option<String>,
    bind_account_id: Option<String>,
    copy_source_instance_id: Option<String>,
    init_mode: Option<String>,
    launch_mode: Option<InstanceLaunchMode>,
    app_speed: Option<CodexAppSpeed>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCodexInstancePayload {
    instance_id: String,
    name: Option<String>,
    working_dir: Option<String>,
    extra_args: Option<String>,
    bind_account_id: Option<String>,
    bind_account_id_set: bool,
    follow_local_account: Option<bool>,
    launch_mode: Option<InstanceLaunchMode>,
    app_speed: Option<CodexAppSpeed>,
    auto_sync_threads: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchIdsPayload {
    batch_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonContentPayload {
    json_content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FilePathsPayload {
    file_paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchImportStartPayload {
    file_paths: Vec<String>,
    check_quota: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchImportSessionPayload {
    session_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchImportConfirmPayload {
    session_id: String,
    item_ids: Vec<String>,
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
struct CodexWakeupTestPayload {
    account_ids: Vec<String>,
    prompt: Option<String>,
    model: Option<String>,
    model_display_name: Option<String>,
    model_reasoning_effort: Option<String>,
    run_id: Option<String>,
    cancel_scope_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexWakeupRunTaskPayload {
    task_id: String,
    run_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexWakeupRunEnabledTasksPayload {
    trigger_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenAccountPayload {
    id_token: String,
    access_token: String,
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiKeyAccountPayload {
    api_key: String,
    api_base_url: Option<String>,
    api_provider_mode: Option<CodexApiProviderMode>,
    api_provider_id: Option<String>,
    api_provider_name: Option<String>,
    api_model_catalog: Option<Vec<String>>,
    api_wire_api: Option<String>,
    api_supports_vision: Option<bool>,
    api_model_vision_support: Option<HashMap<String, bool>>,
    api_vision_routing_model: Option<String>,
    account_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountNamePayload {
    account_id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiKeyCredentialsPayload {
    account_id: String,
    api_key: String,
    api_base_url: Option<String>,
    api_provider_mode: Option<CodexApiProviderMode>,
    api_provider_id: Option<String>,
    api_provider_name: Option<String>,
    api_model_catalog: Option<Vec<String>>,
    api_wire_api: Option<String>,
    api_supports_vision: Option<bool>,
    api_model_vision_support: Option<HashMap<String, bool>>,
    api_vision_routing_model: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiKeyBoundOAuthPayload {
    account_id: String,
    bound_oauth_account_id: Option<String>,
    bound_oauth_use_local_gateway: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessBoundOAuthPayload {
    bound_oauth_account_id: Option<String>,
    bound_oauth_use_local_gateway: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessCreateApiKeyPayload {
    label: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessApiKeyIdPayload {
    api_key_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessEnabledPayload {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelProviderConnectionPayload {
    base_url: String,
    api_key: String,
    wire_api: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelProviderModelsPayload {
    base_url: String,
    api_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelProviderUsagePayload {
    base_url: String,
    api_key: String,
    integration_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelProviderChatTestBatchPayload {
    targets: Vec<codex_model_provider::CodexModelProviderChatTestTarget>,
    prompt: Option<String>,
    model: Option<String>,
    run_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsBoolPayload {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessSaveAccountsPayload {
    account_ids: Vec<String>,
    restrict_free_accounts: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessPortPayload {
    port: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessRoutingStrategyPayload {
    strategy: CodexLocalAccessRoutingStrategy,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessCustomRoutingPayload {
    rules: Vec<CodexLocalAccessCustomRoutingRule>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessAccountModelRulesPayload {
    rules: Vec<CodexLocalAccessAccountModelRule>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessModelRulesPayload {
    model_aliases: Vec<CodexLocalAccessModelAlias>,
    excluded_models: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessModelPricingsPayload {
    model_pricings: Vec<CodexLocalAccessModelPricing>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessRoutingOptionsPayload {
    session_affinity: bool,
    session_affinity_ttl_ms: i64,
    max_retry_credentials: u16,
    max_retry_interval_ms: u64,
    disable_cooling: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessTimeoutsPayload {
    timeouts: CodexLocalAccessTimeouts,
    active_timeout_preset_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessTimeoutPresetsPayload {
    timeout_presets: Vec<CodexLocalAccessTimeoutPreset>,
    active_timeout_preset_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessUpstreamProxyConfigPayload {
    upstream_proxy_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessGatewayModePayload {
    gateway_mode: CodexLocalAccessGatewayMode,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessDebugLogsPayload {
    debug_logs: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessAccessScopePayload {
    access_scope: CodexLocalAccessScope,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessClientBaseUrlHostPayload {
    client_base_url_host: CodexLocalAccessClientBaseUrlHost,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessImageGenerationModePayload {
    image_generation_mode: CodexLocalAccessImageGenerationMode,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessUpdateApiKeyPayload {
    api_key_id: String,
    label: Option<String>,
    enabled: Option<bool>,
    model_prefix: Option<String>,
    allowed_models: Option<Vec<String>>,
    excluded_models: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessActivatePayload {
    auto_repair_mode: Option<codex_session_visibility::CodexSessionVisibilityAutoRepairMode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessQueryRequestLogsPayload {
    page: u32,
    page_size: u32,
    stats_range: Option<String>,
    model_query: Option<String>,
    account_query: Option<String>,
    api_key_query: Option<String>,
    gateway_mode: Option<CodexLocalAccessGatewayMode>,
    request_kind: Option<CodexLocalAccessRequestKind>,
    success: Option<bool>,
    error_category: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessChatTestPayload {
    model_id: String,
    messages: Vec<CodexLocalAccessChatMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccessChatTestStreamPayload {
    session_id: String,
    model_id: String,
    messages: Vec<CodexLocalAccessChatMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TagsPayload {
    account_id: String,
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotePayload {
    account_id: String,
    note: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountGroupsPayload {
    data: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuickConfigPayload {
    model_context_window: Option<i64>,
    auto_compact_token_limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSpeedPayload {
    speed: CodexAppSpeed,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountAppSpeedPayload {
    account_id: String,
    speed: CodexAppSpeed,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReferralPayload {
    account_id: String,
    referral_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReferralInvitePayload {
    account_id: String,
    referral_key: Option<String>,
    emails: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionPayload {
    account_id: String,
    #[serde(default)]
    force: bool,
}

fn resolve_local_account_id() -> Option<String> {
    let account = codex_account::get_current_account()?;
    Some(account.id)
}

fn resolve_default_account_id(settings: &DefaultInstanceSettings) -> Option<String> {
    if settings.follow_local_account {
        resolve_local_account_id()
    } else {
        settings.bind_account_id.clone()
    }
}

fn launch_credential_kind_for_bind_account_id(account_id: &str) -> Option<String> {
    codex_launch_credential_snapshot_for_account_id(account_id, "").map(|snapshot| snapshot.kind)
}

fn read_applied_launch_credential_kind_for_dir(data_dir: &Path) -> Option<String> {
    let account_id = codex_account::read_managed_projection_account_id_from_dir(data_dir)?;
    launch_credential_kind_for_bind_account_id(&account_id)
}

fn build_launch_credential_change(
    before: Option<String>,
    after: Option<String>,
) -> Option<CodexLaunchCredentialChange> {
    let (Some(from), Some(to)) = (before, after) else {
        return None;
    };
    if from == to {
        return None;
    }
    Some(CodexLaunchCredentialChange { from, to })
}

fn log_session_visibility_repair_deferred_before_launch(
    context: &str,
    launch_provider_change: &Option<CodexLaunchCredentialChange>,
) {
    let Some(change) = launch_provider_change else {
        return;
    };
    logger::log_info(&format!(
        "[Codex Session Visibility] {}: credential kind changed before launch, defer quick repair to frontend notice, from={}, to={}",
        context, change.from, change.to
    ));
}

async fn inject_bound_account_to_profile(
    profile_dir: &Path,
    bind_account_id: &str,
) -> Result<(), String> {
    if codex_instance::is_api_service_bind_account_id(bind_account_id) {
        codex_local_access::activate_local_access_for_dir(profile_dir).await?;
        return Ok(());
    }

    if let Some(provider_gateway_account_id) =
        codex_instance::parse_provider_gateway_bind_account_id(bind_account_id)
    {
        codex_local_access::activate_provider_gateway_for_dir(
            profile_dir,
            &provider_gateway_account_id,
        )
        .await?;
        return Ok(());
    }

    codex_local_access::cleanup_provider_gateway_profile_model_overrides(profile_dir)?;
    codex_instance::inject_account_to_profile(profile_dir, bind_account_id).await
}

async fn ensure_provider_gateway_for_bind_account(
    profile_dir: &Path,
    bind_account_id: Option<&str>,
) -> Result<(), String> {
    let Some(bind_account_id) = bind_account_id else {
        codex_local_access::stop_provider_gateways_for_profile(profile_dir).await;
        return Ok(());
    };
    if codex_instance::is_api_service_bind_account_id(bind_account_id) {
        codex_local_access::stop_provider_gateways_for_profile(profile_dir).await;
        return Ok(());
    }
    let Some(provider_gateway_account_id) =
        codex_instance::parse_provider_gateway_bind_account_id(bind_account_id)
    else {
        let Some(account) = codex_account::load_account(bind_account_id) else {
            codex_local_access::stop_provider_gateways_for_profile(profile_dir).await;
            return Ok(());
        };
        if codex_local_access::account_requires_provider_gateway(&account) {
            codex_local_access::stop_provider_gateways_for_profile(profile_dir).await;
            return codex_local_access::ensure_provider_gateway_for_dir(
                profile_dir,
                bind_account_id,
            )
            .await;
        }
        if codex_local_access::account_requires_bound_oauth_local_gateway(&account) {
            codex_local_access::stop_provider_gateways_for_profile(profile_dir).await;
            return codex_local_access::ensure_bound_oauth_local_gateway_for_dir(
                profile_dir,
                bind_account_id,
            )
            .await;
        }
        codex_local_access::stop_provider_gateways_for_profile(profile_dir).await;
        return Ok(());
    };
    codex_local_access::stop_provider_gateways_for_profile(profile_dir).await;
    codex_local_access::ensure_provider_gateway_for_dir(profile_dir, &provider_gateway_account_id)
        .await
}

async fn apply_bound_account_to_initialized_profile(
    profile_dir: &Path,
    bind_account_id: Option<&str>,
    context: &str,
) -> Result<Option<CodexLaunchCredentialChange>, String> {
    if !instance::is_profile_initialized(profile_dir) {
        return Ok(None);
    }

    let previous_kind = read_applied_launch_credential_kind_for_dir(profile_dir);
    if let Some(account_id) = bind_account_id {
        inject_bound_account_to_profile(profile_dir, account_id).await?;
        ensure_provider_gateway_for_bind_account(profile_dir, bind_account_id).await?;
    } else {
        codex_local_access::cleanup_provider_gateway_profile_model_overrides(profile_dir)?;
        codex_local_access::stop_provider_gateways_for_profile(profile_dir).await;
    }
    let launch_credential_change = build_launch_credential_change(
        previous_kind,
        bind_account_id.and_then(launch_credential_kind_for_bind_account_id),
    );
    log_session_visibility_repair_deferred_before_launch(context, &launch_credential_change);
    Ok(launch_credential_change)
}

fn default_instance_view(
    default_dir: &Path,
    default_settings: &DefaultInstanceSettings,
    bind_account_id: Option<String>,
    running: bool,
    last_pid: Option<u32>,
) -> CodexInstanceProfileView {
    CodexInstanceProfileView {
        id: DEFAULT_INSTANCE_ID.to_string(),
        name: String::new(),
        user_data_dir: default_dir.to_string_lossy().to_string(),
        working_dir: None,
        extra_args: default_settings.extra_args.clone(),
        bind_account_id,
        launch_mode: default_settings.launch_mode.clone(),
        app_speed: default_settings.app_speed.clone(),
        created_at: 0,
        last_launched_at: None,
        last_pid,
        running,
        initialized: instance::is_profile_initialized(default_dir),
        is_default: true,
        follow_local_account: default_settings.follow_local_account,
        auto_sync_threads: default_settings.auto_sync_threads,
        codex_launch_credential_change: None,
    }
}

fn list_codex_instances() -> Result<Vec<CodexInstanceProfileView>, String> {
    let store = codex_instance::load_instance_store()?;
    let default_dir = codex_instance::get_default_codex_home()?;

    let default_settings = store.default_settings.clone();
    let process_entries = process::collect_codex_process_entries();
    let mut result: Vec<CodexInstanceProfileView> = store
        .instances
        .into_iter()
        .map(|instance_profile| {
            let resolved_pid = process::resolve_codex_pid_from_entries(
                instance_profile.last_pid,
                Some(&instance_profile.user_data_dir),
                &process_entries,
            );
            let running = resolved_pid.is_some();
            let initialized =
                instance::is_profile_initialized(Path::new(&instance_profile.user_data_dir));
            let mut view =
                CodexInstanceProfileView::from_profile(instance_profile, running, initialized);
            view.last_pid = resolved_pid;
            view
        })
        .collect();

    let default_pid =
        process::resolve_codex_pid_from_entries(default_settings.last_pid, None, &process_entries);
    let default_running = default_pid.is_some();
    let default_bind_account_id = resolve_default_account_id(&default_settings);
    result.push(default_instance_view(
        &default_dir,
        &default_settings,
        default_bind_account_id,
        default_running,
        default_pid,
    ));

    Ok(result)
}

fn create_codex_instance(
    payload: CreateCodexInstancePayload,
) -> Result<CodexInstanceProfileView, String> {
    let instance_profile = codex_instance::create_instance(codex_instance::CreateInstanceParams {
        name: payload.name,
        user_data_dir: payload.user_data_dir,
        working_dir: payload.working_dir,
        extra_args: payload.extra_args.unwrap_or_default(),
        bind_account_id: payload.bind_account_id,
        copy_source_instance_id: payload.copy_source_instance_id,
        init_mode: payload.init_mode,
        launch_mode: payload.launch_mode,
        app_speed: payload.app_speed,
    })?;

    let initialized = instance::is_profile_initialized(Path::new(&instance_profile.user_data_dir));
    Ok(CodexInstanceProfileView::from_profile(
        instance_profile,
        false,
        initialized,
    ))
}

async fn update_codex_instance(
    payload: UpdateCodexInstancePayload,
) -> Result<CodexInstanceProfileView, String> {
    let UpdateCodexInstancePayload {
        instance_id,
        name,
        working_dir,
        extra_args,
        bind_account_id,
        bind_account_id_set,
        follow_local_account,
        launch_mode,
        app_speed,
        auto_sync_threads,
    } = payload;
    let should_apply_bind_account = bind_account_id_set || follow_local_account.is_some();
    let next_bind_account = if bind_account_id_set {
        Some(bind_account_id.clone())
    } else {
        None
    };

    if instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = codex_instance::get_default_codex_home()?;
        let mut updated = codex_instance::update_default_settings(
            next_bind_account,
            extra_args,
            follow_local_account,
            launch_mode,
            auto_sync_threads,
        )?;
        if let Some(speed) = app_speed {
            updated = codex_instance::update_default_app_speed(speed.clone())?;
            codex_speed::write_app_speed_for_dir(&default_dir, speed)?;
        }
        let resolved_pid = process::resolve_codex_pid(updated.last_pid, None);
        let running = resolved_pid.is_some();
        let default_bind_account_id = resolve_default_account_id(&updated);
        let launch_credential_change = if should_apply_bind_account {
            apply_bound_account_to_initialized_profile(
                &default_dir,
                default_bind_account_id.as_deref(),
                "update-default-bind-account",
            )
            .await?
        } else {
            None
        };
        let _ = name;
        let _ = working_dir;
        return Ok(default_instance_view(
            &default_dir,
            &updated,
            default_bind_account_id,
            running,
            resolved_pid,
        )
        .with_launch_credential_change(launch_credential_change));
    }

    if bind_account_id_set && bind_account_id.is_some() {
        let store = codex_instance::load_instance_store()?;
        if let Some(target) = store.instances.iter().find(|item| item.id == instance_id) {
            if !instance::is_profile_initialized(Path::new(&target.user_data_dir)) {
                return Err(
                    "INSTANCE_NOT_INITIALIZED:请先启动一次实例创建数据后，再进行账号绑定"
                        .to_string(),
                );
            }
        }
    }

    let selected_app_speed = app_speed.clone();
    let instance_profile = codex_instance::update_instance(codex_instance::UpdateInstanceParams {
        instance_id,
        name,
        working_dir,
        extra_args,
        bind_account_id: next_bind_account,
        launch_mode,
        app_speed,
    })?;
    if let Some(speed) = selected_app_speed {
        codex_speed::write_app_speed_for_dir(Path::new(&instance_profile.user_data_dir), speed)?;
    }

    let running = instance_profile
        .last_pid
        .map(process::is_pid_running)
        .unwrap_or(false);
    let initialized = instance::is_profile_initialized(Path::new(&instance_profile.user_data_dir));
    let launch_credential_change = if bind_account_id_set {
        apply_bound_account_to_initialized_profile(
            Path::new(&instance_profile.user_data_dir),
            instance_profile.bind_account_id.as_deref(),
            "update-instance-bind-account",
        )
        .await?
    } else {
        None
    };
    let _ = follow_local_account;
    let _ = auto_sync_threads;
    Ok(
        CodexInstanceProfileView::from_profile(instance_profile, running, initialized)
            .with_launch_credential_change(launch_credential_change),
    )
}

fn delete_codex_instance(instance_id: &str) -> Result<(), String> {
    if instance_id == DEFAULT_INSTANCE_ID {
        return Err("默认实例不可删除".to_string());
    }
    codex_instance::delete_instance(instance_id)
}

fn sync_codex_threads_across_idle_instances(context: &str) {
    let started = Instant::now();
    let default_settings = match codex_instance::load_default_settings() {
        Ok(settings) => settings,
        Err(error) => {
            logger::log_warn(&format!(
                "[Codex Thread Sync] {}: skipped automatic idle sync, failed to read settings: {}",
                context, error
            ));
            return;
        }
    };
    if !default_settings.auto_sync_threads {
        return;
    }

    match codex_thread_sync::sync_threads_across_instances_if_all_stopped() {
        Ok(Some(summary)) => {
            if summary.total_synced_thread_count > 0 {
                logger::log_info(&format!(
                    "[Codex Thread Sync] {}: synced {} sessions across {} instances, elapsed_ms={}",
                    context,
                    summary.total_synced_thread_count,
                    summary.mutated_instance_count,
                    started.elapsed().as_millis()
                ));
            } else {
                logger::log_info(&format!(
                    "[Codex Thread Sync] {}: completed with no changes, elapsed_ms={}",
                    context,
                    started.elapsed().as_millis()
                ));
            }
        }
        Ok(None) => {
            logger::log_info(&format!(
                "[Codex Thread Sync] {}: skipped because instances are not idle or not enough instances, elapsed_ms={}",
                context,
                started.elapsed().as_millis()
            ));
        }
        Err(error) => {
            logger::log_warn(&format!(
                "[Codex Thread Sync] {}: skipped automatic idle sync: {}",
                context, error
            ));
        }
    }
}

async fn stop_codex_instance(instance_id: String) -> Result<CodexInstanceProfileView, String> {
    if instance_id == DEFAULT_INSTANCE_ID {
        let default_dir = codex_instance::get_default_codex_home()?;
        process::close_codex_instances(&[default_dir.to_string_lossy().to_string()], 20)?;
        codex_local_access::stop_provider_gateways_for_profile(&default_dir).await;
        let updated = codex_instance::update_default_pid(None)?;
        let default_bind_account_id = resolve_default_account_id(&updated);
        sync_codex_threads_across_idle_instances("after-stop-default");
        return Ok(default_instance_view(
            &default_dir,
            &updated,
            default_bind_account_id,
            false,
            None,
        ));
    }

    let store = codex_instance::load_instance_store()?;
    let instance_profile = store
        .instances
        .into_iter()
        .find(|item| item.id == instance_id)
        .ok_or("实例不存在")?;

    if let Some(pid) = process::resolve_codex_pid(
        instance_profile.last_pid,
        Some(&instance_profile.user_data_dir),
    ) {
        process::close_pid(pid, 20)?;
    }
    codex_local_access::stop_provider_gateways_for_profile(Path::new(
        &instance_profile.user_data_dir,
    ))
    .await;
    let updated = codex_instance::update_instance_pid(&instance_profile.id, None)?;
    let initialized = instance::is_profile_initialized(Path::new(&updated.user_data_dir));
    sync_codex_threads_across_idle_instances("after-stop-instance");
    Ok(CodexInstanceProfileView::from_profile(
        updated,
        false,
        initialized,
    ))
}

async fn close_all_codex_instances() -> Result<(), String> {
    let store = codex_instance::load_instance_store()?;
    let default_home = codex_instance::get_default_codex_home()?;
    let mut target_homes: Vec<String> = Vec::new();
    target_homes.push(default_home.to_string_lossy().to_string());
    for instance_profile in &store.instances {
        let home = instance_profile.user_data_dir.trim();
        if !home.is_empty() {
            target_homes.push(home.to_string());
        }
    }

    process::close_codex_instances(&target_homes, 20)?;
    codex_local_access::stop_provider_gateways_for_profile(&default_home).await;
    for instance_profile in &store.instances {
        let home = instance_profile.user_data_dir.trim();
        if !home.is_empty() {
            codex_local_access::stop_provider_gateways_for_profile(Path::new(home)).await;
        }
    }
    let _ = codex_instance::clear_all_pids();
    sync_codex_threads_across_idle_instances("after-close-all");
    Ok(())
}

fn open_codex_instance_window(instance_id: &str) -> Result<(), String> {
    if instance_id == DEFAULT_INSTANCE_ID {
        let default_settings = codex_instance::load_default_settings()?;
        if default_settings.launch_mode == InstanceLaunchMode::Cli {
            return Err("CLI 模式实例不支持窗口定位，请改用终端执行。".to_string());
        }
        process::focus_codex_instance(default_settings.last_pid, None)
            .map_err(|err| format!("定位 Codex 默认实例窗口失败: {}", err))?;
        return Ok(());
    }

    let store = codex_instance::load_instance_store()?;
    let instance_profile = store
        .instances
        .into_iter()
        .find(|item| item.id == instance_id)
        .ok_or("实例不存在")?;
    if instance_profile.launch_mode == InstanceLaunchMode::Cli {
        return Err("CLI 模式实例不支持窗口定位，请改用终端执行。".to_string());
    }

    process::focus_codex_instance(
        instance_profile.last_pid,
        Some(&instance_profile.user_data_dir),
    )
    .map_err(|err| {
        format!(
            "定位 Codex 实例窗口失败: instance_id={}, err={}",
            instance_profile.id, err
        )
    })?;
    Ok(())
}

fn resolve_instance_base_dir(instance_id: &str) -> Result<PathBuf, String> {
    if instance_id == DEFAULT_INSTANCE_ID {
        return codex_instance::get_default_codex_home();
    }

    let store = codex_instance::load_instance_store()?;
    let instance_profile = store
        .instances
        .into_iter()
        .find(|item| item.id == instance_id)
        .ok_or("实例不存在")?;
    Ok(PathBuf::from(instance_profile.user_data_dir))
}

fn resolve_instance_launch_context(instance_id: &str) -> Result<CodexLaunchContext, String> {
    if instance_id == DEFAULT_INSTANCE_ID {
        let default_settings = codex_instance::load_default_settings()?;
        if default_settings.launch_mode != InstanceLaunchMode::Cli {
            return Err("当前实例未启用 CLI 启动方式".to_string());
        }
        let default_dir = codex_instance::get_default_codex_home()?;
        return Ok(CodexLaunchContext {
            user_data_dir: default_dir.to_string_lossy().to_string(),
            working_dir: None,
            extra_args: default_settings.extra_args,
        });
    }

    let store = codex_instance::load_instance_store()?;
    let instance_profile = store
        .instances
        .into_iter()
        .find(|item| item.id == instance_id)
        .ok_or("实例不存在")?;
    if instance_profile.launch_mode != InstanceLaunchMode::Cli {
        return Err("当前实例未启用 CLI 启动方式".to_string());
    }
    Ok(CodexLaunchContext {
        user_data_dir: instance_profile.user_data_dir,
        working_dir: instance_profile.working_dir,
        extra_args: instance_profile.extra_args,
    })
}

async fn start_codex_instance(
    instance_id: String,
    skip_default_bind_account_injection: bool,
) -> Result<CodexInstanceProfileView, String> {
    let flow_started = Instant::now();
    logger::log_info(&format!(
        "[Codex Start] adapter start_instance started: instance_id={}, skip_default_bind_account_injection={}",
        instance_id, skip_default_bind_account_injection
    ));

    if instance_id == DEFAULT_INSTANCE_ID {
        let prepare_started = Instant::now();
        let default_dir = codex_instance::get_default_codex_home()?;
        let previous_kind = read_applied_launch_credential_kind_for_dir(&default_dir);
        let default_settings = codex_instance::load_default_settings()?;
        let default_bind_account_id = resolve_default_account_id(&default_settings);
        if default_settings.launch_mode != InstanceLaunchMode::Cli {
            process::ensure_codex_launch_path_configured()?;
        }
        logger::log_info(&format!(
            "[Codex Start] adapter default prepare finished: bind_account_id={:?}, launch_mode={:?}, elapsed_ms={}, total_ms={}",
            default_bind_account_id,
            default_settings.launch_mode,
            prepare_started.elapsed().as_millis(),
            flow_started.elapsed().as_millis()
        ));

        let close_started = Instant::now();
        process::close_codex_instances(&[default_dir.to_string_lossy().to_string()], 20)?;
        codex_local_access::stop_provider_gateways_for_profile(&default_dir).await;
        logger::log_info(&format!(
            "[Codex Start] adapter default close phase finished: elapsed_ms={}, total_ms={}",
            close_started.elapsed().as_millis(),
            flow_started.elapsed().as_millis()
        ));

        let speed_started = Instant::now();
        let _ = codex_instance::update_default_pid(None)?;
        codex_speed::write_app_speed_for_dir(&default_dir, default_settings.app_speed.clone())?;
        logger::log_info(&format!(
            "[Codex Start] adapter default speed/pid reset finished: elapsed_ms={}, total_ms={}",
            speed_started.elapsed().as_millis(),
            flow_started.elapsed().as_millis()
        ));

        let inject_started = Instant::now();
        if let Some(ref account_id) = default_bind_account_id {
            if skip_default_bind_account_injection {
                logger::log_info(&format!(
                    "[Codex Start] adapter skipped default bind-account injection: account_id={}",
                    account_id
                ));
            } else {
                inject_bound_account_to_profile(&default_dir, account_id).await?;
            }
        } else {
            codex_local_access::cleanup_provider_gateway_profile_model_overrides(&default_dir)?;
        }
        logger::log_info(&format!(
            "[Codex Start] adapter default profile injection finished: elapsed_ms={}, total_ms={}",
            inject_started.elapsed().as_millis(),
            flow_started.elapsed().as_millis()
        ));

        let provider_gateway_started = Instant::now();
        ensure_provider_gateway_for_bind_account(&default_dir, default_bind_account_id.as_deref())
            .await?;
        logger::log_info(&format!(
            "[Codex Start] adapter default provider gateway finished: elapsed_ms={}, total_ms={}",
            provider_gateway_started.elapsed().as_millis(),
            flow_started.elapsed().as_millis()
        ));

        let launch_credential_change = build_launch_credential_change(
            previous_kind,
            default_bind_account_id
                .as_deref()
                .and_then(launch_credential_kind_for_bind_account_id),
        );
        log_session_visibility_repair_deferred_before_launch(
            "before-start-default",
            &launch_credential_change,
        );
        if skip_default_bind_account_injection {
            logger::log_info(
                "[Codex Thread Sync] adapter before-start-default: skipped on prepared-profile fast path",
            );
        } else {
            sync_codex_threads_across_idle_instances("before-start-default");
        }

        sanitize_codex_config_before_launch(&default_dir)?;

        if default_settings.launch_mode == InstanceLaunchMode::Cli {
            let context = resolve_instance_launch_context(DEFAULT_INSTANCE_ID)?;
            let _ = build_launch_command(&context)?;
            let _ = codex_instance::update_default_pid(None)?;
            return Ok(default_instance_view(
                &default_dir,
                &default_settings,
                default_bind_account_id,
                false,
                None,
            )
            .with_launch_credential_change(launch_credential_change));
        }

        let extra_args = process::parse_extra_args(&default_settings.extra_args);
        let pid = if skip_default_bind_account_injection {
            process::start_codex_default_fast_after_close(&extra_args)?
        } else {
            process::start_codex_default(&extra_args)?
        };
        codex_model_injector::inject_for_codex_home_later(default_dir.clone());
        let updated = codex_instance::update_default_pid(Some(pid))?;
        let running = process::is_pid_running(pid);
        return Ok(default_instance_view(
            &default_dir,
            &updated,
            default_bind_account_id,
            running,
            Some(pid),
        )
        .with_launch_credential_change(launch_credential_change));
    }

    let store = codex_instance::load_instance_store()?;
    let instance_profile = store
        .instances
        .into_iter()
        .find(|item| item.id == instance_id)
        .ok_or("实例不存在")?;

    codex_instance::ensure_instance_shared_skills(Path::new(&instance_profile.user_data_dir))?;
    let instance_dir = Path::new(&instance_profile.user_data_dir);
    let previous_kind = read_applied_launch_credential_kind_for_dir(instance_dir);

    if let Some(pid) = process::resolve_codex_pid(
        instance_profile.last_pid,
        Some(&instance_profile.user_data_dir),
    ) {
        process::close_pid(pid, 20)?;
        let _ = codex_instance::update_instance_pid(&instance_profile.id, None)?;
    }
    codex_local_access::stop_provider_gateways_for_profile(instance_dir).await;
    codex_speed::write_app_speed_for_dir(instance_dir, instance_profile.app_speed.clone())?;

    if let Some(ref account_id) = instance_profile.bind_account_id {
        inject_bound_account_to_profile(instance_dir, account_id).await?;
    } else {
        codex_local_access::cleanup_provider_gateway_profile_model_overrides(instance_dir)?;
    }
    ensure_provider_gateway_for_bind_account(
        instance_dir,
        instance_profile.bind_account_id.as_deref(),
    )
    .await?;

    let launch_credential_change = build_launch_credential_change(
        previous_kind,
        instance_profile
            .bind_account_id
            .as_deref()
            .and_then(launch_credential_kind_for_bind_account_id),
    );
    log_session_visibility_repair_deferred_before_launch(
        "before-start-instance",
        &launch_credential_change,
    );
    sync_codex_threads_across_idle_instances("before-start-instance");
    sanitize_codex_config_before_launch(instance_dir)?;

    if instance_profile.launch_mode == InstanceLaunchMode::Cli {
        let context = resolve_instance_launch_context(&instance_profile.id)?;
        let _ = build_launch_command(&context)?;
        let updated = codex_instance::update_instance_after_cli_prepare(&instance_profile.id)?;
        let initialized = instance::is_profile_initialized(Path::new(&updated.user_data_dir));
        return Ok(
            CodexInstanceProfileView::from_profile(updated, false, initialized)
                .with_launch_credential_change(launch_credential_change),
        );
    }

    process::ensure_codex_launch_path_configured()?;
    let extra_args = process::parse_extra_args(&instance_profile.extra_args);
    let pid = process::start_codex_with_args(&instance_profile.user_data_dir, &extra_args)?;
    codex_model_injector::inject_for_codex_home_later(PathBuf::from(
        &instance_profile.user_data_dir,
    ));
    let updated = codex_instance::update_instance_after_start(&instance_profile.id, pid)?;
    let running = process::is_pid_running(pid);
    let initialized = instance::is_profile_initialized(Path::new(&updated.user_data_dir));
    Ok(
        CodexInstanceProfileView::from_profile(updated, running, initialized)
            .with_launch_credential_change(launch_credential_change),
    )
}

fn sanitize_codex_config_before_launch(data_dir: &Path) -> Result<(), String> {
    codex_config_format::sanitize_codex_config_toml_file(&data_dir.join("config.toml")).map(|_| ())
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

fn build_launch_command(context: &CodexLaunchContext) -> Result<String, String> {
    sanitize_codex_config_before_launch(Path::new(&context.user_data_dir))?;
    let runtime = codex_wakeup::resolve_cli_runtime()?;
    let parsed_args = process::parse_extra_args(&context.extra_args);

    #[cfg(not(target_os = "windows"))]
    {
        let mut command_parts = Vec::new();
        if let Some(ref dir) = context.working_dir {
            if !dir.trim().is_empty() {
                command_parts.push(format!("cd {}", posix_shell_quote(dir)));
            }
        }

        let mut codex_cmd = String::new();
        codex_cmd.push_str("CODEX_HOME=");
        codex_cmd.push_str(&posix_shell_quote(&context.user_data_dir));
        codex_cmd.push(' ');
        if let Some(node_path) = runtime.node_path.as_deref() {
            codex_cmd.push_str(&posix_shell_quote(node_path));
            codex_cmd.push(' ');
        }
        codex_cmd.push_str(&posix_shell_quote(&runtime.binary_path));

        for arg in parsed_args {
            let trimmed = arg.trim();
            if !trimmed.is_empty() {
                codex_cmd.push(' ');
                codex_cmd.push_str(&posix_shell_quote(trimmed));
            }
        }

        command_parts.push(codex_cmd);
        return Ok(command_parts.join(" && "));
    }

    #[cfg(target_os = "windows")]
    {
        let mut command_parts = Vec::new();
        command_parts.push(format!(
            "$env:CODEX_HOME={}",
            powershell_quote(&context.user_data_dir)
        ));

        if let Some(ref dir) = context.working_dir {
            if !dir.trim().is_empty() {
                command_parts.push(format!(
                    "Set-Location -LiteralPath {}",
                    powershell_quote(dir)
                ));
            }
        }

        let mut codex_cmd = String::new();
        if let Some(node_path) = runtime.node_path.as_deref() {
            codex_cmd.push_str("& ");
            codex_cmd.push_str(&powershell_quote(node_path));
            codex_cmd.push(' ');
            codex_cmd.push_str(&powershell_quote(&runtime.binary_path));
        } else {
            codex_cmd.push_str("& ");
            codex_cmd.push_str(&powershell_quote(&runtime.binary_path));
        }

        for arg in parsed_args {
            let trimmed = arg.trim();
            if !trimmed.is_empty() {
                codex_cmd.push(' ');
                codex_cmd.push_str(&powershell_quote(trimmed));
            }
        }

        command_parts.push(codex_cmd);
        return Ok(command_parts.join("; "));
    }

    #[allow(unreachable_code)]
    Err("当前系统暂不支持生成 Codex CLI 启动命令".to_string())
}

fn json_header() -> Header {
    Header::from_bytes(
        &b"Content-Type"[..],
        &b"application/json; charset=utf-8"[..],
    )
    .expect("valid content-type header")
}

fn to_value<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| format!("序列化 Codex adapter 响应失败: {}", error))
}

fn parse_payload<T: for<'de> Deserialize<'de>>(payload: Value) -> Result<T, String> {
    serde_json::from_value(payload)
        .map_err(|error| format!("解析 Codex adapter 请求失败: {}", error))
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

fn emit_host_event_on_blocking_thread(event: String, payload: Value) -> Result<(), String> {
    let url = env::var("COCKPIT_HOST_EVENT_URL")
        .map_err(|_| "Codex adapter 缺少宿主事件桥 URL".to_string())?;
    let token = env::var("COCKPIT_HOST_EVENT_TOKEN")
        .map_err(|_| "Codex adapter 缺少宿主事件桥 token".to_string())?;
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

fn codex_batch_import_event_emitter() -> codex_account::CodexBatchImportEventEmitter {
    Arc::new(|event, payload| {
        if let Err(error) = emit_host_event(event, payload) {
            logger::log_warn(&format!(
                "Codex adapter 转发批量导入事件失败: event={}, err={}",
                event, error
            ));
        }
    })
}

fn codex_wakeup_progress_emitter() -> codex_wakeup::CodexWakeupProgressEmitter {
    Arc::new(|payload| {
        if let Err(error) = emit_host_event(codex_wakeup::PROGRESS_EVENT, payload) {
            logger::log_warn(&format!("Codex adapter 转发唤醒进度事件失败: {}", error));
        }
    })
}

fn wakeup_scheduler_event_emitter() -> wakeup_scheduler::WakeupSchedulerEventEmitter {
    Arc::new(|event, payload| {
        if let Err(error) = emit_host_event(event, payload) {
            logger::log_warn(&format!(
                "Codex adapter 转发通用唤醒调度事件失败: event={}, err={}",
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
                "Codex adapter 转发唤醒批量验证进度事件失败: {}",
                error
            ));
        }
    })
}

fn host_event_emitter() -> codex_oauth::CodexOAuthEventEmitter {
    Arc::new(|event, payload| emit_host_event(event, payload))
}

fn repair_session_visibility(
    payload: RepairSessionVisibilityPayload,
) -> Result<codex_session_visibility::CodexSessionVisibilityRepairSummary, String> {
    let mode = payload
        .mode
        .unwrap_or(codex_session_visibility::CodexSessionVisibilityRepairMode::Quick);
    let resolved_target_provider = match payload
        .target_instance_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(instance_id) => Some(
            codex_session_visibility::resolve_session_visibility_target_provider_from_instance_id(
                instance_id,
            )?,
        ),
        None => payload.target_provider,
    };

    let event_error = Arc::new(Mutex::new(None::<String>));
    let reporter_event_error = Arc::clone(&event_error);
    let reporter =
        move |progress: codex_session_visibility::CodexSessionVisibilityRepairProgress| {
            let should_skip = reporter_event_error
                .lock()
                .map(|guard| guard.is_some())
                .unwrap_or(true);
            if should_skip {
                return;
            }
            let event_payload = match serde_json::to_value(progress) {
                Ok(value) => value,
                Err(error) => {
                    if let Ok(mut guard) = reporter_event_error.lock() {
                        *guard = Some(format!("序列化会话可见性修复进度失败: {}", error));
                    }
                    return;
                }
            };
            if let Err(error) = emit_host_event(
                codex_session_visibility::SESSION_VISIBILITY_REPAIR_PROGRESS_EVENT,
                event_payload,
            ) {
                logger::log_warn(&format!(
                    "Codex adapter 转发会话可见性修复进度失败: {}",
                    error
                ));
                if let Ok(mut guard) = reporter_event_error.lock() {
                    *guard = Some(error);
                }
            }
        };

    let summary = codex_session_visibility::repair_session_visibility_across_instances_with_target(
        mode,
        payload.run_id,
        Some(&reporter),
        resolved_target_provider,
        payload.session_ids,
        payload.repair_instance_ids,
    )?;

    if let Some(error) = event_error.lock().ok().and_then(|guard| guard.clone()) {
        return Err(format!("转发会话可见性修复进度失败: {}", error));
    }

    Ok(summary)
}

fn codex_launch_credential_kind_for_account(account: &CodexAccount) -> &'static str {
    if account.is_api_key_auth() {
        "api"
    } else {
        "account"
    }
}

fn codex_launch_credential_snapshot_for_account(
    account: &CodexAccount,
    source_prefix: &str,
) -> CodexLaunchCredentialSnapshot {
    CodexLaunchCredentialSnapshot {
        kind: codex_launch_credential_kind_for_account(account).to_string(),
        source: format!("{}{}", source_prefix, account.id),
    }
}

fn codex_launch_credential_snapshot_for_account_id(
    account_id: &str,
    source_prefix: &str,
) -> Option<CodexLaunchCredentialSnapshot> {
    let account_id = account_id.trim();
    if account_id.is_empty() {
        return None;
    }

    if codex_instance::is_api_service_bind_account_id(account_id)
        || codex_instance::parse_provider_gateway_bind_account_id(account_id).is_some()
        || codex_local_access::is_local_access_runtime_account_id(account_id)
    {
        return Some(CodexLaunchCredentialSnapshot {
            kind: "api".to_string(),
            source: format!("{}{}", source_prefix, account_id),
        });
    }

    codex_account::load_account(account_id)
        .map(|account| codex_launch_credential_snapshot_for_account(&account, source_prefix))
}

fn read_current_codex_launch_credential_snapshot() -> Option<CodexLaunchCredentialSnapshot> {
    let codex_home = codex_account::get_codex_home();
    if let Some(account_id) =
        codex_account::read_managed_projection_account_id_from_dir(&codex_home)
    {
        if let Some(snapshot) =
            codex_launch_credential_snapshot_for_account_id(&account_id, "profile:")
        {
            return Some(snapshot);
        }
    }

    if let Ok(settings) = codex_instance::load_default_settings() {
        if let Some(bind_account_id) = settings.bind_account_id.as_deref() {
            if let Some(snapshot) =
                codex_launch_credential_snapshot_for_account_id(bind_account_id, "default-bind:")
            {
                return Some(snapshot);
            }
        }
    }

    codex_account::get_current_account()
        .as_ref()
        .map(|account| codex_launch_credential_snapshot_for_account(account, "current-index:"))
}

fn log_session_visibility_repair_after_credential_kind_change(
    context: &str,
    before: Option<CodexLaunchCredentialSnapshot>,
    after: Option<CodexLaunchCredentialSnapshot>,
    auto_repair_mode: Option<codex_session_visibility::CodexSessionVisibilityAutoRepairMode>,
) {
    let (Some(before), Some(after)) = (before, after) else {
        return;
    };
    if before.kind == after.kind {
        return;
    }

    let auto_repair_mode = auto_repair_mode.unwrap_or_default();
    logger::log_info(&format!(
        "[Codex Session Visibility] {}: credential kind changed, defer quick repair to frontend notice, mode={}, from_kind={}, to_kind={}, from_source={}, to_source={}",
        context,
        auto_repair_mode.label(),
        before.kind,
        after.kind,
        before.source,
        after.source
    ));
}

async fn activate_local_access(
    auto_repair_mode: Option<codex_session_visibility::CodexSessionVisibilityAutoRepairMode>,
) -> Result<LocalAccessActivateResult, String> {
    let flow_started = Instant::now();
    logger::log_info("[Codex API Service Switch][Adapter] localAccess.activate started");
    let codex_home = codex_account::get_codex_home();
    let previous_credential = read_current_codex_launch_credential_snapshot();
    logger::log_info(&format!(
        "[Codex API Service Switch][Adapter] previous credential resolved: elapsed_ms={}",
        flow_started.elapsed().as_millis()
    ));

    let activate_started = Instant::now();
    let state = codex_local_access::activate_local_access_for_dir(&codex_home).await?;
    logger::log_info(&format!(
        "[Codex API Service Switch][Adapter] activate_local_access_for_dir finished: elapsed_ms={}, total_ms={}",
        activate_started.elapsed().as_millis(),
        flow_started.elapsed().as_millis()
    ));

    let api_service_speed = codex_speed::get_api_service_app_speed_config()?.speed;
    let speed_started = Instant::now();
    codex_speed::write_official_app_speed(api_service_speed.clone())?;
    logger::log_info(&format!(
        "[Codex API Service Switch][Adapter] write official app speed finished: elapsed_ms={}, total_ms={}",
        speed_started.elapsed().as_millis(),
        flow_started.elapsed().as_millis()
    ));

    let index_started = Instant::now();
    let mut index = codex_account::load_account_index();
    index.current_account_id = None;
    codex_account::save_account_index(&index)?;
    logger::log_info(&format!(
        "[Codex API Service Switch][Adapter] account index cleared: elapsed_ms={}, total_ms={}",
        index_started.elapsed().as_millis(),
        flow_started.elapsed().as_millis()
    ));

    let default_settings_started = Instant::now();
    if let Err(e) = codex_instance::update_default_settings(
        Some(Some(
            codex_instance::CODEX_API_SERVICE_BIND_ACCOUNT_ID.to_string(),
        )),
        None,
        Some(false),
        None,
        None,
    ) {
        logger::log_warn(&format!("更新 Codex 默认实例为 API 服务模式失败: {}", e));
    } else {
        logger::log_info("已同步更新 Codex 默认实例为 API 服务模式");
    }
    if let Err(e) = codex_instance::update_default_app_speed(api_service_speed) {
        logger::log_warn(&format!("更新 Codex 默认实例 API 服务速度失败: {}", e));
    }
    logger::log_info(&format!(
        "[Codex API Service Switch][Adapter] default settings update finished: elapsed_ms={}, total_ms={}",
        default_settings_started.elapsed().as_millis(),
        flow_started.elapsed().as_millis()
    ));

    log_session_visibility_repair_after_credential_kind_change(
        "after-api-service-activate",
        previous_credential,
        Some(CodexLaunchCredentialSnapshot {
            kind: "api".to_string(),
            source: format!(
                "target-bind:{}",
                codex_instance::CODEX_API_SERVICE_BIND_ACCOUNT_ID
            ),
        }),
        auto_repair_mode,
    );

    let user_config = config::get_user_config();
    logger::log_info("API 服务启动模式下跳过 OpenCode / OpenClaw OAuth 同步");
    logger::log_info(&format!(
        "[Codex API Service Switch][Adapter] localAccess.activate finished: total_ms={}",
        flow_started.elapsed().as_millis()
    ));
    Ok(LocalAccessActivateResult {
        state,
        launch_on_switch: user_config.codex_launch_on_switch,
    })
}

async fn switch_codex_account(
    account_id: String,
    auto_repair_mode: Option<codex_session_visibility::CodexSessionVisibilityAutoRepairMode>,
) -> Result<SwitchCodexAccountResult, String> {
    let account_id = account_id.trim().to_string();
    let flow_started = Instant::now();
    logger::log_info(&format!(
        "[Codex Switch][Adapter] switch_codex_account started: account_id={}",
        account_id
    ));

    let previous_credential = read_current_codex_launch_credential_snapshot();
    logger::log_info(&format!(
        "[Codex Switch][Adapter] previous credential resolved: account_id={}, elapsed_ms={}",
        account_id,
        flow_started.elapsed().as_millis()
    ));

    let switch_started = Instant::now();
    let account = codex_account::switch_account_managed(&account_id).await?;
    logger::log_info(&format!(
        "[Codex Switch][Adapter] switch_account_managed finished: account_id={}, elapsed_ms={}, total_ms={}",
        account_id,
        switch_started.elapsed().as_millis(),
        flow_started.elapsed().as_millis()
    ));

    let account_speed = account.app_speed.clone();
    let speed_started = Instant::now();
    codex_speed::write_official_app_speed(account_speed.clone())?;
    logger::log_info(&format!(
        "[Codex Switch][Adapter] write official app speed finished: account_id={}, elapsed_ms={}, total_ms={}",
        account_id,
        speed_started.elapsed().as_millis(),
        flow_started.elapsed().as_millis()
    ));

    log_session_visibility_repair_after_credential_kind_change(
        "after-account-switch",
        previous_credential,
        Some(codex_launch_credential_snapshot_for_account(
            &account,
            "target-account:",
        )),
        auto_repair_mode,
    );

    let default_settings_started = Instant::now();
    if let Err(e) = codex_instance::update_default_settings(
        Some(Some(account_id.clone())),
        None,
        Some(false),
        None,
        None,
    ) {
        logger::log_warn(&format!("更新 Codex 默认实例绑定账号失败: {}", e));
    } else {
        logger::log_info(&format!(
            "已同步更新 Codex 默认实例绑定账号: {}",
            account_id
        ));
    }
    if let Err(e) = codex_instance::update_default_app_speed(account_speed) {
        logger::log_warn(&format!("更新 Codex 默认实例速度失败: {}", e));
    }
    logger::log_info(&format!(
        "[Codex Switch][Adapter] default settings update finished: account_id={}, elapsed_ms={}, total_ms={}",
        account_id,
        default_settings_started.elapsed().as_millis(),
        flow_started.elapsed().as_millis()
    ));

    let user_config = config::get_user_config();
    let mut opencode_updated = false;
    if user_config.opencode_auth_overwrite_on_switch {
        match opencode_auth::replace_openai_entry_from_codex(&account) {
            Ok(()) => {
                opencode_updated = true;
            }
            Err(e) => {
                logger::log_warn(&format!("OpenCode auth.json 更新跳过: {}", e));
            }
        }
    } else {
        logger::log_info("已关闭切换 Codex 时覆盖 OpenCode 登录信息");
    }

    let opencode_restart_app_path = if user_config.opencode_sync_on_switch {
        if user_config.opencode_auth_overwrite_on_switch && opencode_updated {
            Some(user_config.opencode_app_path.clone())
        } else if !user_config.opencode_auth_overwrite_on_switch {
            logger::log_info("OpenCode 登录覆盖已关闭，跳过自动重启");
            None
        } else {
            logger::log_info("OpenCode 未更新 auth.json，跳过启动/重启");
            None
        }
    } else {
        logger::log_info("已关闭 OpenCode 自动重启");
        None
    };

    if user_config.openclaw_auth_overwrite_on_switch {
        match openclaw_auth::replace_openai_codex_entry_from_codex(&account) {
            Ok(()) => {}
            Err(e) => {
                logger::log_warn(&format!("OpenClaw auth 同步失败: {}", e));
            }
        }
    } else {
        logger::log_info("已关闭切换 Codex 时覆盖 OpenClaw 登录信息");
    }

    let restart_specified_app_path = if user_config.codex_restart_specified_app_on_switch {
        let path = user_config.codex_specified_app_path.trim();
        if path.is_empty() {
            logger::log_warn("已开启切换 Codex 时自动重启指定应用，但未配置应用路径，已跳过");
            None
        } else {
            Some(path.to_string())
        }
    } else {
        logger::log_info("已关闭切换 Codex 时自动重启指定应用");
        None
    };

    logger::log_info(&format!(
        "[Codex Switch][Adapter] switch_codex_account finished: account_id={}, total_ms={}",
        account_id,
        flow_started.elapsed().as_millis()
    ));

    Ok(SwitchCodexAccountResult {
        account,
        post_actions: CodexSwitchPostActions {
            codex_launch_on_switch: user_config.codex_launch_on_switch,
            opencode_restart_app_path,
            restart_specified_app_path,
        },
    })
}

fn refresh_current_quota(runtime: &Runtime) -> Result<Value, String> {
    let Some(account) = codex_account::get_current_account() else {
        return Err("未找到当前 Codex 账号".to_string());
    };
    if account.is_api_key_auth() {
        return Ok(Value::Null);
    }
    to_value(runtime.block_on(codex_quota::refresh_account_quota(&account.id))?)
}

fn refresh_imported_accounts(
    runtime: &Runtime,
    accounts: Vec<CodexAccount>,
) -> Result<Vec<CodexAccount>, String> {
    let mut result = Vec::with_capacity(accounts.len());
    for account in accounts {
        if account.is_api_key_auth() {
            result.push(account);
            continue;
        }

        match runtime.block_on(codex_quota::refresh_account_quota(&account.id)) {
            Ok(_) => {}
            Err(error) => logger::log_warn(&format!(
                "Codex adapter 导入后刷新配额失败: account_id={}, email={}, error={}",
                account.id, account.email, error
            )),
        }

        result.push(codex_account::load_account(&account.id).unwrap_or(account));
    }
    Ok(result)
}

fn import_from_local(runtime: &Runtime) -> Result<Value, String> {
    let account = codex_account::import_from_local()?;
    let mut accounts = refresh_imported_accounts(runtime, vec![account])?;
    to_value(
        accounts
            .pop()
            .ok_or_else(|| "账号导入后无法读取".to_string())?,
    )
}

fn import_from_json(runtime: &Runtime, json_content: String) -> Result<Value, String> {
    let accounts = runtime.block_on(codex_account::import_from_json(&json_content))?;
    to_value(refresh_imported_accounts(runtime, accounts)?)
}

fn import_from_files(runtime: &Runtime, file_paths: Vec<String>) -> Result<Value, String> {
    let result = runtime.block_on(codex_account::import_from_files(file_paths))?;
    to_value(codex_account::CodexFileImportResult {
        imported: refresh_imported_accounts(runtime, result.imported)?,
        failed: result.failed,
    })
}

fn save_oauth_tokens(
    runtime: &Runtime,
    tokens: CodexTokens,
    reauth_account_id: Option<&str>,
) -> Result<CodexAccount, String> {
    let account = if let Some(account_id) = reauth_account_id.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }) {
        codex_account::upsert_account_for_reauth(tokens, account_id)?
    } else {
        codex_account::upsert_account(tokens)?
    };

    if let Err(error) = runtime.block_on(codex_quota::refresh_account_quota(&account.id)) {
        logger::log_error(&format!("刷新配额失败: {}", error));
    }

    let loaded =
        codex_account::load_account(&account.id).ok_or_else(|| "账号保存后无法读取".to_string())?;
    logger::log_info(&format!(
        "Codex OAuth 账号已保存: account_id={}, email={}",
        loaded.id, loaded.email
    ));
    Ok(loaded)
}

fn start_oauth_login(runtime: &Runtime) -> Result<Value, String> {
    logger::log_info("Codex OAuth start adapter 命令触发");
    let response = runtime.block_on(codex_oauth::start_oauth_login_with_event_emitter(
        host_event_emitter(),
    ))?;
    logger::log_info(&format!(
        "Codex OAuth start adapter 命令成功: login_id={}",
        response.login_id
    ));
    to_value(response)
}

fn complete_oauth_login(runtime: &Runtime, payload: OAuthCompletePayload) -> Result<Value, String> {
    let started = Instant::now();
    logger::log_info(&format!(
        "Codex OAuth completed adapter 命令开始: login_id={}",
        payload.login_id
    ));
    let tokens = match runtime.block_on(codex_oauth::complete_oauth_login(&payload.login_id)) {
        Ok(tokens) => tokens,
        Err(error) => {
            logger::log_error(&format!(
                "Codex OAuth completed adapter 命令失败: login_id={}, duration_ms={}, error={}",
                payload.login_id,
                started.elapsed().as_millis(),
                error
            ));
            return Err(error);
        }
    };
    let account = save_oauth_tokens(runtime, tokens, payload.reauth_account_id.as_deref())?;
    logger::log_info(&format!(
        "Codex OAuth completed adapter 命令成功: login_id={}, duration_ms={}, account_id={}, account_email={}",
        payload.login_id,
        started.elapsed().as_millis(),
        account.id,
        account.email
    ));
    to_value(account)
}

fn cancel_oauth_login(payload: OAuthCancelPayload) -> Result<Value, String> {
    logger::log_info(&format!(
        "Codex OAuth cancel adapter 命令触发: login_id={}",
        payload.login_id.as_deref().unwrap_or("<none>")
    ));
    let result = codex_oauth::cancel_oauth_flow_for(payload.login_id.as_deref());
    logger::log_info(&format!(
        "Codex OAuth cancel adapter 命令返回: {:?}",
        result.as_ref().map(|_| "ok").map_err(|error| error)
    ));
    to_value(result?)
}

fn submit_oauth_callback_url(payload: OAuthCallbackPayload) -> Result<Value, String> {
    codex_oauth::submit_callback_url(payload.login_id.as_str(), payload.callback_url.as_str())?;
    let event_payload = json!({ "loginId": payload.login_id });
    emit_host_event("codex-oauth-login-completed", event_payload.clone())?;
    emit_host_event("ghcp-oauth-login-completed", event_payload)?;
    to_value(())
}

fn restore_pending_oauth_listener() -> Result<Value, String> {
    codex_oauth::restore_pending_oauth_listener_with_event_emitter(host_event_emitter());
    to_value(())
}

fn restore_codex_runtime(runtime: &Runtime) -> Result<Value, String> {
    runtime.block_on(codex_local_access::restore_local_access_gateway());
    restore_pending_oauth_listener()?;

    let event_emitter = wakeup_scheduler_event_emitter();
    wakeup_scheduler::restore_state_from_disk();
    wakeup_scheduler::ensure_started_with_event_emitter(None, Some(event_emitter.clone()));
    wakeup_scheduler::trigger_startup_tasks_if_needed_with_event_emitter(None, Some(event_emitter));

    to_value(())
}

fn shutdown_codex_runtime_for_app_exit(runtime: &Runtime) -> Result<Value, String> {
    runtime.block_on(codex_local_access::shutdown_local_access_gateway_for_app_exit());
    to_value(())
}

fn add_token_account(runtime: &Runtime, payload: TokenAccountPayload) -> Result<Value, String> {
    let tokens = CodexTokens {
        id_token: payload.id_token,
        access_token: payload.access_token,
        refresh_token: payload.refresh_token,
    };
    let account = codex_account::upsert_account(tokens)?;
    if let Err(error) = runtime.block_on(codex_quota::refresh_account_quota(&account.id)) {
        logger::log_error(&format!("刷新配额失败: {}", error));
    }
    to_value(
        codex_account::load_account(&account.id).ok_or_else(|| "账号保存后无法读取".to_string())?,
    )
}

fn add_api_key_account(payload: ApiKeyAccountPayload) -> Result<Value, String> {
    let account = codex_account::upsert_api_key_account(
        payload.api_key,
        payload.api_base_url,
        payload.api_provider_mode,
        payload.api_provider_id,
        payload.api_provider_name,
        payload.api_model_catalog.unwrap_or_default(),
        payload.api_wire_api,
        payload.api_supports_vision.unwrap_or(false),
        payload.api_model_vision_support.unwrap_or_default(),
        payload.api_vision_routing_model,
        payload.account_name,
    )?;
    to_value(
        codex_account::load_account(&account.id).ok_or_else(|| "账号保存后无法读取".to_string())?,
    )
}

fn update_api_key_credentials(payload: ApiKeyCredentialsPayload) -> Result<Value, String> {
    to_value(codex_account::update_api_key_credentials(
        &payload.account_id,
        payload.api_key,
        payload.api_base_url,
        payload.api_provider_mode,
        payload.api_provider_id,
        payload.api_provider_name,
        payload.api_model_catalog.unwrap_or_default(),
        payload.api_wire_api,
        payload.api_supports_vision.unwrap_or(false),
        payload.api_model_vision_support.unwrap_or_default(),
        payload.api_vision_routing_model,
    )?)
}

fn update_api_key_bound_oauth_account(
    runtime: &Runtime,
    payload: ApiKeyBoundOAuthPayload,
) -> Result<Value, String> {
    to_value(
        runtime.block_on(codex_account::update_api_key_bound_oauth_account(
            &payload.account_id,
            payload.bound_oauth_account_id,
            payload.bound_oauth_use_local_gateway.unwrap_or(false),
        ))?,
    )
}

fn update_local_access_bound_oauth_account(
    runtime: &Runtime,
    payload: LocalAccessBoundOAuthPayload,
) -> Result<Value, String> {
    to_value(
        runtime.block_on(codex_local_access::update_local_access_bound_oauth_account(
            payload.bound_oauth_account_id,
            payload.bound_oauth_use_local_gateway.unwrap_or(false),
        ))?,
    )
}

fn delete_account(runtime: &Runtime, account_id: String) -> Result<Value, String> {
    let account_ids = vec![account_id];
    codex_account::remove_account(&account_ids[0])?;
    runtime.block_on(
        codex_local_access::remove_deleted_accounts_from_local_access_pool(&account_ids),
    )?;
    Ok(Value::Null)
}

fn delete_accounts(runtime: &Runtime, account_ids: Vec<String>) -> Result<Value, String> {
    codex_account::remove_accounts(&account_ids)?;
    runtime.block_on(
        codex_local_access::remove_deleted_accounts_from_local_access_pool(&account_ids),
    )?;
    Ok(Value::Null)
}

fn load_account_groups() -> Result<Value, String> {
    let path = account::get_data_dir()?.join(CODEX_GROUPS_FILE);
    if !path.exists() {
        return to_value("[]");
    }
    to_value(
        std::fs::read_to_string(&path)
            .map_err(|error| format!("Failed to read codex groups: {}", error))?,
    )
}

fn save_account_groups(data: String) -> Result<Value, String> {
    let dir = account::get_data_dir()?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("Failed to create dir: {}", error))?;
    }
    let path = dir.join(CODEX_GROUPS_FILE);
    std::fs::write(&path, data)
        .map_err(|error| format!("Failed to write codex groups: {}", error))?;
    Ok(Value::Null)
}

fn load_model_providers() -> Result<Value, String> {
    let path = account::get_data_dir()?.join(CODEX_MODEL_PROVIDERS_FILE);
    if !path.exists() {
        return to_value("[]");
    }
    to_value(
        std::fs::read_to_string(&path)
            .map_err(|error| format!("Failed to read codex model providers: {}", error))?,
    )
}

fn save_model_providers(data: String) -> Result<Value, String> {
    let dir = account::get_data_dir()?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("Failed to create dir: {}", error))?;
    }
    let path = dir.join(CODEX_MODEL_PROVIDERS_FILE);
    std::fs::write(&path, data)
        .map_err(|error| format!("Failed to write codex model providers: {}", error))?;
    Ok(Value::Null)
}

fn json_path<'a>(root: Option<&'a Value>, path: &[&str]) -> Option<&'a Value> {
    let mut current = root?;
    for key in path {
        current = current.as_object()?.get(*key)?;
    }
    Some(current)
}

fn normalize_provider_base_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn load_model_providers_array() -> Result<Vec<Value>, String> {
    let raw = match load_model_providers()? {
        Value::String(value) => value,
        _ => "[]".to_string(),
    };
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

fn find_codex_provider_for_account(providers: &[Value], account: &CodexAccount) -> Option<Value> {
    let provider_id = account
        .api_provider_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(provider_id) = provider_id {
        if let Some(provider) = providers.iter().find(|provider| {
            json_path(Some(provider), &["id"])
                .and_then(Value::as_str)
                .map(str::trim)
                == Some(provider_id)
        }) {
            return Some(provider.clone());
        }
    }

    let account_base = account
        .api_base_url
        .as_deref()
        .map(normalize_provider_base_url)
        .filter(|value| !value.is_empty())?;
    providers
        .iter()
        .find(|provider| {
            json_path(Some(provider), &["baseUrl"])
                .and_then(Value::as_str)
                .map(normalize_provider_base_url)
                == Some(account_base.clone())
        })
        .cloned()
}

fn save_detected_codex_provider_integration_type(
    provider_id: Option<&str>,
    base_url: &str,
    mode: &str,
) -> Result<(), String> {
    if mode != "new_api" && mode != "sub2api" {
        return Ok(());
    }

    let raw = match load_model_providers()? {
        Value::String(value) => value,
        _ => "[]".to_string(),
    };
    let mut providers: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("解析 Codex 模型供应商失败: {}", error))?;
    let Some(items) = providers.as_array_mut() else {
        return Ok(());
    };

    let normalized_base_url = normalize_provider_base_url(base_url);
    let mut changed = false;
    for provider in items {
        let id_matches = provider_id
            .map(|target_id| {
                json_path(Some(provider), &["id"])
                    .and_then(Value::as_str)
                    .map(str::trim)
                    == Some(target_id)
            })
            .unwrap_or(false);
        let base_matches = json_path(Some(provider), &["baseUrl"])
            .and_then(Value::as_str)
            .map(normalize_provider_base_url)
            == Some(normalized_base_url.clone());

        if id_matches || base_matches {
            if provider
                .get("integrationType")
                .and_then(Value::as_str)
                .map(str::trim)
                != Some(mode)
            {
                if let Some(object) = provider.as_object_mut() {
                    object.insert(
                        "integrationType".to_string(),
                        Value::String(mode.to_string()),
                    );
                    object.insert(
                        "updatedAt".to_string(),
                        Value::Number(serde_json::Number::from(now_unix_millis())),
                    );
                    changed = true;
                }
            }
            break;
        }
    }

    if changed {
        let data = serde_json::to_string_pretty(&providers)
            .map_err(|error| format!("序列化 Codex 模型供应商失败: {}", error))?;
        save_model_providers(data)?;
    }

    Ok(())
}

fn refresh_codex_api_key_usage(runtime: &Runtime, account_id: String) -> Result<Value, String> {
    let mut account = codex_account::list_accounts()
        .into_iter()
        .find(|account| account.id == account_id)
        .ok_or_else(|| "未找到 Codex API Key 账号".to_string())?;

    if !account.is_api_key_auth() {
        return to_value(runtime.block_on(codex_quota::refresh_account_quota(&account.id))?);
    }

    let api_key = account
        .openai_api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Codex API Key 为空".to_string())?;
    let providers = load_model_providers_array()?;
    let provider = find_codex_provider_for_account(&providers, &account);
    let base_url = provider
        .as_ref()
        .and_then(|provider| json_path(Some(provider), &["baseUrl"]))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            account
                .api_base_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .ok_or_else(|| "Codex API Base URL 为空".to_string())?
        .to_string();
    let integration_type = provider
        .as_ref()
        .and_then(|provider| json_path(Some(provider), &["integrationType"]))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let summary = runtime.block_on(codex_model_provider::query_usage(
        base_url.clone(),
        api_key.to_string(),
        integration_type,
    ))?;
    let summary_value = serde_json::to_value(&summary)
        .map_err(|error| format!("序列化 Codex API Key 用量失败: {}", error))?;

    let mut raw_data = account
        .quota
        .as_ref()
        .and_then(|quota| quota.raw_data.clone())
        .unwrap_or_else(|| json!({}));
    if !raw_data.is_object() {
        raw_data = json!({});
    }
    if let Some(object) = raw_data.as_object_mut() {
        object.insert("provider_usage".to_string(), summary_value);
    }

    account.quota = Some(CodexQuota {
        hourly_percentage: 0,
        hourly_reset_time: None,
        hourly_window_minutes: None,
        hourly_window_present: Some(false),
        weekly_percentage: 0,
        weekly_reset_time: None,
        weekly_window_minutes: None,
        weekly_window_present: Some(false),
        reset_credits_available: None,
        reset_credits: Vec::new(),
        reset_credits_next_expires_at: None,
        raw_data: Some(raw_data),
    });
    account.quota_error = None;
    account.usage_updated_at = Some(now_unix_seconds());
    codex_account::save_account(&account)?;

    if let Some(mode) = summary.mode.as_deref() {
        let provider_id = provider
            .as_ref()
            .and_then(|provider| json_path(Some(provider), &["id"]))
            .and_then(Value::as_str);
        save_detected_codex_provider_integration_type(provider_id, &base_url, mode)?;
    }

    to_value(account)
}

fn codex_config_toml_path() -> Result<Value, String> {
    to_value(
        codex_account::get_codex_home()
            .join("config.toml")
            .to_string_lossy()
            .to_string(),
    )
}

fn save_api_service_app_speed(speed: CodexAppSpeed) -> Result<Value, String> {
    let saved = codex_speed::save_api_service_app_speed(speed.clone())?;
    if let Ok(settings) = codex_instance::load_default_settings() {
        if settings.bind_account_id.as_deref()
            == Some(codex_instance::CODEX_API_SERVICE_BIND_ACCOUNT_ID)
        {
            let _ = codex_instance::update_default_app_speed(speed);
        }
    }
    codex_local_access::trigger_gateway_reload_in_background("保存 API 服务速度配置");
    to_value(saved)
}

fn update_account_app_speed(payload: AccountAppSpeedPayload) -> Result<Value, String> {
    let account = codex_account::update_account_app_speed(&payload.account_id, payload.speed)?;
    let account_speed = account.app_speed.clone();
    let current_account_id = codex_account::load_account_index().current_account_id;
    let provider_gateway_bind_account_id =
        codex_instance::provider_gateway_bind_account_id(&payload.account_id);
    let default_bind_account_id = codex_instance::load_default_settings()
        .ok()
        .and_then(|settings| settings.bind_account_id);
    let default_bind_matches_provider_gateway = provider_gateway_bind_account_id
        .as_deref()
        .map(|bind_account_id| default_bind_account_id.as_deref() == Some(bind_account_id))
        .unwrap_or(false);

    if current_account_id.as_deref() == Some(payload.account_id.as_str())
        || default_bind_account_id.as_deref() == Some(payload.account_id.as_str())
        || default_bind_matches_provider_gateway
    {
        codex_speed::write_official_app_speed(account_speed.clone())?;
        let _ = codex_instance::update_default_app_speed(account_speed.clone());
        if default_bind_matches_provider_gateway {
            if let Ok(default_dir) = codex_instance::get_default_codex_home() {
                codex_local_access::reload_provider_gateway_for_profile_in_background(
                    default_dir,
                    payload.account_id.clone(),
                    "更新默认 provider gateway 账号速度配置",
                );
            }
        }
    }

    let bound_instances = codex_instance::update_bound_instances_app_speed(
        &payload.account_id,
        account_speed.clone(),
    )?;
    for instance in bound_instances {
        codex_speed::write_app_speed_for_dir(
            std::path::Path::new(&instance.user_data_dir),
            account_speed.clone(),
        )?;
    }

    if let Some(provider_gateway_bind_account_id) = provider_gateway_bind_account_id.as_deref() {
        let provider_gateway_bound_instances = codex_instance::update_bound_instances_app_speed(
            provider_gateway_bind_account_id,
            account_speed.clone(),
        )?;
        for instance in provider_gateway_bound_instances {
            codex_speed::write_app_speed_for_dir(
                std::path::Path::new(&instance.user_data_dir),
                account_speed.clone(),
            )?;
            codex_local_access::reload_provider_gateway_for_profile_in_background(
                std::path::PathBuf::from(instance.user_data_dir),
                payload.account_id.clone(),
                "更新 provider gateway 账号速度配置",
            );
        }
    }

    to_value(account)
}

fn set_codex_launch_on_switch(enabled: bool) -> Result<Value, String> {
    let current = config::get_user_config();
    if current.codex_launch_on_switch == enabled {
        return Ok(Value::Null);
    }
    let next = config::UserConfig {
        codex_launch_on_switch: enabled,
        ..current
    };
    config::save_user_config(&next)?;
    Ok(Value::Null)
}

fn set_codex_local_access_entry_visible(enabled: bool) -> Result<Value, String> {
    let current = config::get_user_config();
    if current.codex_local_access_entry_visible == enabled {
        return Ok(Value::Null);
    }
    let next = config::UserConfig {
        codex_local_access_entry_visible: enabled,
        ..current
    };
    config::save_user_config(&next)?;
    Ok(Value::Null)
}

fn codex_keepalive_allow_attempt(key: &str) -> bool {
    let now = now_unix_seconds();
    let Ok(state) = CODEX_KEEPALIVE_NEXT_ALLOWED.lock() else {
        return true;
    };
    state.get(key).map(|next| *next <= now).unwrap_or(true)
}

fn codex_keepalive_clear_backoff(key: &str) {
    if let Ok(mut state) = CODEX_KEEPALIVE_NEXT_ALLOWED.lock() {
        state.remove(key);
    }
}

fn codex_keepalive_mark_failure(key: &str) {
    if let Ok(mut state) = CODEX_KEEPALIVE_NEXT_ALLOWED.lock() {
        state.insert(
            key.to_string(),
            now_unix_seconds() + CODEX_KEEPALIVE_FAILURE_BACKOFF_SECONDS,
        );
    }
}

fn keepalive_due_codex_accounts(runtime: &Runtime) -> Result<i32, String> {
    let accounts = codex_account::list_accounts_checked()?;
    let mut refreshed = 0i32;

    for account in accounts
        .into_iter()
        .filter(|account| !account.is_api_key_auth())
    {
        if !account.requires_reauth && !codex_account::is_managed_auth_refresh_due(&account) {
            continue;
        }

        let key = format!("codex:{}", account.id);
        if !codex_keepalive_allow_attempt(&key) {
            continue;
        }

        match runtime.block_on(codex_account::keepalive_managed_account(
            &account.id,
            "TokenKeeper 授权保活",
        )) {
            Ok(updated) => {
                codex_keepalive_clear_backoff(&key);
                refreshed += 1;
                logger::log_info(&format!(
                    "[TokenKeeper][Codex] Token 保活成功: account_id={}, email={}",
                    updated.id, updated.email
                ));
            }
            Err(err) => {
                codex_keepalive_mark_failure(&key);
                logger::log_warn(&format!(
                    "[TokenKeeper][Codex] Token 保活失败，进入退避: account_id={}, error={}",
                    account.id, err
                ));
            }
        }
    }

    Ok(refreshed)
}

fn handle_rpc(runtime: &Runtime, request: RpcRequest) -> Result<Value, String> {
    match request.method.as_str() {
        "health.check" => Ok(json!({ "status": "ok" })),
        "adapter.shutdown" => Ok(Value::Null),
        "runtime.restore" => restore_codex_runtime(runtime),
        "runtime.shutdownForAppExit" => shutdown_codex_runtime_for_app_exit(runtime),
        "accounts.list" => to_value(codex_account::list_accounts_checked()?),
        "accounts.current" => to_value(codex_account::get_current_account()),
        "accounts.pickAutoSwitchTarget" => {
            to_value(codex_account::pick_auto_switch_target_if_needed()?)
        }
        "switch.account" => {
            let payload: SwitchCodexAccountPayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(switch_codex_account(
                payload.account_id,
                payload.auto_repair_mode,
            ))?)
        }
        "accounts.delete" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            delete_account(runtime, payload.account_id)
        }
        "accounts.deleteMany" => {
            let payload: AccountIdsPayload = parse_payload(request.payload)?;
            delete_accounts(runtime, payload.account_ids)
        }
        "accounts.addToken" => {
            let payload: TokenAccountPayload = parse_payload(request.payload)?;
            add_token_account(runtime, payload)
        }
        "accounts.addApiKey" => {
            let payload: ApiKeyAccountPayload = parse_payload(request.payload)?;
            add_api_key_account(payload)
        }
        "accounts.updateName" => {
            let payload: AccountNamePayload = parse_payload(request.payload)?;
            to_value(codex_account::update_account_name(
                &payload.account_id,
                payload.name,
            )?)
        }
        "accounts.updateApiKeyCredentials" => {
            let payload: ApiKeyCredentialsPayload = parse_payload(request.payload)?;
            update_api_key_credentials(payload)
        }
        "accounts.updateApiKeyBoundOAuthAccount" => {
            let payload: ApiKeyBoundOAuthPayload = parse_payload(request.payload)?;
            update_api_key_bound_oauth_account(runtime, payload)
        }
        "accounts.updateTags" => {
            let payload: TagsPayload = parse_payload(request.payload)?;
            to_value(codex_account::update_account_tags(
                &payload.account_id,
                payload.tags,
            )?)
        }
        "accounts.updateNote" => {
            let payload: NotePayload = parse_payload(request.payload)?;
            to_value(codex_account::update_account_note(
                &payload.account_id,
                payload.note,
            )?)
        }
        "accounts.keepaliveDue" => to_value(keepalive_due_codex_accounts(runtime)?),
        "accounts.refreshApiKeyUsage" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            refresh_codex_api_key_usage(runtime, payload.account_id)
        }
        "accounts.loadGroups" => load_account_groups(),
        "accounts.saveGroups" => {
            let payload: AccountGroupsPayload = parse_payload(request.payload)?;
            save_account_groups(payload.data)
        }
        "accounts.importFromLocal" => import_from_local(runtime),
        "accounts.importFromJson" => {
            let payload: JsonContentPayload = parse_payload(request.payload)?;
            import_from_json(runtime, payload.json_content)
        }
        "accounts.importFromFiles" => {
            let payload: FilePathsPayload = parse_payload(request.payload)?;
            import_from_files(runtime, payload.file_paths)
        }
        "accounts.batchImport.startFromFiles" => {
            let payload: BatchImportStartPayload = parse_payload(request.payload)?;
            to_value(
                codex_account::start_codex_batch_import_from_files_with_emitter(
                    runtime.handle().clone(),
                    payload.file_paths,
                    payload.check_quota,
                    codex_batch_import_event_emitter(),
                )?,
            )
        }
        "accounts.batchImport.cancel" => {
            let payload: BatchImportSessionPayload = parse_payload(request.payload)?;
            codex_account::cancel_codex_batch_import(&payload.session_id)?;
            Ok(Value::Null)
        }
        "accounts.batchImport.resume" => {
            let payload: BatchImportSessionPayload = parse_payload(request.payload)?;
            codex_account::resume_codex_batch_import_with_emitter(
                runtime.handle().clone(),
                &payload.session_id,
                codex_batch_import_event_emitter(),
            )?;
            Ok(Value::Null)
        }
        "accounts.batchImport.preview" => {
            let payload: BatchImportSessionPayload = parse_payload(request.payload)?;
            to_value(codex_account::get_codex_batch_import_preview(
                &payload.session_id,
            )?)
        }
        "accounts.batchImport.confirm" => {
            let payload: BatchImportConfirmPayload = parse_payload(request.payload)?;
            to_value(codex_account::confirm_codex_batch_import(
                &payload.session_id,
                &payload.item_ids,
            )?)
        }
        "modelProviders.load" => load_model_providers(),
        "modelProviders.save" => {
            let payload: AccountGroupsPayload = parse_payload(request.payload)?;
            save_model_providers(payload.data)
        }
        "modelProviders.testConnection" => {
            let payload: ModelProviderConnectionPayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(codex_model_provider::test_connection(
                payload.base_url,
                payload.api_key,
                payload.wire_api,
            ))?)
        }
        "modelProviders.listModels" => {
            let payload: ModelProviderModelsPayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(codex_model_provider::list_models(
                payload.base_url,
                payload.api_key,
            ))?)
        }
        "modelProviders.queryUsage" => {
            let payload: ModelProviderUsagePayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(codex_model_provider::query_usage(
                payload.base_url,
                payload.api_key,
                payload.integration_type,
            ))?)
        }
        "modelProviders.chatTestBatch" => {
            let payload: ModelProviderChatTestBatchPayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(codex_model_provider::chat_test_batch(
                payload.targets,
                payload.prompt,
                payload.model,
                payload.run_id,
                |progress| {
                    let payload = to_value(progress)?;
                    emit_host_event(CODEX_MODEL_PROVIDER_CHAT_TEST_PROGRESS_EVENT, payload)
                },
            ))?)
        }
        "oauth.isPortInUse" => to_value(process::is_port_in_use(codex_oauth::get_callback_port())?),
        "oauth.closePortProcess" => {
            let killed = process::kill_port_processes(codex_oauth::get_callback_port())?;
            to_value(killed as u32)
        }
        "oauth.start" => start_oauth_login(runtime),
        "oauth.complete" => {
            let payload: OAuthCompletePayload = parse_payload(request.payload)?;
            complete_oauth_login(runtime, payload)
        }
        "oauth.cancel" => {
            let payload: OAuthCancelPayload = parse_payload(request.payload)?;
            cancel_oauth_login(payload)
        }
        "oauth.submitCallbackUrl" => {
            let payload: OAuthCallbackPayload = parse_payload(request.payload)?;
            submit_oauth_callback_url(payload)
        }
        "oauth.restorePendingListener" => restore_pending_oauth_listener(),
        "localAccess.updateBoundOAuthAccount" => {
            let payload: LocalAccessBoundOAuthPayload = parse_payload(request.payload)?;
            update_local_access_bound_oauth_account(runtime, payload)
        }
        "localAccess.getState" => {
            to_value(runtime.block_on(codex_local_access::get_local_access_state())?)
        }
        "localAccess.rotateApiKey" => {
            to_value(runtime.block_on(codex_local_access::rotate_local_access_api_key())?)
        }
        "localAccess.clearStats" => {
            to_value(runtime.block_on(codex_local_access::clear_local_access_stats())?)
        }
        "localAccess.prepareRestart" => to_value(
            runtime.block_on(codex_local_access::prepare_local_access_gateway_for_restart())?,
        ),
        "localAccess.killPort" => {
            to_value(runtime.block_on(codex_local_access::kill_local_access_port_processes())?)
        }
        "localAccess.removeAccount" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::remove_local_access_account(
                    &payload.account_id,
                ))?,
            )
        }
        "localAccess.createApiKey" => {
            let payload: LocalAccessCreateApiKeyPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::create_local_access_api_key(
                    payload.label,
                ))?,
            )
        }
        "localAccess.rotateNamedApiKey" => {
            let payload: LocalAccessApiKeyIdPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::rotate_local_access_named_api_key(
                    payload.api_key_id,
                ))?,
            )
        }
        "localAccess.deleteApiKey" => {
            let payload: LocalAccessApiKeyIdPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::delete_local_access_api_key(
                    payload.api_key_id,
                ))?,
            )
        }
        "localAccess.setEnabled" => {
            let payload: LocalAccessEnabledPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::set_local_access_enabled(
                    payload.enabled,
                ))?,
            )
        }
        "localAccess.activate" => {
            let payload: LocalAccessActivatePayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(activate_local_access(payload.auto_repair_mode))?)
        }
        "localAccess.saveAccounts" => {
            let payload: LocalAccessSaveAccountsPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::save_local_access_accounts(
                    payload.account_ids,
                    payload.restrict_free_accounts.unwrap_or(true),
                ))?,
            )
        }
        "localAccess.updatePort" => {
            let payload: LocalAccessPortPayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(codex_local_access::update_local_access_port(payload.port))?)
        }
        "localAccess.updateRoutingStrategy" => {
            let payload: LocalAccessRoutingStrategyPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::update_local_access_routing_strategy(
                    payload.strategy,
                ))?,
            )
        }
        "localAccess.updateCustomRouting" => {
            let payload: LocalAccessCustomRoutingPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::update_local_access_custom_routing(
                    payload.rules,
                ))?,
            )
        }
        "localAccess.updateAccountModelRules" => {
            let payload: LocalAccessAccountModelRulesPayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(
                codex_local_access::update_local_access_account_model_rules(payload.rules),
            )?)
        }
        "localAccess.updateModelRules" => {
            let payload: LocalAccessModelRulesPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::update_local_access_model_rules(
                    payload.model_aliases,
                    payload.excluded_models,
                ))?,
            )
        }
        "localAccess.updateModelPricings" => {
            let payload: LocalAccessModelPricingsPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::update_local_access_model_pricings(
                    payload.model_pricings,
                ))?,
            )
        }
        "localAccess.updateRoutingOptions" => {
            let payload: LocalAccessRoutingOptionsPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::update_local_access_routing_options(
                    payload.session_affinity,
                    payload.session_affinity_ttl_ms,
                    payload.max_retry_credentials,
                    payload.max_retry_interval_ms,
                    payload.disable_cooling,
                ))?,
            )
        }
        "localAccess.updateTimeouts" => {
            let payload: LocalAccessTimeoutsPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::update_local_access_timeouts(
                    payload.timeouts,
                    payload.active_timeout_preset_id,
                ))?,
            )
        }
        "localAccess.updateTimeoutPresets" => {
            let payload: LocalAccessTimeoutPresetsPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::update_local_access_timeout_presets(
                    payload.timeout_presets,
                    payload.active_timeout_preset_id,
                ))?,
            )
        }
        "localAccess.updateUpstreamProxyConfig" => {
            let payload: LocalAccessUpstreamProxyConfigPayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(
                codex_local_access::update_local_access_upstream_proxy_config(
                    payload.upstream_proxy_url,
                ),
            )?)
        }
        "localAccess.updateGatewayMode" => {
            let payload: LocalAccessGatewayModePayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::update_local_access_gateway_mode(
                    payload.gateway_mode,
                ))?,
            )
        }
        "localAccess.updateDebugLogs" => {
            let payload: LocalAccessDebugLogsPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::update_local_access_debug_logs(
                    payload.debug_logs,
                ))?,
            )
        }
        "localAccess.updateAccessScope" => {
            let payload: LocalAccessAccessScopePayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::update_local_access_scope(
                    payload.access_scope,
                ))?,
            )
        }
        "localAccess.updateClientBaseUrlHost" => {
            let payload: LocalAccessClientBaseUrlHostPayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(
                codex_local_access::update_local_access_client_base_url_host(
                    payload.client_base_url_host,
                ),
            )?)
        }
        "localAccess.updateImageGenerationMode" => {
            let payload: LocalAccessImageGenerationModePayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(
                codex_local_access::update_local_access_image_generation_mode(
                    payload.image_generation_mode,
                ),
            )?)
        }
        "localAccess.updateApiKey" => {
            let payload: LocalAccessUpdateApiKeyPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::update_local_access_api_key(
                    payload.api_key_id,
                    payload.label,
                    payload.enabled,
                    payload.model_prefix,
                    payload.allowed_models,
                    payload.excluded_models,
                ))?,
            )
        }
        "localAccess.queryRequestLogs" => {
            let payload: LocalAccessQueryRequestLogsPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::query_local_access_usage_events(
                    payload.page,
                    payload.page_size,
                    payload.stats_range,
                    payload.model_query,
                    payload.account_query,
                    payload.api_key_query,
                    payload.gateway_mode,
                    payload.request_kind,
                    payload.success,
                    payload.error_category,
                ))?,
            )
        }
        "localAccess.test" => {
            to_value(runtime.block_on(codex_local_access::test_local_access_with_dialog())?)
        }
        "localAccess.chatTest" => {
            let payload: LocalAccessChatTestPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_local_access::chat_local_access_with_dialog(
                    payload.model_id,
                    payload.messages,
                ))?,
            )
        }
        "localAccess.chatTestStream" => {
            let payload: LocalAccessChatTestStreamPayload = parse_payload(request.payload)?;
            let mut event_error: Option<String> = None;
            runtime.block_on(
                codex_local_access::stream_chat_local_access_with_dialog_events(
                    payload.session_id,
                    payload.model_id,
                    payload.messages,
                    |event_payload| {
                        if event_error.is_some() {
                            return;
                        }
                        if let Err(error) = emit_host_event(
                            CODEX_LOCAL_ACCESS_CHAT_TEST_STREAM_EVENT,
                            event_payload,
                        ) {
                            logger::log_warn(&format!(
                                "Codex adapter 转发本地 API 流式测试事件失败: {}",
                                error
                            ));
                            event_error = Some(error);
                        }
                    },
                ),
            )?;
            if let Some(error) = event_error {
                return Err(format!("转发本地 API 流式测试事件失败: {}", error));
            }
            Ok(Value::Null)
        }
        "instances.store.get" => to_value(codex_instance::load_instance_store()?),
        "instances.store.replace" => {
            let payload: InstanceStorePayload = parse_payload(request.payload)?;
            let store = sanitize_instance_store(&payload.store);
            codex_instance::save_instance_store(&store)?;
            Ok(Value::Null)
        }
        "instances.defaults" => to_value(codex_instance::get_instance_defaults()?),
        "instances.list" => to_value(list_codex_instances()?),
        "instances.create" => {
            let payload: CreateCodexInstancePayload = parse_payload(request.payload)?;
            to_value(create_codex_instance(payload)?)
        }
        "instances.update" => {
            let payload: UpdateCodexInstancePayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(update_codex_instance(payload))?)
        }
        "instances.delete" => {
            let payload: InstanceIdPayload = parse_payload(request.payload)?;
            delete_codex_instance(&payload.instance_id)?;
            Ok(Value::Null)
        }
        "instances.start" => {
            let payload: StartCodexInstancePayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(start_codex_instance(
                payload.instance_id,
                payload.skip_default_bind_account_injection.unwrap_or(false),
            ))?)
        }
        "instances.stop" => {
            let payload: InstanceIdPayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(stop_codex_instance(payload.instance_id))?)
        }
        "instances.closeAll" => {
            runtime.block_on(close_all_codex_instances())?;
            Ok(Value::Null)
        }
        "instances.window.open" => {
            let payload: InstanceIdPayload = parse_payload(request.payload)?;
            open_codex_instance_window(&payload.instance_id)?;
            Ok(Value::Null)
        }
        "instances.quickConfig.get" => {
            let payload: InstanceIdPayload = parse_payload(request.payload)?;
            let base_dir = resolve_instance_base_dir(&payload.instance_id)?;
            to_value(codex_account::read_quick_config_from_config_toml(
                &base_dir,
            )?)
        }
        "instances.quickConfig.save" => {
            let payload: InstanceQuickConfigPayload = parse_payload(request.payload)?;
            let base_dir = resolve_instance_base_dir(&payload.instance_id)?;
            to_value(codex_account::save_quick_config_for_base_dir(
                &base_dir,
                payload.model_context_window,
                payload.auto_compact_token_limit,
            )?)
        }
        "instances.configPath" => {
            let payload: InstanceIdPayload = parse_payload(request.payload)?;
            let base_dir = resolve_instance_base_dir(&payload.instance_id)?;
            let path = base_dir.join("config.toml");
            if !path.exists() {
                return Err(format!("未找到实例 config.toml 文件: {}", path.display()));
            }
            to_value(path.to_string_lossy().to_string())
        }
        "instances.launchCommand.get" => {
            let payload: InstanceIdPayload = parse_payload(request.payload)?;
            let context = resolve_instance_launch_context(&payload.instance_id)?;
            to_value(CodexInstanceLaunchInfo {
                instance_id: payload.instance_id,
                user_data_dir: context.user_data_dir.clone(),
                launch_command: build_launch_command(&context)?,
            })
        }
        "sessions.syncThreadsAcrossInstances" => {
            to_value(codex_thread_sync::sync_threads_across_instances()?)
        }
        "sessions.syncToInstance" => {
            let payload: SyncSessionsToInstancePayload = parse_payload(request.payload)?;
            to_value(codex_thread_sync::sync_sessions_to_instance(
                payload.session_ids,
                payload.target_instance_id,
            )?)
        }
        "sessions.visibilityRepairProviders.list" => {
            to_value(codex_session_visibility::list_session_visibility_repair_providers()?)
        }
        "sessions.visibilityRepairInstances.list" => {
            to_value(codex_session_visibility::list_session_visibility_repair_instances()?)
        }
        "sessions.visibilityRepair.run" => {
            let payload: RepairSessionVisibilityPayload = parse_payload(request.payload)?;
            to_value(repair_session_visibility(payload)?)
        }
        "sessions.list" => {
            let payload: SessionSearchPayload = parse_payload(request.payload)?;
            to_value(codex_session_manager::list_sessions_across_instances(
                payload.title_query,
                payload.content_query,
            )?)
        }
        "sessions.tokenStats" => {
            let payload: SessionIdsPayload = parse_payload(request.payload)?;
            to_value(
                codex_session_manager::get_session_token_stats_across_instances(
                    payload.session_ids,
                )?,
            )
        }
        "sessions.moveToTrash" => {
            let payload: SessionIdsPayload = parse_payload(request.payload)?;
            to_value(
                codex_session_manager::move_sessions_to_trash_across_instances(
                    payload.session_ids,
                )?,
            )
        }
        "sessions.listTrash" => {
            to_value(codex_session_manager::list_trashed_sessions_across_instances()?)
        }
        "sessions.restoreFromTrash" => {
            let payload: SessionIdsPayload = parse_payload(request.payload)?;
            to_value(
                codex_session_manager::restore_sessions_from_trash_across_instances(
                    payload.session_ids,
                )?,
            )
        }
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
        "wakeup.test" => {
            let payload: CodexWakeupTestPayload = parse_payload(request.payload)?;
            let progress_emitter = codex_wakeup_progress_emitter();
            to_value(
                runtime.block_on(codex_wakeup::run_batch_with_progress_emitter(
                    None,
                    Some(&progress_emitter),
                    payload.account_ids,
                    payload.prompt,
                    codex_wakeup::CodexWakeupExecutionConfig {
                        model: payload.model,
                        model_display_name: payload.model_display_name,
                        model_reasoning_effort: payload.model_reasoning_effort,
                    },
                    codex_wakeup::TaskRunContext {
                        trigger_type: "test".to_string(),
                        task_id: None,
                        task_name: None,
                    },
                    payload.run_id,
                    payload.cancel_scope_id.as_deref(),
                ))?,
            )
        }
        "wakeup.runTask" => {
            let payload: CodexWakeupRunTaskPayload = parse_payload(request.payload)?;
            let progress_emitter = codex_wakeup_progress_emitter();
            to_value(runtime.block_on(
                codex_wakeup_scheduler::run_task_now_with_progress_emitter(
                    None,
                    Some(&progress_emitter),
                    &payload.task_id,
                    "manual_task",
                    payload.run_id,
                ),
            )?)
        }
        "wakeup.runEnabledTasks" => {
            let payload: CodexWakeupRunEnabledTasksPayload = parse_payload(request.payload)?;
            let trigger = payload
                .trigger_type
                .unwrap_or_else(|| "startup".to_string());
            let progress_emitter = codex_wakeup_progress_emitter();
            to_value(runtime.block_on(
                codex_wakeup_scheduler::run_enabled_tasks_now_with_progress_emitter(
                    None,
                    Some(&progress_emitter),
                    &trigger,
                ),
            )?)
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
        "wakeup.getCliStatus" => to_value(codex_wakeup::wakeup_runtime_status()),
        "wakeup.updateRuntimeConfig" => {
            let payload: codex_wakeup::CodexWakeupRuntimeConfig = parse_payload(request.payload)?;
            codex_wakeup::save_runtime_config(&payload)?;
            to_value(codex_wakeup::wakeup_runtime_status())
        }
        "wakeup.getOverview" => to_value(codex_wakeup::load_overview()?),
        "wakeup.getState" => to_value(codex_wakeup::load_state()?),
        "wakeup.saveState" => {
            let payload: codex_wakeup::CodexWakeupState = parse_payload(request.payload)?;
            to_value(codex_wakeup::save_state(&payload)?)
        }
        "wakeup.loadHistory" => to_value(codex_wakeup::load_history()?),
        "wakeup.clearHistory" => {
            codex_wakeup::clear_history()?;
            Ok(Value::Null)
        }
        "wakeup.cancelScope" => {
            let payload: CancelScopePayload = parse_payload(request.payload)?;
            codex_wakeup::cancel_wakeup_scope(&payload.cancel_scope_id)?;
            Ok(Value::Null)
        }
        "wakeup.releaseScope" => {
            let payload: CancelScopePayload = parse_payload(request.payload)?;
            codex_wakeup::release_wakeup_scope(&payload.cancel_scope_id)?;
            Ok(Value::Null)
        }
        "settings.setLaunchOnSwitch" => {
            let payload: SettingsBoolPayload = parse_payload(request.payload)?;
            set_codex_launch_on_switch(payload.enabled)
        }
        "settings.setLocalAccessEntryVisible" => {
            let payload: SettingsBoolPayload = parse_payload(request.payload)?;
            set_codex_local_access_entry_visible(payload.enabled)
        }
        "config.path" => codex_config_toml_path(),
        "config.quick.get" => to_value(codex_account::load_current_quick_config()?),
        "config.quick.save" => {
            let payload: QuickConfigPayload = parse_payload(request.payload)?;
            to_value(codex_account::save_current_quick_config(
                payload.model_context_window,
                payload.auto_compact_token_limit,
            )?)
        }
        "config.appSpeed.get" => to_value(codex_speed::get_app_speed_config()?),
        "config.appSpeed.save" => {
            let payload: AppSpeedPayload = parse_payload(request.payload)?;
            to_value(codex_speed::save_api_service_app_speed(payload.speed)?)
        }
        "config.apiServiceAppSpeed.get" => {
            to_value(codex_speed::get_api_service_app_speed_config()?)
        }
        "config.apiServiceAppSpeed.save" => {
            let payload: AppSpeedPayload = parse_payload(request.payload)?;
            save_api_service_app_speed(payload.speed)
        }
        "accounts.updateAppSpeed" => {
            let payload: AccountAppSpeedPayload = parse_payload(request.payload)?;
            update_account_app_speed(payload)
        }
        "accounts.refreshProfile" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(codex_account::refresh_account_profile(&payload.account_id))?)
        }
        "accounts.export" => {
            let payload: AccountIdsPayload = parse_payload(request.payload)?;
            to_value(codex_account::export_accounts(&payload.account_ids)?)
        }
        "quota.refresh" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(codex_quota::refresh_account_quota(&payload.account_id))?)
        }
        "quota.refreshCurrent" => refresh_current_quota(runtime),
        "quota.refreshAll" => to_value(runtime.block_on(codex_quota::refresh_all_quotas())?),
        "quota.resetCredits" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(codex_quota::fetch_account_reset_credits(
                &payload.account_id,
            ))?)
        }
        "quota.consumeResetCredit" => {
            let payload: AccountIdPayload = parse_payload(request.payload)?;
            runtime.block_on(codex_quota::consume_reset_credit(&payload.account_id))?;
            Ok(Value::Null)
        }
        "quota.referralInviteEligibility" => {
            let payload: ReferralPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_quota::fetch_referral_invite_eligibility(
                    &payload.account_id,
                    payload.referral_key,
                ))?,
            )
        }
        "quota.referralEligibilityRules" => {
            let payload: ReferralPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_quota::fetch_referral_eligibility_rules(
                    &payload.account_id,
                    payload.referral_key,
                ))?,
            )
        }
        "quota.sendReferralInvites" => {
            let payload: ReferralInvitePayload = parse_payload(request.payload)?;
            to_value(runtime.block_on(codex_quota::send_referral_invites(
                &payload.account_id,
                payload.referral_key,
                payload.emails,
            ))?)
        }
        "quota.refreshSubscriptionInfo" => {
            let payload: SubscriptionPayload = parse_payload(request.payload)?;
            to_value(
                runtime.block_on(codex_quota::refresh_account_subscription_info(
                    &payload.account_id,
                    payload.force,
                ))?,
            )
        }
        "quota.alertPayload" => to_value(codex_account::run_quota_alert_if_needed()?),
        other => Err(format!("未知 Codex adapter 方法: {}", other)),
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
            "error": { "message": format!("序列化 Codex adapter HTTP 响应失败: {}", error) }
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
    runtime: Arc<Runtime>,
    shutdown: Arc<AtomicBool>,
    token: String,
    mut request: tiny_http::Request,
) {
    if request.method() != &Method::Post || request.url() != "/rpc" {
        write_json_response(
            request,
            404,
            error_response("Codex adapter 路由不存在".to_string()),
        );
        return;
    }
    if !is_authorized(&request, &token) {
        write_json_response(
            request,
            401,
            error_response("Codex adapter token 无效".to_string()),
        );
        return;
    }

    let mut body = String::new();
    if let Err(error) = request.as_reader().read_to_string(&mut body) {
        write_json_response(
            request,
            400,
            error_response(format!("读取 Codex adapter 请求失败: {}", error)),
        );
        return;
    }

    let rpc_request = match serde_json::from_str::<RpcRequest>(&body) {
        Ok(value) => value,
        Err(error) => {
            write_json_response(
                request,
                400,
                error_response(format!("解析 Codex adapter 请求 JSON 失败: {}", error)),
            );
            return;
        }
    };

    let should_shutdown = rpc_request.method == "adapter.shutdown";
    let response = match handle_rpc(&runtime, rpc_request) {
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

    let runtime = Arc::new(Runtime::new().expect("create tokio runtime"));
    let server = Server::http("127.0.0.1:0").expect("bind codex adapter server");
    let address = server.server_addr().to_string();
    let port = address
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .expect("parse codex adapter port");
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

    while !shutdown.load(Ordering::SeqCst) {
        match server.recv_timeout(Duration::from_millis(200)) {
            Ok(Some(request)) => {
                let runtime = Arc::clone(&runtime);
                let shutdown = Arc::clone(&shutdown);
                let token = token.clone();
                std::thread::spawn(move || {
                    handle_http_request(runtime, shutdown, token, request);
                });
            }
            Ok(None) => {}
            Err(error) => {
                eprintln!("Codex adapter 接收请求失败: {}", error);
                break;
            }
        }
    }
}
