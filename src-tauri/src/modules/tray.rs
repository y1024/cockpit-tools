//! 系统托盘模块
//! 管理系统托盘图标和菜单

use std::collections::{HashMap, HashSet};

use tauri::{
    menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, Runtime,
};
use tracing::info;

use crate::modules::logger;

/// 托盘菜单 ID
pub const TRAY_ID: &str = "main-tray";

/// 单层最多直出的平台数量（超出进入“更多平台”子菜单）
const TRAY_PLATFORM_MAX_VISIBLE: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PlatformId {
    Antigravity,
    Codex,
    GitHubCopilot,
    Windsurf,
    Kiro,
}

impl PlatformId {
    fn default_order() -> [Self; 5] {
        [
            Self::Antigravity,
            Self::Codex,
            Self::GitHubCopilot,
            Self::Windsurf,
            Self::Kiro,
        ]
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            crate::modules::tray_layout::PLATFORM_ANTIGRAVITY => Some(Self::Antigravity),
            crate::modules::tray_layout::PLATFORM_CODEX => Some(Self::Codex),
            crate::modules::tray_layout::PLATFORM_GITHUB_COPILOT => Some(Self::GitHubCopilot),
            crate::modules::tray_layout::PLATFORM_WINDSURF => Some(Self::Windsurf),
            crate::modules::tray_layout::PLATFORM_KIRO => Some(Self::Kiro),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Antigravity => crate::modules::tray_layout::PLATFORM_ANTIGRAVITY,
            Self::Codex => crate::modules::tray_layout::PLATFORM_CODEX,
            Self::GitHubCopilot => crate::modules::tray_layout::PLATFORM_GITHUB_COPILOT,
            Self::Windsurf => crate::modules::tray_layout::PLATFORM_WINDSURF,
            Self::Kiro => crate::modules::tray_layout::PLATFORM_KIRO,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Antigravity => "Antigravity",
            Self::Codex => "Codex",
            Self::GitHubCopilot => "GitHub Copilot",
            Self::Windsurf => "Windsurf",
            Self::Kiro => "Kiro",
        }
    }

    fn nav_target(self) -> &'static str {
        match self {
            Self::Antigravity => "overview",
            Self::Codex => "codex",
            Self::GitHubCopilot => "github-copilot",
            Self::Windsurf => "windsurf",
            Self::Kiro => "kiro",
        }
    }

    fn stable_rank(self) -> usize {
        match self {
            Self::Antigravity => 0,
            Self::Codex => 1,
            Self::GitHubCopilot => 2,
            Self::Windsurf => 3,
            Self::Kiro => 4,
        }
    }
}

/// 菜单项 ID
pub mod menu_ids {
    pub const SHOW_WINDOW: &str = "show_window";
    pub const REFRESH_QUOTA: &str = "refresh_quota";
    pub const SETTINGS: &str = "settings";
    pub const QUIT: &str = "quit";
}

/// 账号显示信息
struct AccountDisplayInfo {
    account: String,
    quota_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct CopilotUsage {
    inline_used_percent: Option<i32>,
    chat_used_percent: Option<i32>,
    reset_ts: Option<i64>,
}

/// 创建系统托盘
pub fn create_tray<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<TrayIcon<R>, tauri::Error> {
    info!("[Tray] 正在创建系统托盘...");

    let menu = build_tray_menu(app)?;

    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Cockpit Tools")
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(handle_tray_event)
        .build(app)?;

    info!("[Tray] 系统托盘创建成功");
    Ok(tray)
}

/// 构建托盘菜单
fn build_tray_menu<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<Menu<R>, tauri::Error> {
    let config = crate::modules::config::get_user_config();
    let lang = &config.language;

    let show_window = MenuItem::with_id(
        app,
        menu_ids::SHOW_WINDOW,
        get_text("show_window", lang),
        true,
        None::<&str>,
    )?;
    let refresh_quota = MenuItem::with_id(
        app,
        menu_ids::REFRESH_QUOTA,
        get_text("refresh_quota", lang),
        true,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(
        app,
        menu_ids::SETTINGS,
        get_text("settings", lang),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        app,
        menu_ids::QUIT,
        get_text("quit", lang),
        true,
        None::<&str>,
    )?;

    let ordered_platforms = resolve_tray_platforms();
    let split_index = ordered_platforms.len().min(TRAY_PLATFORM_MAX_VISIBLE);
    let (visible_platforms, overflow_platforms) = ordered_platforms.split_at(split_index);

    let mut platform_submenus: Vec<Submenu<R>> = Vec::new();
    for platform in visible_platforms {
        platform_submenus.push(build_platform_submenu(app, *platform, lang)?);
    }

    let mut overflow_submenus: Vec<Submenu<R>> = Vec::new();
    for platform in overflow_platforms {
        overflow_submenus.push(build_platform_submenu(app, *platform, lang)?);
    }

    let overflow_refs: Vec<&dyn IsMenuItem<R>> = overflow_submenus
        .iter()
        .map(|submenu| submenu as &dyn IsMenuItem<R>)
        .collect();
    let more_platforms_submenu = if overflow_refs.is_empty() {
        None
    } else {
        Some(Submenu::with_id_and_items(
            app,
            "tray_more_platforms",
            get_text("more_platforms", lang),
            true,
            &overflow_refs,
        )?)
    };

    let no_platform_item = if platform_submenus.is_empty() && overflow_submenus.is_empty() {
        Some(MenuItem::with_id(
            app,
            "tray_no_platform_selected",
            get_text("no_platform_selected", lang),
            true,
            None::<&str>,
        )?)
    } else {
        None
    };

    let menu = Menu::with_id(app, "tray_menu")?;
    menu.append(&show_window)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    if let Some(item) = &no_platform_item {
        menu.append(item)?;
    } else {
        for submenu in &platform_submenus {
            menu.append(submenu)?;
        }
        if let Some(submenu) = &more_platforms_submenu {
            menu.append(submenu)?;
        }
    }

    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&refresh_quota)?;
    menu.append(&settings)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&quit)?;
    Ok(menu)
}

