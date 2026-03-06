import type { Account } from '../types/account';
import type { CodexAccount } from '../types/codex';
import type { GitHubCopilotAccount } from '../types/githubCopilot';
import type { WindsurfAccount } from '../types/windsurf';
import type { KiroAccount, KiroAccountStatus } from '../types/kiro';
import {
  formatResetTimeDisplay,
  getAntigravityTierBadge,
  getDisplayModels,
  getModelShortName,
  getQuotaClass as getAntigravityQuotaClass,
  matchModelName,
} from '../utils/account';
import {
  formatCodexResetTime,
  getCodexPlanDisplayName,
  getCodexQuotaClass,
  getCodexQuotaWindows,
} from '../types/codex';
import {
  formatGitHubCopilotResetTime,
  getGitHubCopilotPlanDisplayName,
  getGitHubCopilotQuotaClass,
  getGitHubCopilotUsage,
} from '../types/githubCopilot';
import {
  formatWindsurfResetTime,
  getWindsurfAccountDisplayEmail,
  getWindsurfCreditsSummary,
  getWindsurfPlanDisplayName,
  getWindsurfQuotaClass,
} from '../types/windsurf';
import {
  formatKiroResetTime,
  getKiroAccountDisplayEmail,
  getKiroAccountDisplayUserId,
  getKiroAccountLoginProvider,
  getKiroAccountStatus,
  getKiroAccountStatusReason,
  getKiroCreditsSummary,
  getKiroPlanBadgeClass,
  getKiroPlanDisplayName,
  getKiroQuotaClass,
} from '../types/kiro';
import type { DisplayGroup, GroupSettings } from '../services/groupService';
import { calculateGroupQuota } from '../services/groupService';

type Translate = {
  (key: string): string;
  (key: string, defaultValue: string): string;
  (key: string, options: Record<string, unknown>): string;
  (
    key: string,
    defaultValue: string,
    options: Record<string, unknown>,
  ): string;
};

export interface UnifiedQuotaMetric {
  key: string;
  label: string;
  percentage: number;
  quotaClass: string;
  valueText: string;
  resetText?: string;
  resetAt?: string | number | null;
  used?: number;
  total?: number;
  left?: number;
}

export interface UnifiedAccountPresentation {
  id: string;
  displayName: string;
  planLabel: string;
  planClass: string;
  quotaItems: UnifiedQuotaMetric[];
  cycleText?: string;
}

export interface KiroAccountPresentation extends UnifiedAccountPresentation {
  userIdText: string;
  signedInWithText: string;
  addOnExpiryText: string;
  accountStatus: KiroAccountStatus;
  accountStatusReason: string | null;
  isBanned: boolean;
  hasStatusError: boolean;
}

export interface QuotaPreviewLine {
  key: string;
  label: string;
  percentage: number;
  quotaClass: string;
  text: string;
}

type AgQuotaDisplayItem = {
  key: string;
  label: string;
  percentage: number;
  resetTime: string;
};

export type CreditMetrics = {
  usedPercent: number;
  used: number;
  total: number;
  left: number;
};

function toFiniteNumber(value: number | null | undefined): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) return 0;
  if (value <= 0) return 0;
  if (value >= 100) return 100;
  return Math.round(value);
}

export function buildCreditMetrics(
  used: number | null | undefined,
  total: number | null | undefined,
  left: number | null | undefined,
): CreditMetrics {
  const safeUsed = toFiniteNumber(used);
  const safeTotal = toFiniteNumber(total);
  const safeLeft = toFiniteNumber(left);

  let usedPercent = 0;
  if (safeTotal != null && safeTotal > 0) {
    if (safeUsed != null) {
      usedPercent = clampPercent((safeUsed / safeTotal) * 100);
    } else if (safeLeft != null) {
      usedPercent = clampPercent(((safeTotal - safeLeft) / safeTotal) * 100);
    }
  }

  return {
    usedPercent,
    used: safeUsed ?? 0,
    total: safeTotal ?? 0,
    left: safeLeft ?? 0,
  };
}

function getAgAccountQuotas(account: Account): Record<string, number> {
  const quotas: Record<string, number> = {};
  if (!account.quota?.models) {
    return quotas;
  }
  for (const model of account.quota.models) {
    quotas[model.name] = model.percentage;
  }
  return quotas;
}

