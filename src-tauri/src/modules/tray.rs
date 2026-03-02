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
struct CopilotMetric {
    used_percent: Option<i32>,
    included: bool,
}

#[derive(Debug, Clone, Copy)]
struct CopilotUsage {
    inline: CopilotMetric,
    chat: CopilotMetric,
    premium: CopilotMetric,
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
                let grouped_lines = build_antigravity_group_quota_lines(lang, &quota.models);
                if grouped_lines.is_empty() {
                    build_model_quota_lines(lang, &quota.models)
                } else {
                    grouped_lines
                }
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

fn normalize_antigravity_model_for_match(value: &str) -> String {
    let normalized = value.trim().to_lowercase();
    if normalized.is_empty() {
        return normalized;
    }
    if normalized.starts_with("gemini-3.1-flash")
        || normalized.starts_with("gemini-2.5-flash")
        || normalized.starts_with("gemini-3-flash")
    {
        return "gemini-3-flash".to_string();
    }
    if normalized.starts_with("gemini-3.1-pro-high") || normalized.starts_with("gemini-3-pro-high")
    {
        return "gemini-3.1-pro-high".to_string();
    }
    if normalized.starts_with("gemini-3.1-pro-low") || normalized.starts_with("gemini-3-pro-low") {
        return "gemini-3.1-pro-low".to_string();
    }
    if normalized.starts_with("claude-sonnet-4-6") || normalized.starts_with("claude-sonnet-4-5") {
        return "claude-sonnet-4-6".to_string();
    }
    if normalized.starts_with("claude-opus-4-6-thinking")
        || normalized.starts_with("claude-opus-4-5-thinking")
    {
        return "claude-opus-4-6-thinking".to_string();
    }
    match normalized.as_str() {
        "gemini-3-pro-high" => "gemini-3.1-pro-high".to_string(),
        "gemini-3-pro-low" => "gemini-3.1-pro-low".to_string(),
        "claude-sonnet-4-5" => "claude-sonnet-4-6".to_string(),
        "claude-sonnet-4-5-thinking" => "claude-sonnet-4-6".to_string(),
        "claude-opus-4-5-thinking" => "claude-opus-4-6-thinking".to_string(),
        _ => normalized,
    }
}

fn antigravity_model_matches(model_name: &str, target: &str) -> bool {
    let left = normalize_antigravity_model_for_match(model_name);
    let right = normalize_antigravity_model_for_match(target);
    if left.is_empty() || right.is_empty() {
        return false;
    }
    left == right || left.starts_with(&(right.clone() + "-")) || right.starts_with(&(left + "-"))
}

fn parse_model_reset_ts(reset_time: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(reset_time)
        .ok()
        .map(|value| value.timestamp())
}

fn build_antigravity_group_quota_lines(
    lang: &str,
    models: &[crate::models::quota::ModelQuota],
) -> Vec<String> {
    let settings = crate::modules::group_settings::load_group_settings();
    let ordered_groups = settings.get_ordered_groups(Some(3));
    if ordered_groups.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    for group_id in ordered_groups {
        let group_models = settings.get_models_in_group(&group_id);
        if group_models.is_empty() {
            continue;
        }

        let mut total_percentage: i64 = 0;
        let mut count: i64 = 0;
        let mut earliest_reset_ts: Option<i64> = None;

        for model in models {
            let belongs = group_models
                .iter()
                .any(|group_model| antigravity_model_matches(&model.name, group_model));
            if !belongs {
                continue;
            }

            total_percentage += i64::from(model.percentage.clamp(0, 100));
            count += 1;
            if let Some(reset_ts) = parse_model_reset_ts(&model.reset_time) {
                earliest_reset_ts = Some(match earliest_reset_ts {
                    Some(current) => current.min(reset_ts),
                    None => reset_ts,
                });
            }
        }

        if count <= 0 {
            continue;
        }

        let avg_percentage = (total_percentage as f64 / count as f64).round() as i32;
        let reset_text = earliest_reset_ts.map(|ts| format_reset_time_from_ts(lang, Some(ts)));
        lines.push(format_quota_line(
            lang,
            &settings.get_group_name(&group_id),
            &format_percent_text(avg_percentage),
            reset_text.as_deref(),
        ));
    }

    lines
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
                lines.push(format_quota_line(
                    lang,
                    &format_codex_window_label(quota.hourly_window_minutes, "5h"),
                    &format_percent_text(quota.hourly_percentage),
                    Some(&format_reset_time_from_ts(lang, quota.hourly_reset_time)),
                ));
            }

            if !has_presence || quota.weekly_window_present.unwrap_or(false) {
                lines.push(format_quota_line(
                    lang,
                    &format_codex_window_label(quota.weekly_window_minutes, "Weekly"),
                    &format_percent_text(quota.weekly_percentage),
                    Some(&format_reset_time_from_ts(lang, quota.weekly_reset_time)),
                ));
            }