fn resolve_tray_platforms() -> Vec<PlatformId> {
    let layout = crate::modules::tray_layout::load_tray_layout();
    let visible = sanitize_platform_list(&layout.tray_platform_ids);
    let visible_set: HashSet<PlatformId> = visible.iter().copied().collect();

    if visible_set.is_empty() {
        return Vec::new();
    }

    let ordered = if layout.sort_mode == crate::modules::tray_layout::SORT_MODE_MANUAL {
        normalize_platform_order(&layout.ordered_platform_ids)
    } else {
        auto_sort_platforms_by_account_count()
    };

    ordered
        .into_iter()
        .filter(|platform| visible_set.contains(platform))
        .collect()
}

fn sanitize_platform_list(ids: &[String]) -> Vec<PlatformId> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();

    for raw in ids {
        let Some(platform) = PlatformId::from_str(raw.trim()) else {
            continue;
        };
        if seen.insert(platform) {
            result.push(platform);
        }
    }

    result
}

fn normalize_platform_order(ids: &[String]) -> Vec<PlatformId> {
    let mut result = sanitize_platform_list(ids);
    let mut seen: HashSet<PlatformId> = result.iter().copied().collect();

    for platform in PlatformId::default_order() {
        if seen.insert(platform) {
            result.push(platform);
        }
    }

    result
}

fn auto_sort_platforms_by_account_count() -> Vec<PlatformId> {
    let counts = collect_platform_account_counts();
    let mut platforms = PlatformId::default_order().to_vec();

    platforms.sort_by(|a, b| {
        let a_count = counts.get(a).copied().unwrap_or(0);
        let b_count = counts.get(b).copied().unwrap_or(0);
        b_count
            .cmp(&a_count)
            .then_with(|| a.stable_rank().cmp(&b.stable_rank()))
    });

    platforms
}

fn collect_platform_account_counts() -> HashMap<PlatformId, usize> {
    let mut counts = HashMap::new();
    counts.insert(
        PlatformId::Antigravity,
        crate::modules::account::list_accounts()
            .map(|accounts| accounts.len())
            .unwrap_or(0),
    );
    counts.insert(
        PlatformId::Codex,
        crate::modules::codex_account::list_accounts().len(),
    );
    counts.insert(
        PlatformId::GitHubCopilot,
        crate::modules::github_copilot_account::list_accounts().len(),
    );
    counts.insert(
        PlatformId::Windsurf,
        crate::modules::windsurf_account::list_accounts().len(),
    );
    counts.insert(
        PlatformId::Kiro,
        crate::modules::kiro_account::list_accounts().len(),
    );
    counts
}

