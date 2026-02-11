/** Windsurf 账号数据（后端原样返回的结构） */
export interface WindsurfAccount {
  id: string;
  github_login: string;
  github_id: number;
  github_name?: string | null;
  github_email?: string | null;
  tags?: string[] | null;

  // 注意：这里包含敏感信息。前端不应打印/上报。
  github_access_token: string;
  github_token_type?: string | null;
  github_scope?: string | null;
  copilot_token: string;

  copilot_plan?: string | null;
  copilot_chat_enabled?: boolean | null;
  copilot_expires_at?: number | null;
  copilot_refresh_in?: number | null;
  copilot_quota_snapshots?: unknown;
  copilot_quota_reset_date?: string | null;
  copilot_limited_user_quotas?: unknown;
  copilot_limited_user_reset_date?: number | null;

  created_at: number;
  last_used: number;

  // ---- 兼容旧 UI（从 Codex 页面复制而来） ----
  // 这些字段不会由后端直接返回，需要在前端做映射/派生。
  email?: string;
  plan_type?: string;
  quota?: WindsurfQuota;
}

export type WindsurfQuotaClass = 'high' | 'medium' | 'low' | 'critical';
export type WindsurfPlanBadge = 'FREE' | 'INDIVIDUAL' | 'PRO' | 'BUSINESS' | 'ENTERPRISE' | 'UNKNOWN';

export function getWindsurfPlanDisplayName(planType?: string | null): string {
  if (!planType) return 'UNKNOWN';
  const upper = planType.toUpperCase();
  if (upper.includes('FREE')) return 'FREE';
  if (upper.includes('INDIVIDUAL_PRO')) return 'PRO';
  if (upper === 'PRO') return 'PRO';
  if (upper.includes('INDIVIDUAL')) return 'INDIVIDUAL';
  if (upper.includes('BUSINESS')) return 'BUSINESS';
  if (upper.includes('ENTERPRISE')) return 'ENTERPRISE';
  return upper;
}

function resolvePlanFromSku(sku: string): WindsurfPlanBadge | null {
  const lower = sku.toLowerCase();
  if (!lower) return null;
  if (lower.includes('free_limited') || lower.includes('no_auth_limited')) return 'FREE';
  if (lower.includes('enterprise')) return 'ENTERPRISE';
  if (lower.includes('business')) return 'BUSINESS';
  if (lower.includes('individual_pro') || lower === 'pro' || lower.includes('_pro')) return 'PRO';
  if (lower.includes('individual')) return 'INDIVIDUAL';
  return null;
}

export function getWindsurfPlanBadge(account: WindsurfAccount): WindsurfPlanBadge {
  const tokenMap = parseTokenMap(account.copilot_token || '');
  const skuBadge = resolvePlanFromSku(tokenMap['sku'] || '');
  if (skuBadge) return skuBadge;

  const normalizedPlan = getWindsurfPlanDisplayName(account.copilot_plan);
  switch (normalizedPlan) {
    case 'FREE':
      return 'FREE';
    case 'PRO':
      return 'PRO';
    case 'INDIVIDUAL':
      return 'INDIVIDUAL';
    case 'BUSINESS':
      return 'BUSINESS';
    case 'ENTERPRISE':
      return 'ENTERPRISE';
    default:
      return 'UNKNOWN';
  }
}

export function getWindsurfQuotaClass(percentage: number): WindsurfQuotaClass {
  // Windsurf 页面展示的是“使用量”：使用越高，风险颜色越高。
  if (percentage <= 20) return 'high';
  if (percentage <= 60) return 'medium';
  if (percentage <= 85) return 'low';
  return 'critical';
}

export function getWindsurfAccountDisplayEmail(account: WindsurfAccount): string {
  return account.github_email?.trim() || account.github_login;
}

type Translate = (key: string, options?: Record<string, unknown>) => string;

export type WindsurfUsage = {
  inlineSuggestionsUsedPercent: number | null;
  chatMessagesUsedPercent: number | null;
  allowanceResetAt?: number | null; // unix seconds
  remainingCompletions?: number | null;
  remainingChat?: number | null;
  totalCompletions?: number | null;
  totalChat?: number | null;
};

/** 兼容 Codex 风格的 quota 结构（用于复用 UI 组件/样式） */
export interface WindsurfQuota {
  hourly_percentage: number;
  hourly_reset_time?: number | null;
  weekly_percentage: number;
  weekly_reset_time?: number | null;
  raw_data?: unknown;
}