function buildAgDisplayGroupSettings(groups: DisplayGroup[]): GroupSettings {
  const settings: GroupSettings = {
    groupMappings: {},
    groupNames: {},
    groupOrder: groups.map((group) => group.id),
    updatedAt: 0,
    updatedBy: 'desktop',
  };

  for (const group of groups) {
    settings.groupNames[group.id] = group.name;
    for (const modelId of group.models) {
      settings.groupMappings[modelId] = group.id;
    }
  }
  return settings;
}

export function getAntigravityGroupResetTimestamp(account: Account, group: DisplayGroup): number | null {
  if (!account.quota?.models?.length) {
    return null;
  }

  let earliest: number | null = null;
  for (const model of account.quota.models) {
    const belongsToGroup = group.models.some((groupModelId) => matchModelName(model.name, groupModelId));
    if (!belongsToGroup) {
      continue;
    }
    const parsed = new Date(model.reset_time);
    if (Number.isNaN(parsed.getTime())) {
      continue;
    }
    const timestamp = parsed.getTime();
    if (earliest === null || timestamp < earliest) {
      earliest = timestamp;
    }
  }
  return earliest;
}

export function getAntigravityQuotaDisplayItems(account: Account, displayGroups: DisplayGroup[]): AgQuotaDisplayItem[] {
  const rawDisplayModels = getDisplayModels(account.quota);
  if (rawDisplayModels.length === 0) {
    return [];
  }

  if (displayGroups.length === 0) {
    return rawDisplayModels.map((model) => ({
      key: model.name,
      label: getModelShortName(model.name),
      percentage: model.percentage,
      resetTime: model.reset_time,
    }));
  }

  const quotas = getAgAccountQuotas(account);
  const settings = buildAgDisplayGroupSettings(displayGroups);
  const groupedItems: AgQuotaDisplayItem[] = [];

  for (const group of displayGroups) {
    const percentage = calculateGroupQuota(group.id, quotas, settings);
    if (percentage === null) continue;

    const resetTimestamp = getAntigravityGroupResetTimestamp(account, group);
    groupedItems.push({
      key: `group:${group.id}`,
      label: group.name,
      percentage,
      resetTime: resetTimestamp ? new Date(resetTimestamp).toISOString() : '',
    });
  }

  if (groupedItems.length > 0) {
    return groupedItems;
  }

  return rawDisplayModels.map((model) => ({
    key: model.name,
    label: getModelShortName(model.name),
    percentage: model.percentage,
    resetTime: model.reset_time,
  }));
}

export function buildAntigravityAccountPresentation(
  account: Account,
  displayGroups: DisplayGroup[],
  t: Translate,
): UnifiedAccountPresentation {
  const tierBadge = getAntigravityTierBadge(account.quota);
  const quotaItems = getAntigravityQuotaDisplayItems(account, displayGroups).map((item) => ({
    key: item.key,
    label: item.label,
    percentage: item.percentage,
    quotaClass: getAntigravityQuotaClass(item.percentage),
    valueText: `${item.percentage}%`,
    resetText: item.resetTime ? formatResetTimeDisplay(item.resetTime, t) : '',
    resetAt: item.resetTime,
  }));

  return {
    id: account.id,
    displayName: account.email,
    planLabel: tierBadge.label,
    planClass: tierBadge.className,
    quotaItems,
  };
}

export function buildCodexAccountPresentation(
  account: CodexAccount,
  t: Translate,
): UnifiedAccountPresentation {
  const normalizedPlan = getCodexPlanDisplayName(account.plan_type);
  const rawPlan = account.plan_type?.trim();
  const quotaItems = getCodexQuotaWindows(account.quota).map((window) => ({
    key: window.id,
    label: window.label,
    percentage: window.percentage,
    quotaClass: getCodexQuotaClass(window.percentage),
    valueText: `${window.percentage}%`,
    resetText: window.resetTime ? formatCodexResetTime(window.resetTime, t) : '',
    resetAt: window.resetTime,
  }));

  return {
    id: account.id,
    displayName: account.email,
    planLabel: rawPlan || normalizedPlan,
    planClass: normalizedPlan.toLowerCase(),
    quotaItems,
  };
}

function buildCopilotMetric(
  percentage: number | null | undefined,
  included: boolean | undefined,
  quotaClassGetter: (value: number) => string,
  includedText: string,
) {
  if (included) {
    return {
      valueText: includedText,
      percentage: 100,
      quotaClass: quotaClassGetter(0),
    };
  }
  if (typeof percentage !== 'number' || !Number.isFinite(percentage)) {
    return {
      valueText: '-',
      percentage: 0,
      quotaClass: quotaClassGetter(0),
    };
  }
  const normalized = Math.max(0, Math.min(100, Math.round(percentage)));
  return {
    valueText: `${normalized}%`,
    percentage: normalized,
    quotaClass: quotaClassGetter(normalized),
  };
}