fn build_platform_submenu<R: Runtime>(
    app: &tauri::AppHandle<R>,
    platform: PlatformId,
    lang: &str,
) -> Result<Submenu<R>, tauri::Error> {
    let info = get_account_display_info(platform, lang);
    let mut items: Vec<MenuItem<R>> = Vec::new();

    items.push(MenuItem::with_id(
        app,
        format!("platform:{}:account", platform.as_str()),
        info.account,
        true,
        None::<&str>,
    )?);

    for (idx, line) in info.quota_lines.iter().enumerate() {
        items.push(MenuItem::with_id(
            app,
            format!("platform:{}:quota:{}", platform.as_str(), idx),
            line,
            true,
            None::<&str>,
        )?);
    }

    let refs: Vec<&dyn IsMenuItem<R>> = items
        .iter()
        .map(|item| item as &dyn IsMenuItem<R>)
        .collect();

    Submenu::with_id_and_items(
        app,
        format!("platform:{}:submenu", platform.as_str()),
        platform.title(),
        true,
        &refs,
    )
}

fn get_account_display_info(platform: PlatformId, lang: &str) -> AccountDisplayInfo {
    match platform {
        PlatformId::Antigravity => build_antigravity_display_info(lang),
        PlatformId::Codex => build_codex_display_info(lang),
        PlatformId::GitHubCopilot => build_github_copilot_display_info(lang),
        PlatformId::Windsurf => build_windsurf_display_info(lang),
        PlatformId::Kiro => build_kiro_display_info(lang),
    }
}

fn build_antigravity_display_info(lang: &str) -> AccountDisplayInfo {
    match crate::modules::account::get_current_account() {
        Ok(Some(account)) => {
            let quota_lines = if let Some(quota) = &account.quota {
                build_model_quota_lines(lang, &quota.models)
            } else {
                vec![get_text("loading", lang)]
            };
            AccountDisplayInfo {
                account: format!("📧 {}", account.email),
                quota_lines,
            }
        }
        _ => AccountDisplayInfo {
            account: format!("📧 {}", get_text("not_logged_in", lang)),
            quota_lines: vec!["—".to_string()],
        },
    }
}

fn format_codex_window_label(window_minutes: Option<i64>, fallback: &str) -> String {
    const HOUR_MINUTES: i64 = 60;
    const DAY_MINUTES: i64 = 24 * HOUR_MINUTES;
    const WEEK_MINUTES: i64 = 7 * DAY_MINUTES;

    let Some(minutes) = window_minutes.filter(|value| *value > 0) else {
        return fallback.to_string();
    };

    if minutes >= WEEK_MINUTES - 1 {
        let weeks = (minutes + WEEK_MINUTES - 1) / WEEK_MINUTES;
        return if weeks <= 1 {
            "Weekly".to_string()
        } else {
            format!("{} Week", weeks)
        };
    }

    if minutes >= DAY_MINUTES - 1 {
        let days = (minutes + DAY_MINUTES - 1) / DAY_MINUTES;
        return format!("{}d", days);
    }

    if minutes >= HOUR_MINUTES {
        let hours = (minutes + HOUR_MINUTES - 1) / HOUR_MINUTES;
        return format!("{}h", hours);
    }

    format!("{}m", minutes)
}

fn build_codex_display_info(lang: &str) -> AccountDisplayInfo {
    if let Some(account) = crate::modules::codex_account::get_current_account() {
        let mut quota_lines = if let Some(quota) = &account.quota {
            let has_presence =
                quota.hourly_window_present.is_some() || quota.weekly_window_present.is_some();
            let mut lines = Vec::new();

            if !has_presence || quota.hourly_window_present.unwrap_or(false) {
                lines.push(format!(
                    "{}: {}% · {} {}",
                    format_codex_window_label(quota.hourly_window_minutes, "5h"),
                    quota.hourly_percentage.clamp(0, 100),
                    get_text("reset", lang),
                    format_reset_time_from_ts(lang, quota.hourly_reset_time)
                ));
            }

            if !has_presence || quota.weekly_window_present.unwrap_or(false) {
                lines.push(format!(
                    "{}: {}% · {} {}",
                    format_codex_window_label(quota.weekly_window_minutes, "Weekly"),
                    quota.weekly_percentage.clamp(0, 100),
                    get_text("reset", lang),
                    format_reset_time_from_ts(lang, quota.weekly_reset_time)
                ));
            }

            if lines.is_empty() {
                lines.push(format!(
                    "{}: {}% · {} {}",
                    format_codex_window_label(quota.hourly_window_minutes, "5h"),
                    quota.hourly_percentage.clamp(0, 100),
                    get_text("reset", lang),
                    format_reset_time_from_ts(lang, quota.hourly_reset_time)
                ));
            }

            lines
        } else {
            vec![get_text("loading", lang)]
        };

        if quota_lines.is_empty() {
            quota_lines.push("—".to_string());
        }

        AccountDisplayInfo {
            account: format!("📧 {}", account.email),
            quota_lines,
        }
    } else {
        AccountDisplayInfo {
            account: format!("📧 {}", get_text("not_logged_in", lang)),
            quota_lines: vec!["—".to_string()],
        }
    }
}