function parseTokenMap(token: string): Record<string, string> {
  const map: Record<string, string> = {};
  const prefix = token.split(':')[0] ?? token;
  for (const part of prefix.split(';')) {
    const [k, v] = part.split('=');
    const key = (k || '').trim();
    if (!key) continue;
    map[key] = (v || '').trim();
  }
  return map;
}

function isFreeLimitedSku(account: WindsurfAccount, tokenMap: Record<string, string>): boolean {
  const sku = (tokenMap['sku'] || '').toLowerCase();
  if (sku.includes('free_limited')) return true;
  const plan = (account.copilot_plan || '').toLowerCase();
  return plan.includes('free_limited');
}

function getPremiumQuotaSnapshot(account: WindsurfAccount): Record<string, unknown> | null {
  const raw = account.copilot_quota_snapshots as unknown;
  if (!raw || typeof raw !== 'object') return null;
  const snapshots = raw as Record<string, unknown>;

  const premiumInteractions = snapshots['premium_interactions'];
  if (premiumInteractions && typeof premiumInteractions === 'object') {
    return premiumInteractions as Record<string, unknown>;
  }

  const premiumModels = snapshots['premium_models'];
  if (premiumModels && typeof premiumModels === 'object') {
    return premiumModels as Record<string, unknown>;
  }

  return null;
}

function getNumber(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string') {
    const n = Number(value);
    return Number.isFinite(n) ? n : null;
  }
  return null;
}

function getLimitedQuota(account: WindsurfAccount, key: 'chat' | 'completions'): number | null {
  const raw = account.copilot_limited_user_quotas as any;
  if (!raw || typeof raw !== 'object') return null;
  return getNumber(raw[key]);
}

function pickAllowanceResetAt(account: WindsurfAccount): number | null {
  if (typeof account.copilot_limited_user_reset_date === 'number') {
    return account.copilot_limited_user_reset_date;
  }
  if (typeof account.copilot_quota_reset_date === 'string' && account.copilot_quota_reset_date.trim()) {
    const parsed = Date.parse(account.copilot_quota_reset_date);
    if (Number.isFinite(parsed)) {
      return Math.floor(parsed / 1000);
    }
  }
  const tokenMap = parseTokenMap(account.copilot_token || '');
  const rd = tokenMap['rd'];
  if (rd) {
    const head = rd.split(':')[0];
    const n = Number(head);
    if (Number.isFinite(n)) return n;
  }
  return null;
}

function clampPercent(value: number): number {
  if (value < 0) return 0;
  if (value > 100) return 100;
  return Math.round(value);
}

function calcUsedPercent(total: number | null, remaining: number | null): number | null {
  if (total == null || remaining == null) return null;
  if (total <= 0) return null;
  // remaining 可能会大于 total（异常/不同计划），这里做一个宽松处理
  const used = Math.max(0, total - remaining);
  return clampPercent((used / total) * 100);
}

function calcUsedPercentFromPremiumSnapshot(snapshot: Record<string, unknown>): number | null {
  const unlimited = snapshot['unlimited'] === true;
  if (unlimited) return 0;

  const entitlement = getNumber(snapshot['entitlement']);
  if (entitlement != null && entitlement < 0) {
    return 0;
  }

  const percentRemaining = getNumber(snapshot['percent_remaining']);
  if (percentRemaining != null) {
    return clampPercent(100 - percentRemaining);
  }

  return null;
}

function calcRemainingFromPremiumSnapshot(snapshot: Record<string, unknown>): number | null {
  const entitlement = getNumber(snapshot['entitlement']);
  const percentRemaining = getNumber(snapshot['percent_remaining']);
  if (entitlement == null || percentRemaining == null || entitlement <= 0) return null;
  return Math.max(0, Math.round((entitlement * percentRemaining) / 100));
}