export function buildGitHubCopilotAccountPresentation(
  account: GitHubCopilotAccount,
  t: Translate,
): UnifiedAccountPresentation {
  const displayName = account.email ?? account.github_email ?? account.github_login;
  const normalizedPlan = getGitHubCopilotPlanDisplayName(account.plan_type || account.copilot_plan);
  const rawPlan = account.plan_type?.trim() || account.copilot_plan?.trim();
  const usage = getGitHubCopilotUsage(account);
  const includedText = t('githubCopilot.usage.included', 'Included');

  const inline = buildCopilotMetric(
    usage.inlineSuggestionsUsedPercent,
    usage.inlineIncluded,
    getGitHubCopilotQuotaClass,
    includedText,
  );
  const chat = buildCopilotMetric(
    usage.chatMessagesUsedPercent,
    usage.chatIncluded,
    getGitHubCopilotQuotaClass,
    includedText,
  );
  const premium = buildCopilotMetric(
    usage.premiumRequestsUsedPercent,
    usage.premiumIncluded,
    getGitHubCopilotQuotaClass,
    includedText,
  );

  const inlineReset = account.quota?.hourly_reset_time ?? usage.allowanceResetAt ?? null;
  const chatReset = account.quota?.weekly_reset_time ?? usage.allowanceResetAt ?? null;

  return {
    id: account.id,
    displayName,
    planLabel: rawPlan || normalizedPlan,
    planClass: normalizedPlan.toLowerCase(),
    quotaItems: [
      {
        key: 'inline',
        label: t('common.shared.quota.hourly', 'Inline Suggestions'),
        percentage: inline.percentage,
        quotaClass: inline.quotaClass,
        valueText: inline.valueText,
        resetText: inlineReset ? formatGitHubCopilotResetTime(inlineReset, t) : '',
        resetAt: inlineReset,
      },
      {
        key: 'chat',
        label: t('common.shared.quota.weekly', 'Chat messages'),
        percentage: chat.percentage,
        quotaClass: chat.quotaClass,
        valueText: chat.valueText,
        resetText: chatReset ? formatGitHubCopilotResetTime(chatReset, t) : '',
        resetAt: chatReset,
      },
      {
        key: 'premium',
        label: t('githubCopilot.columns.premium', 'Premium requests'),
        percentage: premium.percentage,
        quotaClass: premium.quotaClass,
        valueText: premium.valueText,
      },
    ],
  };
}

export function buildWindsurfAccountPresentation(
  account: WindsurfAccount,
  t: Translate,
): UnifiedAccountPresentation {
  const credits = getWindsurfCreditsSummary(account);
  const normalizedPlan = getWindsurfPlanDisplayName(
    account.plan_type ?? account.copilot_plan ?? credits.planName ?? null,
  );
  const rawPlan = account.plan_type?.trim() || account.copilot_plan?.trim() || credits.planName?.trim();
  const promptMetrics = buildCreditMetrics(
    credits.promptCreditsUsed,
    credits.promptCreditsTotal,
    credits.promptCreditsLeft,
  );
  const addOnMetrics = buildCreditMetrics(
    credits.addOnCreditsUsed,
    credits.addOnCreditsTotal,
    credits.addOnCredits,
  );

  return {
    id: account.id,
    displayName: account.email?.trim() || getWindsurfAccountDisplayEmail(account),
    planLabel: rawPlan || normalizedPlan,
    planClass: normalizedPlan.toLowerCase(),
    cycleText: credits.planEndsAt
      ? formatWindsurfResetTime(credits.planEndsAt, t)
      : t('common.shared.credits.planEndsUnknown', '配额周期时间未知'),
    quotaItems: [
      {
        key: 'prompt',
        label: t('common.shared.columns.promptCredits', 'User Prompt credits'),
        percentage: promptMetrics.usedPercent,
        quotaClass: getWindsurfQuotaClass(promptMetrics.usedPercent),
        valueText: `${promptMetrics.usedPercent}%`,
        used: promptMetrics.used,
        total: promptMetrics.total,
        left: promptMetrics.left,
      },
      {
        key: 'addon',
        label: t('common.shared.columns.addOnPromptCredits', 'Add-on prompt credits'),
        percentage: addOnMetrics.usedPercent,
        quotaClass: getWindsurfQuotaClass(addOnMetrics.usedPercent),
        valueText: `${addOnMetrics.usedPercent}%`,
        used: addOnMetrics.used,
        total: addOnMetrics.total,
        left: addOnMetrics.left,
      },
    ],
  };
}