fn build_github_copilot_display_info(lang: &str) -> AccountDisplayInfo {
    let accounts = crate::modules::github_copilot_account::list_accounts();
    let Some(account) = resolve_github_copilot_current_account(&accounts) else {
        return AccountDisplayInfo {
            account: format!("📧 {}", get_text("not_logged_in", lang)),
            quota_lines: vec!["—".to_string()],
        };
    };

    let usage = compute_copilot_usage(
        &account.copilot_token,
        account.copilot_plan.as_deref(),
        account.copilot_limited_user_quotas.as_ref(),
        account.copilot_quota_snapshots.as_ref(),
        account.copilot_limited_user_reset_date,
        account.copilot_quota_reset_date.as_deref(),
    );

    AccountDisplayInfo {
        account: format!(
            "📧 {}",
            display_login_email(account.github_email.as_deref(), &account.github_login)
        ),
        quota_lines: build_copilot_quota_lines(lang, usage, "Inline", "Chat"),
    }
}

fn build_windsurf_display_info(lang: &str) -> AccountDisplayInfo {
    let accounts = crate::modules::windsurf_account::list_accounts();
    let Some(account) = resolve_windsurf_current_account(&accounts) else {
        return AccountDisplayInfo {
            account: format!("📧 {}", get_text("not_logged_in", lang)),
            quota_lines: vec!["—".to_string()],
        };
    };

    let mut usage = compute_copilot_usage(
        &account.copilot_token,
        account.copilot_plan.as_deref(),
        account.copilot_limited_user_quotas.as_ref(),
        account.copilot_quota_snapshots.as_ref(),
        account.copilot_limited_user_reset_date,
        account.copilot_quota_reset_date.as_deref(),
    );
    if usage.reset_ts.is_none() {
        usage.reset_ts = resolve_windsurf_plan_end_ts(&account);
    }

    AccountDisplayInfo {
        account: format!(
            "📧 {}",
            display_login_email(account.github_email.as_deref(), &account.github_login)
        ),
        quota_lines: build_windsurf_quota_lines(lang, usage),
    }
}

fn build_kiro_display_info(lang: &str) -> AccountDisplayInfo {
    let accounts = crate::modules::kiro_account::list_accounts();
    let Some(account) = resolve_kiro_current_account(&accounts) else {
        return AccountDisplayInfo {
            account: format!("📧 {}", get_text("not_logged_in", lang)),
            quota_lines: vec!["—".to_string()],
        };
    };

    let mut quota_lines = Vec::new();
    let reset_text = format_reset_time_from_ts(lang, account.usage_reset_at);

    if let Some(plan) =
        first_non_empty(&[account.plan_name.as_deref(), account.plan_tier.as_deref()])
    {
        quota_lines.push(format!("Plan: {}", plan));
    }

    if let Some(remaining_pct) = calc_remaining_percent(account.credits_total, account.credits_used)
    {
        quota_lines.push(format!(
            "Prompt: {}% · {} {}",
            remaining_pct,
            get_text("reset", lang),
            reset_text
        ));
    }

    if let Some(remaining_pct) = calc_remaining_percent(account.bonus_total, account.bonus_used) {
        quota_lines.push(format!(
            "Add-on: {}% · {} {}",
            remaining_pct,
            get_text("reset", lang),
            reset_text
        ));
    }

    if quota_lines.is_empty() {
        quota_lines.push(get_text("loading", lang));
    }

    AccountDisplayInfo {
        account: format!(
            "📧 {}",
            first_non_empty(&[Some(account.email.as_str()), Some(account.id.as_str())])
                .unwrap_or("—")
        ),
        quota_lines,
    }
}

fn resolve_github_copilot_current_account(
    accounts: &[crate::models::github_copilot::GitHubCopilotAccount],
) -> Option<crate::models::github_copilot::GitHubCopilotAccount> {
    if let Ok(settings) = crate::modules::github_copilot_instance::load_default_settings() {
        if let Some(bind_id) = settings.bind_account_id {
            let bind_id = bind_id.trim();
            if !bind_id.is_empty() {
                if let Some(account) = accounts.iter().find(|account| account.id == bind_id) {
                    return Some(account.clone());
                }
            }
        }
    }

    accounts
        .iter()
        .max_by_key(|account| account.last_used)
        .cloned()
}