            if lines.is_empty() {
                lines.push(format_quota_line(
                    lang,
                    &format_codex_window_label(quota.hourly_window_minutes, "5h"),
                    &format_percent_text(quota.hourly_percentage),
                    Some(&format_reset_time_from_ts(lang, quota.hourly_reset_time)),
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
        quota_lines: build_copilot_quota_lines(lang, usage),
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
        quota_lines.push(format_quota_line(
            lang,
            "Prompt",
            &format_percent_text(remaining_pct),
            Some(&reset_text),
        ));
    }

    if let Some(remaining_pct) = calc_remaining_percent(account.bonus_total, account.bonus_used) {
        quota_lines.push(format_quota_line(
            lang,
            "Add-on",
            &format_percent_text(remaining_pct),
            Some(&reset_text),
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

fn format_percent_text(percentage: i32) -> String {
    format!("{}%", percentage.clamp(0, 100))
}

fn format_quota_line(
    lang: &str,
    label: &str,
    value_text: &str,
    reset_text: Option<&str>,
) -> String {
    let normalized_reset = reset_text
        .map(|text| text.trim())
        .filter(|text| !text.is_empty() && *text != "—");

    if let Some(reset) = normalized_reset {
        format!(
            "{}: {} · {} {}",
            label,
            value_text,
            get_text("reset", lang),
            reset
        )
    } else {
        format!("{}: {}", label, value_text)
    }
}

fn format_copilot_metric_value(lang: &str, metric: CopilotMetric) -> Option<String> {
    if metric.included {
        return Some(get_text("included", lang));
    }
    metric
        .used_percent
        .map(|percentage| format!("{}%", percentage))
}

fn build_copilot_quota_lines(lang: &str, usage: CopilotUsage) -> Vec<String> {
    let mut lines = Vec::new();
    let reset_text = format_reset_time_from_ts(lang, usage.reset_ts);

    if let Some(value_text) = format_copilot_metric_value(lang, usage.inline) {
        lines.push(format_quota_line(
            lang,
            &get_text("ghcp_inline", lang),
            &value_text,
            Some(&reset_text),
        ));
    }
    if let Some(value_text) = format_copilot_metric_value(lang, usage.chat) {
        lines.push(format_quota_line(
            lang,
            &get_text("ghcp_chat", lang),
            &value_text,
            Some(&reset_text),
        ));
    }
    let premium_value =
        format_copilot_metric_value(lang, usage.premium).unwrap_or_else(|| "-".to_string());
    lines.push(format_quota_line(
        lang,
        &get_text("ghcp_premium", lang),
        &premium_value,
        None,
    ));

    if lines.is_empty() {
        lines.push(get_text("loading", lang));
    }

    lines
}

fn build_windsurf_quota_lines(lang: &str, usage: CopilotUsage) -> Vec<String> {
    let mut lines = Vec::new();
    let reset_text = format_reset_time_from_ts(lang, usage.reset_ts);

    if let Some(percentage) = usage.inline.used_percent {
        lines.push(format_quota_line(
            lang,
            "Prompt",
            &format_percent_text(percentage),
            Some(&reset_text),
        ));
    }
    if let Some(percentage) = usage.chat.used_percent {
        lines.push(format_quota_line(
            lang,
            "Flow",
            &format_percent_text(percentage),
            Some(&reset_text),
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

    let completions_snapshot = get_quota_snapshot(quota_snapshots, "completions");
    let chat_snapshot = get_quota_snapshot(quota_snapshots, "chat");
    let premium_snapshot = get_quota_snapshot(quota_snapshots, "premium_interactions");

    let limited = limited_quotas.and_then(|value| value.as_object());
    let remaining_inline = remaining_from_snapshot(completions_snapshot).or_else(|| {
        limited
            .and_then(|obj| obj.get("completions"))
            .and_then(parse_json_number)
    });
    let remaining_chat = remaining_from_snapshot(chat_snapshot).or_else(|| {
        limited
            .and_then(|obj| obj.get("chat"))
            .and_then(parse_json_number)
    });

    let total_inline = entitlement_from_snapshot(completions_snapshot)
        .or_else(|| parse_token_number(&token_map, "cq"))
        .or(remaining_inline);
    let total_chat = entitlement_from_snapshot(chat_snapshot)
        .or_else(|| parse_token_number(&token_map, "tq"))
        .or_else(|| {
            if is_free_limited {
                remaining_chat.map(|_| 500.0)
            } else {
                remaining_chat
            }
        });

    CopilotUsage {
        inline: CopilotMetric {
            used_percent: used_percent_from_snapshot(completions_snapshot)
                .or_else(|| calc_used_percent(total_inline, remaining_inline)),
            included: is_included_snapshot(completions_snapshot),
        },
        chat: CopilotMetric {
            used_percent: used_percent_from_snapshot(chat_snapshot)
                .or_else(|| calc_used_percent(total_chat, remaining_chat)),
            included: is_included_snapshot(chat_snapshot),
        },
        premium: CopilotMetric {
            used_percent: used_percent_from_snapshot(premium_snapshot),
            included: is_included_snapshot(premium_snapshot),
        },
        reset_ts,
    }
}

fn get_quota_snapshot<'a>(
    quota_snapshots: Option<&'a serde_json::Value>,
    key: &str,
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    let snapshots = quota_snapshots.and_then(|value| value.as_object())?;
    let primary = snapshots.get(key).and_then(|snapshot| snapshot.as_object());
    if primary.is_some() {
        return primary;
    }
    if key == "premium_interactions" {
        return snapshots
            .get("premium_models")
            .and_then(|snapshot| snapshot.as_object());
    }
    None
}

fn entitlement_from_snapshot(
    snapshot: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Option<f64> {
    snapshot
        .and_then(|data| data.get("entitlement"))
        .and_then(parse_json_number)
        .filter(|value| *value > 0.0)
}

fn remaining_from_snapshot(
    snapshot: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Option<f64> {
    if let Some(remaining) = snapshot
        .and_then(|data| data.get("remaining"))
        .and_then(parse_json_number)
    {
        return Some(remaining);
    }

    let entitlement = snapshot
        .and_then(|data| data.get("entitlement"))
        .and_then(parse_json_number)?;
    let percent_remaining = snapshot
        .and_then(|data| data.get("percent_remaining"))
        .and_then(parse_json_number)?;
    if entitlement <= 0.0 {
        return None;
    }
    Some((entitlement * (percent_remaining / 100.0)).max(0.0))
}

fn is_included_snapshot(snapshot: Option<&serde_json::Map<String, serde_json::Value>>) -> bool {
    if snapshot
        .and_then(|data| data.get("unlimited"))
        .and_then(|value| value.as_bool())
        == Some(true)
    {
        return true;
    }

    snapshot
        .and_then(|data| data.get("entitlement"))
        .and_then(parse_json_number)
        .map(|value| value < 0.0)
        .unwrap_or(false)
}

fn used_percent_from_snapshot(
    snapshot: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Option<i32> {
    if snapshot
        .and_then(|data| data.get("unlimited"))
        .and_then(|value| value.as_bool())
        == Some(true)
    {
        return Some(0);
    }

    let entitlement = snapshot
        .and_then(|data| data.get("entitlement"))
        .and_then(parse_json_number);
    let remaining = snapshot
        .and_then(|data| data.get("remaining"))
        .and_then(parse_json_number);

    if let (Some(total), Some(left)) = (entitlement, remaining) {
        return calc_used_percent(Some(total), Some(left));
    }

    let percent_remaining = snapshot
        .and_then(|data| data.get("percent_remaining"))
        .and_then(parse_json_number)
        .map(clamp_percent)?;
    Some(clamp_percent((100 - percent_remaining) as f64))
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
        lines.push(format_quota_line(
            lang,
            &model.name,
            &format_percent_text(model.percentage),
            Some(&reset_text),
        ));
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
        ("included", "zh-cn") => "包含".to_string(),
        ("ghcp_inline", "zh-cn") => "Inline".to_string(),
        ("ghcp_chat", "zh-cn") => "Chat".to_string(),
        ("ghcp_premium", "zh-cn") => "Premium".to_string(),
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
        ("included", "zh-tw") => "已包含".to_string(),
        ("ghcp_inline", "zh-tw") => "Inline".to_string(),
        ("ghcp_chat", "zh-tw") => "Chat".to_string(),
        ("ghcp_premium", "zh-tw") => "Premium".to_string(),
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
        ("included", "en") => "Included".to_string(),
        ("ghcp_inline", "en") => "Inline".to_string(),
        ("ghcp_chat", "en") => "Chat".to_string(),
        ("ghcp_premium", "en") => "Premium".to_string(),
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
        ("included", "ja") => "含まれる".to_string(),
        ("ghcp_inline", "ja") => "Inline".to_string(),
        ("ghcp_chat", "ja") => "Chat".to_string(),
        ("ghcp_premium", "ja") => "Premium".to_string(),
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
        ("included", "ru") => "Включено".to_string(),
        ("ghcp_inline", "ru") => "Inline".to_string(),
        ("ghcp_chat", "ru") => "Chat".to_string(),
        ("ghcp_premium", "ru") => "Premium".to_string(),
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
        ("included", _) => "Included".to_string(),
        ("ghcp_inline", _) => "Inline".to_string(),
        ("ghcp_chat", _) => "Chat".to_string(),
        ("ghcp_premium", _) => "Premium".to_string(),
        ("more_platforms", _) => "More platforms".to_string(),
        ("no_platform_selected", _) => "No tray platforms selected".to_string(),

        _ => key.to_string(),
    }
}
