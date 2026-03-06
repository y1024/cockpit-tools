/** Codex 账号数据 */
export interface CodexAccount {
  id: string;
  email: string;
  user_id?: string;
  plan_type?: string;
  account_id?: string;
  tokens: CodexTokens;
  quota?: CodexQuota;
  quota_error?: CodexQuotaErrorInfo;
  tags?: string[];
  created_at: number;
  last_used: number;
}

export interface CodexQuotaErrorInfo {
  code?: string;
  message: string;
  timestamp: number;
}

/** Codex Token 数据 */
export interface CodexTokens {
  id_token: string;
  access_token: string;
  refresh_token?: string;
}

/** Codex 配额数据 */
export interface CodexQuota {
  /** 5小时配额百分比 (0-100) */
  hourly_percentage: number;
  /** 5小时配额重置时间 (Unix timestamp) */
  hourly_reset_time?: number;
  /** 主窗口时长（分钟） */
  hourly_window_minutes?: number;
  /** 主窗口是否存在（接口返回） */
  hourly_window_present?: boolean;
  /** 周配额百分比 (0-100) */
  weekly_percentage: number;
  /** 周配额重置时间 (Unix timestamp) */
  weekly_reset_time?: number;
  /** 次窗口时长（分钟） */
  weekly_window_minutes?: number;
  /** 次窗口是否存在（接口返回） */
  weekly_window_present?: boolean;
  /** 原始响应数据 */
  raw_data?: unknown;
}

/** 获取订阅类型显示名称 */
export function getCodexPlanDisplayName(planType?: string): string {
  if (!planType) return 'FREE';
  const upper = planType.toUpperCase();
  if (upper.includes('TEAM')) return 'TEAM';
  if (upper.includes('ENTERPRISE')) return 'ENTERPRISE';
  if (upper.includes('PLUS')) return 'PLUS';
  if (upper.includes('PRO')) return 'PRO';
  return upper;
}

/** 获取配额百分比的样式类名 */
export function getCodexQuotaClass(percentage: number): string {
  if (percentage >= 80) return 'high';
  if (percentage >= 40) return 'medium';
  if (percentage >= 10) return 'low';
  return 'critical';
}

type Translate = (key: string, options?: Record<string, unknown>) => string;

export interface CodexQuotaWindow {
  id: 'primary' | 'secondary';
  label: string;
  percentage: number;
  resetTime?: number;
  windowMinutes?: number;
}

export function getCodexQuotaWindowLabel(
  windowMinutes: number | undefined,
  fallback: 'hourly' | 'weekly' = 'hourly'
): string {
  const HOUR_MINUTES = 60;
  const DAY_MINUTES = 24 * HOUR_MINUTES;
  const WEEK_MINUTES = 7 * DAY_MINUTES;
  const safeMinutes =
    typeof windowMinutes === 'number' && Number.isFinite(windowMinutes) && windowMinutes > 0
      ? Math.ceil(windowMinutes)
      : null;

  if (safeMinutes == null) {
    return fallback === 'weekly' ? 'Weekly' : '5h';
  }

  if (safeMinutes >= WEEK_MINUTES - 1) {
    const weeks = Math.ceil(safeMinutes / WEEK_MINUTES);
    return weeks <= 1 ? 'Weekly' : `${weeks} Week`;
  }

  if (safeMinutes >= DAY_MINUTES - 1) {
    return `${Math.ceil(safeMinutes / DAY_MINUTES)}d`;
  }

  if (safeMinutes >= HOUR_MINUTES) {
    return `${Math.ceil(safeMinutes / HOUR_MINUTES)}h`;
  }

  return `${Math.ceil(safeMinutes)}m`;
}

export function getCodexQuotaWindows(quota: CodexQuota | undefined): CodexQuotaWindow[] {
  if (!quota) return [];

  const windows: CodexQuotaWindow[] = [];
  const hasPresenceFlags =
    quota.hourly_window_present !== undefined || quota.weekly_window_present !== undefined;

  const appendPrimary = !hasPresenceFlags || quota.hourly_window_present === true;
  const appendSecondary = !hasPresenceFlags || quota.weekly_window_present === true;

  if (appendPrimary) {
    windows.push({
      id: 'primary',
      label: getCodexQuotaWindowLabel(quota.hourly_window_minutes, 'hourly'),
      percentage: quota.hourly_percentage,
      resetTime: quota.hourly_reset_time,
      windowMinutes: quota.hourly_window_minutes,
    });
  }

  if (appendSecondary) {
    windows.push({
      id: 'secondary',
      label: getCodexQuotaWindowLabel(quota.weekly_window_minutes, 'weekly'),
      percentage: quota.weekly_percentage,
      resetTime: quota.weekly_reset_time,
      windowMinutes: quota.weekly_window_minutes,
    });
  }

  if (windows.length > 0) {
    return windows;
  }

  return [
    {
      id: 'primary',
      label: getCodexQuotaWindowLabel(quota.hourly_window_minutes, 'hourly'),
      percentage: quota.hourly_percentage,
      resetTime: quota.hourly_reset_time,
      windowMinutes: quota.hourly_window_minutes,
    },
  ];
}

/** 格式化重置时间显示（相对时间 + 绝对时间） */
export function formatCodexResetTime(
  resetTime: number | undefined,
  t: Translate
): string {
  if (!resetTime) return '';

  const now = Math.floor(Date.now() / 1000);
  const diff = resetTime - now;

  if (diff <= 0) return t('common.shared.quota.resetDone');

  const totalMinutes = Math.floor(diff / 60);
  const days = Math.floor(totalMinutes / (60 * 24));
  const hours = Math.floor((totalMinutes % (60 * 24)) / 60);
  const minutes = totalMinutes % 60;

  let parts = [];
  if (days > 0) parts.push(`${days}d`);
  if (hours > 0) parts.push(`${hours}h`);
  if (minutes > 0) parts.push(`${minutes}m`);
  
  const relative = parts.length > 0 ? parts.join(' ') : '<1m';
  const absolute = formatCodexResetTimeAbsolute(resetTime);

  return `${relative} (${absolute})`;
}

export function formatCodexResetTimeAbsolute(
  resetTime: number | undefined
): string {
  if (!resetTime) return '';

  const resetDate = new Date(resetTime * 1000);
  
  const pad = (value: number) => String(value).padStart(2, '0');
  const month = pad(resetDate.getMonth() + 1);
  const day = pad(resetDate.getDate());
  const hours = pad(resetDate.getHours());
  const minutes = pad(resetDate.getMinutes());
  
  return `${month}/${day} ${hours}:${minutes}`;
}