fn resolve_windsurf_current_account(
    accounts: &[crate::models::windsurf::WindsurfAccount],
) -> Option<crate::models::windsurf::WindsurfAccount> {
    if let Ok(settings) = crate::modules::windsurf_instance::load_default_settings() {
        if let Some(bind_id) = settings.bind_account_id {
            let bind_id = bind_id.trim();
            if !bind_id.is_empty() {
                if let Some(account) = accounts.iter().find(|account| account.id == bind_id) {
                    return Some(account.clone());
                }
            }
        }
    }

    accounts
        .iter()
        .max_by_key(|account| account.last_used)
        .cloned()
}

fn resolve_kiro_current_account(
    accounts: &[crate::models::kiro::KiroAccount],
) -> Option<crate::models::kiro::KiroAccount> {
    if let Ok(settings) = crate::modules::kiro_instance::load_default_settings() {
        if let Some(bind_id) = settings.bind_account_id {
            let bind_id = bind_id.trim();
            if !bind_id.is_empty() {
                if let Some(account) = accounts.iter().find(|account| account.id == bind_id) {
                    return Some(account.clone());
                }
            }
        }
    }

    accounts
        .iter()
        .max_by_key(|account| account.last_used)
        .cloned()
}

fn first_non_empty<'a>(values: &[Option<&'a str>]) -> Option<&'a str> {
    values
        .iter()
        .flatten()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
}

fn calc_remaining_percent(total: Option<f64>, used: Option<f64>) -> Option<i32> {
    let total = total?;
    if !total.is_finite() || total <= 0.0 {
        return None;
    }

    let used = used.unwrap_or(0.0);
    if !used.is_finite() {
        return None;
    }

    let remaining = (total - used).max(0.0);
    Some(clamp_percent((remaining / total) * 100.0))
}

fn display_login_email(email: Option<&str>, login: &str) -> String {
    email
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(login)
        .to_string()
}

fn build_copilot_quota_lines(
    lang: &str,
    usage: CopilotUsage,
    inline_label: &str,
    chat_label: &str,
) -> Vec<String> {
    let mut lines = Vec::new();
    let reset_text = format_reset_time_from_ts(lang, usage.reset_ts);

    if let Some(percentage) = usage.inline_used_percent {
        lines.push(format!(
            "{}: {}% · {} {}",
            inline_label,
            percentage,
            get_text("reset", lang),
            reset_text
        ));
    }
    if let Some(percentage) = usage.chat_used_percent {
        lines.push(format!(
            "{}: {}% · {} {}",
            chat_label,
            percentage,
            get_text("reset", lang),
            reset_text
        ));
    }

    if lines.is_empty() {
        lines.push(get_text("loading", lang));
    }

    lines
}

fn build_windsurf_quota_lines(lang: &str, usage: CopilotUsage) -> Vec<String> {
    let mut lines = Vec::new();
    let reset_text = format_reset_time_from_ts(lang, usage.reset_ts);

    if let Some(percentage) = usage.inline_used_percent {
        lines.push(format!(
            "Prompt: {}% · {} {}",
            percentage,
            get_text("reset", lang),
            reset_text
        ));
    }
    if let Some(percentage) = usage.chat_used_percent {
        lines.push(format!(
            "Flow: {}% · {} {}",
            percentage,
            get_text("reset", lang),
            reset_text
        ));
    }

    if lines.is_empty() {
        lines.push(get_text("loading", lang));
    }

    lines
}

fn compute_copilot_usage(
    token: &str,
    plan: Option<&str>,
    limited_quotas: Option<&serde_json::Value>,
    quota_snapshots: Option<&serde_json::Value>,
    limited_reset_ts: Option<i64>,
    quota_reset_date: Option<&str>,
) -> CopilotUsage {
    let token_map = parse_token_map(token);
    let reset_ts = limited_reset_ts
        .or_else(|| parse_reset_date_to_ts(quota_reset_date))
        .or_else(|| {
            parse_token_number(&token_map, "rd")
                .map(|value| value.floor() as i64)
                .filter(|value| *value > 0)
        });
    let sku = token_map
        .get("sku")
        .map(|value| value.to_lowercase())
        .unwrap_or_default();
    let is_free_limited = sku.contains("free_limited")
        || sku.contains("no_auth_limited")
        || plan
            .map(|value| value.to_lowercase().contains("free_limited"))
            .unwrap_or(false);

    if !is_free_limited {
        if let Some(premium_used) = premium_used_percent(quota_snapshots) {
            return CopilotUsage {
                inline_used_percent: Some(premium_used),
                chat_used_percent: Some(premium_used),
                reset_ts,
            };
        }
    }

    let limited = limited_quotas.and_then(|value| value.as_object());
    let remaining_inline = limited
        .and_then(|obj| obj.get("completions"))
        .and_then(parse_json_number);
    let remaining_chat = limited
        .and_then(|obj| obj.get("chat"))
        .and_then(parse_json_number);

    let total_inline = parse_token_number(&token_map, "cq").or(remaining_inline);
    let total_chat = parse_token_number(&token_map, "tq").or_else(|| {
        if is_free_limited {
            remaining_chat.map(|_| 500.0)
        } else {
            remaining_chat
        }
    });

    CopilotUsage {
        inline_used_percent: calc_used_percent(total_inline, remaining_inline),
        chat_used_percent: calc_used_percent(total_chat, remaining_chat),
        reset_ts,
    }
}