function shouldShowKiroAddOn(
  addOnMetrics: CreditMetrics,
  bonusExpireDays: number | null | undefined,
): boolean {
  return (
    addOnMetrics.left > 0 ||
    addOnMetrics.used > 0 ||
    addOnMetrics.total > 0 ||
    (typeof bonusExpireDays === 'number' && Number.isFinite(bonusExpireDays) && bonusExpireDays > 0)
  );
}

export function buildKiroAccountPresentation(
  account: KiroAccount,
  t: Translate,
): KiroAccountPresentation {
  const credits = getKiroCreditsSummary(account);
  const normalizedPlan = getKiroPlanDisplayName(
    account.plan_type ?? account.plan_name ?? account.plan_tier ?? credits.planName ?? null,
  );
  const rawPlan = account.plan_type?.trim() || account.plan_name?.trim() || account.plan_tier?.trim();
  const promptMetrics = buildCreditMetrics(
    credits.promptCreditsUsed,
    credits.promptCreditsTotal,
    credits.promptCreditsLeft,
  );
  const addOnMetrics = buildCreditMetrics(
    credits.addOnCreditsUsed,
    credits.addOnCreditsTotal,
    credits.addOnCredits,
  );
  const showAddOn = shouldShowKiroAddOn(addOnMetrics, credits.bonusExpireDays);
  const accountStatus = getKiroAccountStatus(account);
  const accountStatusReason = getKiroAccountStatusReason(account);
  const provider = getKiroAccountLoginProvider(account);
  const signedInWithText = provider
    ? t('kiro.account.signedInWithProvider', {
        provider,
        defaultValue: 'Signed in with {{provider}}',
      })
    : t('kiro.account.signedInWithUnknown', 'Signed in with unknown');

  const addOnExpiryText =
    typeof credits.bonusExpireDays === 'number' && Number.isFinite(credits.bonusExpireDays)
      ? t('kiro.credits.expiryDays', {
          days: Math.max(0, Math.round(credits.bonusExpireDays)),
          defaultValue: '{{days}} days',
        })
      : t('kiro.credits.expiryUnknown', '—');

  const quotaItems: UnifiedQuotaMetric[] = [
    {
      key: 'prompt',
      label: t('common.shared.columns.promptCredits', 'User Prompt credits'),
      percentage: promptMetrics.usedPercent,
      quotaClass: getKiroQuotaClass(promptMetrics.usedPercent),
      valueText: `${promptMetrics.usedPercent}%`,
      used: promptMetrics.used,
      total: promptMetrics.total,
      left: promptMetrics.left,
    },
  ];

  if (showAddOn) {
    quotaItems.push({
      key: 'addon',
      label: t('common.shared.columns.addOnPromptCredits', 'Add-on prompt credits'),
      percentage: addOnMetrics.usedPercent,
      quotaClass: getKiroQuotaClass(addOnMetrics.usedPercent),
      valueText: `${addOnMetrics.usedPercent}%`,
      used: addOnMetrics.used,
      total: addOnMetrics.total,
      left: addOnMetrics.left,
    });
  }

  return {
    id: account.id,
    displayName: getKiroAccountDisplayEmail(account),
    userIdText: getKiroAccountDisplayUserId(account),
    signedInWithText,
    addOnExpiryText,
    planLabel: rawPlan || normalizedPlan,
    planClass: getKiroPlanBadgeClass(rawPlan || normalizedPlan),
    accountStatus,
    accountStatusReason,
    isBanned: accountStatus === 'banned',
    hasStatusError: accountStatus === 'error',
    cycleText: credits.planEndsAt
      ? formatKiroResetTime(credits.planEndsAt, t)
      : t('common.shared.credits.planEndsUnknown', '配额周期时间未知'),
    quotaItems,
  };
}

export function buildQuotaPreviewLines(
  quotaItems: UnifiedQuotaMetric[],
  limit = 3,
): QuotaPreviewLine[] {
  return quotaItems.slice(0, Math.max(0, limit)).map((item) => ({
    key: item.key,
    label: item.label,
    percentage: item.percentage,
    quotaClass: item.quotaClass,
    text: `${item.label} ${item.valueText}`,
  }));
}