export function getWindsurfUsage(account: WindsurfAccount): WindsurfUsage {
  const tokenMap = parseTokenMap(account.copilot_token || '');
  const freeLimited = isFreeLimitedSku(account, tokenMap);

  // 与 VS Code 扩展口径对齐：付费用户优先使用 quota_snapshots.premium_interactions。
  if (!freeLimited) {
    const premiumSnapshot = getPremiumQuotaSnapshot(account);
    if (premiumSnapshot) {
      const usedPercent = calcUsedPercentFromPremiumSnapshot(premiumSnapshot);
      const entitlement = getNumber(premiumSnapshot['entitlement']);
      const remaining = calcRemainingFromPremiumSnapshot(premiumSnapshot);

      return {
        inlineSuggestionsUsedPercent: usedPercent,
        chatMessagesUsedPercent: usedPercent,
        allowanceResetAt: pickAllowanceResetAt(account),
        remainingCompletions: remaining,
        remainingChat: remaining,
        totalCompletions: entitlement,
        totalChat: entitlement,
      };
    }
  }

  const remainingCompletions = getLimitedQuota(account, 'completions');
  const remainingChat = getLimitedQuota(account, 'chat');

  const totalCompletions = getNumber(tokenMap['cq']) ?? (remainingCompletions ?? null);
  // VS Code Windsurf Free Usage 的 chat 口径：
  // free_limited 账号一般按 500 总额度计算已用百分比。
  let totalChat = getNumber(tokenMap['tq']);
  if (totalChat == null) {
    if (freeLimited && remainingChat != null) {
      totalChat = 500;
    } else {
      totalChat = remainingChat ?? null;
    }
  }

  return {
    inlineSuggestionsUsedPercent: calcUsedPercent(totalCompletions, remainingCompletions),
    chatMessagesUsedPercent: calcUsedPercent(totalChat, remainingChat),
    allowanceResetAt: pickAllowanceResetAt(account),
    remainingCompletions,
    remainingChat,
    totalCompletions,
    totalChat,
  };
}

export function formatUnixSecondsToYmd(seconds: number, locale = 'zh-CN'): string {
  const date = new Date(seconds * 1000);
  if (Number.isNaN(date.getTime())) return '';
  return new Intl.DateTimeFormat(locale, { year: 'numeric', month: '2-digit', day: '2-digit' }).format(date);
}

export function formatWindsurfAllowanceResetLine(
  account: WindsurfAccount,
  t: Translate,
  locale = 'zh-CN',
): string {
  const usage = getWindsurfUsage(account);
  const resetAt = usage.allowanceResetAt;
  if (!resetAt) return t('windsurf.usage.resetUnknown', { defaultValue: 'Allowance resets -' });
  const dateText = formatUnixSecondsToYmd(resetAt, locale);
  if (!dateText) return t('windsurf.usage.resetUnknown', { defaultValue: 'Allowance resets -' });
  return t('windsurf.usage.resetLine', {
    dateText,
    defaultValue: 'Allowance resets {{dateText}}.',
  });
}

export function formatWindsurfResetTime(
  resetTime: number | null | undefined,
  t: Translate,
): string {
  if (!resetTime) return '';
  const now = Math.floor(Date.now() / 1000);
  const diff = resetTime - now;
  if (diff <= 0) return t('windsurf.quota.resetDone', { defaultValue: '已重置' });

  const totalMinutes = Math.floor(diff / 60);
  const days = Math.floor(totalMinutes / (60 * 24));
  const hours = Math.floor((totalMinutes % (60 * 24)) / 60);
  const minutes = totalMinutes % 60;

  let relative = t('windsurf.time.lessThanMinute', { defaultValue: '<1m' });
  if (days > 0 && hours > 0) {
    relative = t('windsurf.time.relativeDaysHours', {
      days,
      hours,
      defaultValue: '{{days}}d {{hours}}h',
    });
  } else if (days > 0) {
    relative = t('windsurf.time.relativeDays', {
      days,
      defaultValue: '{{days}}d',
    });
  } else if (hours > 0 && minutes > 0) {
    relative = t('windsurf.time.relativeHoursMinutes', {
      hours,
      minutes,
      defaultValue: '{{hours}}h {{minutes}}m',
    });
  } else if (hours > 0) {
    relative = t('windsurf.time.relativeHours', {
      hours,
      defaultValue: '{{hours}}h',
    });
  } else if (minutes > 0) {
    relative = t('windsurf.time.relativeMinutes', {
      minutes,
      defaultValue: '{{minutes}}m',
    });
  }

  const absolute = formatWindsurfResetTimeAbsolute(resetTime);
  return t('windsurf.time.relativeWithAbsolute', {
    relative,
    absolute,
    defaultValue: '{{relative}} ({{absolute}})',
  });
}

export function formatWindsurfResetTimeAbsolute(resetTime: number | null | undefined): string {
  if (!resetTime) return '';
  const date = new Date(resetTime * 1000);
  if (Number.isNaN(date.getTime())) return '';
  const pad = (value: number) => String(value).padStart(2, '0');
  const month = pad(date.getMonth() + 1);
  const day = pad(date.getDate());
  const hours = pad(date.getHours());
  const minutes = pad(date.getMinutes());
  return `${month}/${day} ${hours}:${minutes}`;
}