fn resolve_windsurf_plan_end_ts(account: &crate::models::windsurf::WindsurfAccount) -> Option<i64> {
    let mut candidates: Vec<Option<&serde_json::Value>> = Vec::new();
    let user_status = account.windsurf_user_status.as_ref();
    let snapshots = account.copilot_quota_snapshots.as_ref();

    candidates.push(json_path(
        user_status,
        &["userStatus", "planStatus", "planEnd"],
    ));
    candidates.push(json_path(
        user_status,
        &["userStatus", "planStatus", "plan_end"],
    ));
    candidates.push(json_path(user_status, &["planStatus", "planEnd"]));
    candidates.push(json_path(user_status, &["planStatus", "plan_end"]));
    candidates.push(json_path(snapshots, &["windsurfPlanStatus", "planEnd"]));
    candidates.push(json_path(snapshots, &["windsurfPlanStatus", "plan_end"]));
    candidates.push(json_path(
        snapshots,
        &["windsurfPlanStatus", "planStatus", "planEnd"],
    ));
    candidates.push(json_path(
        snapshots,
        &["windsurfPlanStatus", "planStatus", "plan_end"],
    ));
    candidates.push(json_path(
        snapshots,
        &["windsurfUserStatus", "userStatus", "planStatus", "planEnd"],
    ));
    candidates.push(json_path(
        snapshots,
        &["windsurfUserStatus", "userStatus", "planStatus", "plan_end"],
    ));

    for candidate in candidates.into_iter().flatten() {
        if let Some(ts) = parse_timestamp_like(candidate) {
            return Some(ts);
        }
    }

    None
}

fn json_path<'a>(
    root: Option<&'a serde_json::Value>,
    path: &[&str],
) -> Option<&'a serde_json::Value> {
    let mut current = root?;
    for key in path {
        current = current.as_object()?.get(*key)?;
    }
    Some(current)
}

fn parse_timestamp_like(value: &serde_json::Value) -> Option<i64> {
    match value {
        serde_json::Value::Number(num) => parse_timestamp_number(num.as_f64()?),
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return None;
            }
            if let Ok(n) = trimmed.parse::<f64>() {
                return parse_timestamp_number(n);
            }
            chrono::DateTime::parse_from_rfc3339(trimmed)
                .ok()
                .map(|dt| dt.timestamp())
        }
        serde_json::Value::Object(obj) => {
            if let Some(seconds) = obj.get("seconds").and_then(|v| v.as_i64()) {
                return Some(seconds);
            }
            if let Some(seconds) = obj.get("unixSeconds").and_then(|v| v.as_i64()) {
                return Some(seconds);
            }
            if let Some(inner) = obj.get("value") {
                return parse_timestamp_like(inner);
            }
            None
        }
        _ => None,
    }
}

fn parse_timestamp_number(raw: f64) -> Option<i64> {
    if !raw.is_finite() || raw <= 0.0 {
        return None;
    }
    if raw > 1e12 {
        return Some((raw / 1000.0).floor() as i64);
    }
    Some(raw.floor() as i64)
}

fn parse_token_map(token: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let prefix = token.split(':').next().unwrap_or(token);
    for item in prefix.split(';') {
        let mut parts = item.splitn(2, '=');
        let key = parts.next().unwrap_or("").trim();
        if key.is_empty() {
            continue;
        }
        let value = parts.next().unwrap_or("").trim();
        map.insert(key.to_string(), value.to_string());
    }
    map
}

fn parse_token_number(map: &HashMap<String, String>, key: &str) -> Option<f64> {
    map.get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .and_then(|value| value.split(':').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite())
}

fn parse_json_number(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Number(num) => num.as_f64(),
        serde_json::Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|value| value.is_finite())
}

fn calc_used_percent(total: Option<f64>, remaining: Option<f64>) -> Option<i32> {
    let total = total?;
    let remaining = remaining?;
    if total <= 0.0 {
        return None;
    }

    let used = (total - remaining).max(0.0);
    Some(clamp_percent((used / total) * 100.0))
}

fn premium_used_percent(quota_snapshots: Option<&serde_json::Value>) -> Option<i32> {
    let snapshots = quota_snapshots?.as_object()?;
    let premium = snapshots
        .get("premium_interactions")
        .or_else(|| snapshots.get("premium_models"))
        .and_then(|value| value.as_object())?;

    if premium.get("unlimited").and_then(|value| value.as_bool()) == Some(true) {
        return Some(0);
    }

    let percent_remaining = premium
        .get("percent_remaining")
        .and_then(parse_json_number)
        .map(clamp_percent)?;
    Some(clamp_percent((100 - percent_remaining) as f64))
}

fn parse_reset_date_to_ts(reset_date: Option<&str>) -> Option<i64> {
    let reset_date = reset_date?.trim();
    if reset_date.is_empty() {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(reset_date)
        .ok()
        .map(|value| value.timestamp())
}

fn clamp_percent(value: f64) -> i32 {
    value.round().clamp(0.0, 100.0) as i32
}

fn build_model_quota_lines(lang: &str, models: &[crate::models::quota::ModelQuota]) -> Vec<String> {
    let mut lines = Vec::new();
    for model in models.iter().take(4) {
        let reset_text = format_reset_time(lang, &model.reset_time);
        if reset_text.is_empty() {
            lines.push(format!("{}: {}%", model.name, model.percentage));
        } else {
            lines.push(format!(
                "{}: {}% · {} {}",
                model.name,
                model.percentage,
                get_text("reset", lang),
                reset_text
            ));
        }
    }
    if lines.is_empty() {
        lines.push("—".to_string());
    }
    lines
}

fn format_reset_time_from_ts(lang: &str, reset_ts: Option<i64>) -> String {
    let Some(reset_ts) = reset_ts else {
        return "—".to_string();
    };
    let now = chrono::Utc::now().timestamp();
    let remaining_secs = reset_ts - now;
    if remaining_secs <= 0 {
        return get_text("reset_done", lang);
    }
    format_remaining_duration(remaining_secs)
}

fn format_remaining_duration(remaining_secs: i64) -> String {
    let mut secs = remaining_secs.max(0);
    let days = secs / 86_400;
    secs %= 86_400;
    let hours = secs / 3_600;
    secs %= 3_600;
    let minutes = (secs / 60).max(1);

    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

/// 格式化重置时间
fn format_reset_time(lang: &str, reset_time: &str) -> String {
    if let Ok(reset) = chrono::DateTime::parse_from_rfc3339(reset_time) {
        let now = chrono::Utc::now();
        let duration = reset.signed_duration_since(now);

        if duration.num_seconds() <= 0 {
            return get_text("reset_done", lang);
        }

        let hours = duration.num_hours();
        let minutes = duration.num_minutes() % 60;

        if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}m", minutes)
        }
    } else {
        reset_time.to_string()
    }
}

/// 处理菜单事件
fn handle_menu_event<R: Runtime>(app: &tauri::AppHandle<R>, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref();
    logger::log_info(&format!("[Tray] 菜单点击: {}", id));

    match id {
        menu_ids::SHOW_WINDOW => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
        menu_ids::REFRESH_QUOTA => {
            let _ = app.emit("tray:refresh_quota", ());
        }
        menu_ids::SETTINGS => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
                let _ = app.emit("tray:navigate", "settings");
            }
        }
        menu_ids::QUIT => {
            info!("[Tray] 用户选择退出应用");
            app.exit(0);
        }
        _ => {
            if let Some(platform) = parse_platform_from_menu_id(id) {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                    let _ = app.emit("tray:navigate", platform.nav_target());
                }
            } else if id.starts_with("ag_") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                    let _ = app.emit("tray:navigate", "overview");
                }
            } else if id.starts_with("codex_") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                    let _ = app.emit("tray:navigate", "codex");
                }
            }
        }
    }
}

fn parse_platform_from_menu_id(id: &str) -> Option<PlatformId> {
    let mut parts = id.split(':');
    if parts.next()? != "platform" {
        return None;
    }
    PlatformId::from_str(parts.next()?)
}

/// 处理托盘图标事件
fn handle_tray_event<R: Runtime>(tray: &TrayIcon<R>, event: TrayIconEvent) {
    match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } => {
            if let Some(window) = tray.app_handle().get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
        TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => {
            if let Some(window) = tray.app_handle().get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
        _ => {}
    }
}

/// 更新托盘菜单
pub fn update_tray_menu<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let menu = build_tray_menu(app).map_err(|e| e.to_string())?;
        tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
        logger::log_info("[Tray] 托盘菜单已更新");
    }
    Ok(())
}

/// 获取本地化文本
fn get_text(key: &str, lang: &str) -> String {
    match (key, lang) {
        // 简体中文
        ("show_window", "zh-cn") => "显示主窗口".to_string(),
        ("refresh_quota", "zh-cn") => "🔄 刷新配额".to_string(),
        ("settings", "zh-cn") => "⚙️ 设置...".to_string(),
        ("quit", "zh-cn") => "❌ 退出".to_string(),
        ("not_logged_in", "zh-cn") => "未登录".to_string(),
        ("loading", "zh-cn") => "加载中...".to_string(),
        ("reset", "zh-cn") => "重置".to_string(),
        ("reset_done", "zh-cn") => "已重置".to_string(),
        ("more_platforms", "zh-cn") => "更多平台".to_string(),
        ("no_platform_selected", "zh-cn") => "未选择托盘平台".to_string(),

        // 繁体中文
        ("show_window", "zh-tw") => "顯示主視窗".to_string(),
        ("refresh_quota", "zh-tw") => "🔄 重新整理配額".to_string(),
        ("settings", "zh-tw") => "⚙️ 設定...".to_string(),
        ("quit", "zh-tw") => "❌ 結束".to_string(),
        ("not_logged_in", "zh-tw") => "未登入".to_string(),
        ("loading", "zh-tw") => "載入中...".to_string(),
        ("reset", "zh-tw") => "重置".to_string(),
        ("reset_done", "zh-tw") => "已重置".to_string(),
        ("more_platforms", "zh-tw") => "更多平台".to_string(),
        ("no_platform_selected", "zh-tw") => "未選擇托盤平台".to_string(),

        // 英文
        ("show_window", "en") => "Show Window".to_string(),
        ("refresh_quota", "en") => "🔄 Refresh Quota".to_string(),
        ("settings", "en") => "⚙️ Settings...".to_string(),
        ("quit", "en") => "❌ Quit".to_string(),
        ("not_logged_in", "en") => "Not logged in".to_string(),
        ("loading", "en") => "Loading...".to_string(),
        ("reset", "en") => "Reset".to_string(),
        ("reset_done", "en") => "Reset done".to_string(),
        ("more_platforms", "en") => "More platforms".to_string(),
        ("no_platform_selected", "en") => "No tray platforms selected".to_string(),

        // 日语
        ("show_window", "ja") => "ウィンドウを表示".to_string(),
        ("refresh_quota", "ja") => "🔄 クォータを更新".to_string(),
        ("settings", "ja") => "⚙️ 設定...".to_string(),
        ("quit", "ja") => "❌ 終了".to_string(),
        ("not_logged_in", "ja") => "未ログイン".to_string(),
        ("loading", "ja") => "読み込み中...".to_string(),
        ("reset", "ja") => "リセット".to_string(),
        ("reset_done", "ja") => "リセット済み".to_string(),
        ("more_platforms", "ja") => "その他のプラットフォーム".to_string(),
        ("no_platform_selected", "ja") => {
            "トレイに表示するプラットフォームがありません".to_string()
        }

        // 俄语
        ("show_window", "ru") => "Показать окно".to_string(),
        ("refresh_quota", "ru") => "🔄 Обновить квоту".to_string(),
        ("settings", "ru") => "⚙️ Настройки...".to_string(),
        ("quit", "ru") => "❌ Выход".to_string(),
        ("not_logged_in", "ru") => "Не авторизован".to_string(),
        ("loading", "ru") => "Загрузка...".to_string(),
        ("reset", "ru") => "Сброс".to_string(),
        ("reset_done", "ru") => "Сброс выполнен".to_string(),
        ("more_platforms", "ru") => "Другие платформы".to_string(),
        ("no_platform_selected", "ru") => "Платформы для трея не выбраны".to_string(),

        // 默认英文
        ("show_window", _) => "Show Window".to_string(),
        ("refresh_quota", _) => "🔄 Refresh Quota".to_string(),
        ("settings", _) => "⚙️ Settings...".to_string(),
        ("quit", _) => "❌ Quit".to_string(),
        ("not_logged_in", _) => "Not logged in".to_string(),
        ("loading", _) => "Loading...".to_string(),
        ("reset", _) => "Reset".to_string(),
        ("reset_done", _) => "Reset done".to_string(),
        ("more_platforms", _) => "More platforms".to_string(),
        ("no_platform_selected", _) => "No tray platforms selected".to_string(),

        _ => key.to_string(),
    }
}
