export interface CodebuddyAccount {
  id: string;
  email: string;
  uid?: string | null;
  nickname?: string | null;
  enterprise_id?: string | null;
  enterprise_name?: string | null;
  tags?: string[] | null;

  access_token: string;
  refresh_token?: string | null;
  token_type?: string | null;
  expires_at?: number | null;
  domain?: string | null;

  plan_type?: string;
  dosage_notify_code?: string;
  dosage_notify_zh?: string;
  dosage_notify_en?: string;
  payment_type?: string;

  quota_raw?: unknown;
  auth_raw?: unknown;
  profile_raw?: unknown;
  usage_raw?: unknown;

  status?: string | null;
  status_reason?: string | null;
  quota_query_last_error?: string | null;
  quota_query_last_error_at?: number | null;

  created_at: number;
  last_used: number;
}

export type CodebuddyPlanBadge = 'FREE' | 'PRO' | 'TRIAL' | 'ENTERPRISE' | 'UNKNOWN';

/**
 * Aligned with the official CodeBuddy web client's PackageCode enum (`ce`).
 * Source: main-*.js → `var ce = (e => ( ... ))(ce || {})`
 */
export const CB_PACKAGE_CODE = {
  free: 'TCACA_code_001_PqouKr6QWV',
  proMon: 'TCACA_code_002_AkiJS3ZHF5',
  proYear: 'TCACA_code_003_FAnt7lcmRT',
  gift: 'TCACA_code_006_DbXS0lrypC',
  activity: 'TCACA_code_007_nzdH5h4Nl0',
  freeMon: 'TCACA_code_008_cfWoLwvjU4',
  extra: 'TCACA_code_009_0XmEQc2xOf',
} as const;

/**
 * Aligned with the official CodeBuddy web client's resource status enum (`st`).
 */
export const CB_RESOURCE_STATUS = {
  valid: 0,
  refund: 1,
  expired: 2,
  usedUp: 3,
} as const;

const CB_ENTERPRISE_ACCOUNT_TYPES = ['ultimate', 'exclusive', 'premise'];

export interface CodebuddyPlanDetail {
  type: 'pro' | 'free';
  isPro: boolean;
  isTrial: boolean;
  badge: string;
  packageCode: string | null;
}

export function getCodebuddyAccountDisplayEmail(account: CodebuddyAccount): string {
  return account.email || account.nickname || account.uid || account.id;
}

export function getCodebuddyAccountDisplayName(account: CodebuddyAccount): string {
  return account.nickname || account.email || account.uid || account.id;
}

function extractResourceAccounts(account: CodebuddyAccount): Array<Record<string, unknown>> {
  const usageRoot = asRecord(account.usage_raw);
  const quotaRoot = asRecord(account.quota_raw);
  const userResource = asRecord(quotaRoot?.userResource) ?? usageRoot;
  const data = asRecord(userResource?.data);
  const response = asRecord(data?.Response);
  const payload = asRecord(response?.Data);
  const list = Array.isArray(payload?.Accounts) ? (payload!.Accounts as unknown[]) : [];
  return list.filter((a): a is Record<string, unknown> => a != null && typeof a === 'object');
}

/**
 * Determine plan detail following the official CodeBuddy web client logic:
 *   1. Enterprise account types override everything.
 *   2. Filter resource accounts by Status ∈ {valid(0), usedUp(3)}.
 *   3. isPro = has proYear or proMon package with active status.
 *   4. isTrial = has gift package with active status.
 *   5. badge: PRO > TRIAL > FREE; fallback to payment_type when no resource data.
 */
export function getCodebuddyPlanDetail(account: CodebuddyAccount): CodebuddyPlanDetail {
  const profile = asRecord(account.profile_raw);
  const accountType = typeof profile?.type === 'string' ? profile.type.toLowerCase() : '';
  if (CB_ENTERPRISE_ACCOUNT_TYPES.includes(accountType)) {
    return { type: 'pro', isPro: true, isTrial: false, badge: 'ENTERPRISE', packageCode: null };
  }

  const all = extractResourceAccounts(account);
  const active = all.filter((a) => {
    const s = typeof a.Status === 'number' ? a.Status : -1;
    return s === CB_RESOURCE_STATUS.valid || s === CB_RESOURCE_STATUS.usedUp;
  });

  const proPkg = active.find((a) => {
    const c = typeof a.PackageCode === 'string' ? a.PackageCode : '';
    return c === CB_PACKAGE_CODE.proYear || c === CB_PACKAGE_CODE.proMon;
  });

  const hasGift = active.some((a) => {
    const c = typeof a.PackageCode === 'string' ? a.PackageCode : '';
    return c === CB_PACKAGE_CODE.gift;
  });

  if (proPkg) {
    const code = typeof proPkg.PackageCode === 'string' ? proPkg.PackageCode : null;
    return { type: 'pro', isPro: true, isTrial: hasGift, badge: 'PRO', packageCode: code };
  }

  if (hasGift) {
    return { type: 'free', isPro: false, isTrial: true, badge: 'TRIAL', packageCode: CB_PACKAGE_CODE.gift };
  }

  if (all.length === 0) {
    return planBadgeFallback(account);
  }

  return { type: 'free', isPro: false, isTrial: false, badge: 'FREE', packageCode: null };
}

function planBadgeFallback(account: CodebuddyAccount): CodebuddyPlanDetail {
  const payment = account.payment_type?.toLowerCase() || '';
  const plan = account.plan_type?.toLowerCase() || '';
  const source = payment || plan;

  if (source.includes('enterprise'))
    return { type: 'pro', isPro: true, isTrial: false, badge: 'ENTERPRISE', packageCode: null };
  if (source.includes('trial'))
    return { type: 'free', isPro: false, isTrial: true, badge: 'TRIAL', packageCode: null };
  if (source.includes('pro'))
    return { type: 'pro', isPro: true, isTrial: false, badge: 'PRO', packageCode: null };
  if (source.includes('free'))
    return { type: 'free', isPro: false, isTrial: false, badge: 'FREE', packageCode: null };
  if (source) {
    const raw = (account.payment_type || account.plan_type || 'UNKNOWN').toUpperCase();
    return { type: 'free', isPro: false, isTrial: false, badge: raw, packageCode: null };
  }
  return { type: 'free', isPro: false, isTrial: false, badge: 'UNKNOWN', packageCode: null };
}

export function getCodebuddyPlanBadge(account: CodebuddyAccount): string {
  return getCodebuddyPlanDetail(account).badge;
}

export function getCodebuddyPlanBadgeClass(badge: string): string {
  switch (badge) {
    case 'FREE':
      return 'plan-badge plan-free';
    case 'PRO':
      return 'plan-badge plan-pro';
    case 'TRIAL':
      return 'plan-badge plan-trial';
    case 'ENTERPRISE':
      return 'plan-badge plan-enterprise';
    default:
      return 'plan-badge plan-unknown';
  }
}

export interface CodebuddyUsage {
  dosageNotifyCode?: string;
  dosageNotifyZh?: string;
  dosageNotifyEn?: string;
  paymentType?: string;
  isNormal: boolean;
  inlineSuggestionsUsedPercent: number | null;
  chatMessagesUsedPercent: number | null;
  allowanceResetAt?: number | null;
}

export function getCodebuddyUsage(account: CodebuddyAccount): CodebuddyUsage {
  const code = account.dosage_notify_code || '';
  return {
    dosageNotifyCode: code,
    dosageNotifyZh: account.dosage_notify_zh || undefined,
    dosageNotifyEn: account.dosage_notify_en || undefined,
    paymentType: account.payment_type || undefined,
    isNormal: !code || code === '0' || code === 'USAGE_NORMAL',
    inlineSuggestionsUsedPercent: null,
    chatMessagesUsedPercent: null,
    allowanceResetAt: null,
  };
}

/**
 * Align with official account menu:
 * aggregate active package `CycleCapacityRemainPrecise` as credits balance.
 */
export function getCodebuddyCreditsBalance(account: CodebuddyAccount): number | null {
  const active = extractResourceAccounts(account).filter((item) => isActiveResource(item));
  if (active.length === 0) return null;
  const balance = active.reduce((sum, item) => sum + parseCycleRemain(item), 0);
  if (!Number.isFinite(balance)) return null;
  return Math.max(0, balance);
}

export function getCodebuddyAccountStatus(account: CodebuddyAccount): string {
  return account.status || 'unknown';
}

export interface CodebuddyResourceSummary {
  packageName: string | null;
  cycleStartTime: string | null;
  cycleEndTime: string | null;
  remain: number | null;
  used: number | null;
  total: number | null;
  remainPercent: number | null;
  boundUpdatedAt: number | null;
}

export interface CodebuddyOfficialQuotaResource {
  packageCode: string | null;
  packageName: string | null;
  cycleStartTime: string | null;
  cycleEndTime: string | null;
  deductionEndTime: number | null;
  expiredTime: string | null;
  total: number;
  remain: number;
  used: number;
  usedPercent: number;
  remainPercent: number | null;
  refreshAt: number | null;
  expireAt: number | null;
  isBasePackage: boolean;
}

export interface CodebuddyOfficialQuotaModel {
  resources: CodebuddyOfficialQuotaResource[];
  extra: CodebuddyOfficialQuotaResource;
  updatedAt: number | null;
}

function getAccountQuotaUpdatedAtMs(account: CodebuddyAccount): number | null {
  const lastUsed = account.last_used;
  if (typeof lastUsed !== 'number' || !Number.isFinite(lastUsed) || lastUsed <= 0) return null;
  return Math.trunc(lastUsed * 1000);
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' ? (value as Record<string, unknown>) : null;
}

function parseNumeric(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string' && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function isExtraPackage(a: Record<string, unknown>): boolean {
  return typeof a.PackageCode === 'string' && a.PackageCode === CB_PACKAGE_CODE.extra;
}

function isActiveResource(a: Record<string, unknown>): boolean {
  const s = typeof a.Status === 'number' ? a.Status : -1;
  return s === CB_RESOURCE_STATUS.valid || s === CB_RESOURCE_STATUS.usedUp;
}

function parseDateTimeToEpoch(value: unknown): number | null {
  if (typeof value !== 'string') return null;
  const text = value.trim();
  if (!text) return null;
  const isoText = text.includes('T') ? text : text.replace(' ', 'T');
  const parsed = Date.parse(isoText);
  return Number.isFinite(parsed) ? parsed : null;
}

function parseCycleTotal(a: Record<string, unknown>): number {
  return (
    parseNumeric(a.CycleCapacitySizePrecise) ??
    parseNumeric(a.CycleCapacitySize) ??
    parseNumeric(a.CapacitySizePrecise) ??
    parseNumeric(a.CapacitySize) ??
    0
  );
}

function parseCycleRemain(a: Record<string, unknown>): number {
  return (
    parseNumeric(a.CycleCapacityRemainPrecise) ??
    parseNumeric(a.CycleCapacityRemain) ??
    parseNumeric(a.CapacityRemainPrecise) ??
    parseNumeric(a.CapacityRemain) ??
    0
  );
}

function isTrialOrFreeMonPackage(a: Record<string, unknown>): boolean {
  const code = typeof a.PackageCode === 'string' ? a.PackageCode : '';
  return code === CB_PACKAGE_CODE.gift || code === CB_PACKAGE_CODE.freeMon;
}

function isProPackage(a: Record<string, unknown>): boolean {
  if (isTrialOrFreeMonPackage(a)) return false;
  const code = typeof a.PackageCode === 'string' ? a.PackageCode : '';
  return code === CB_PACKAGE_CODE.proMon || code === CB_PACKAGE_CODE.proYear;
}

function aggregateCycleResources(list: Array<Record<string, unknown>>): Record<string, unknown> | null {
  if (list.length === 0) return null;
  const first = list[0];
  const totals = list.reduce(
    (acc: { total: number; remain: number }, item) => {
      acc.total += parseCycleTotal(item);
      acc.remain += parseCycleRemain(item);
      return acc;
    },
    { total: 0, remain: 0 },
  );
  return {
    ...first,
    CycleCapacitySizePrecise: String(totals.total),
    CycleCapacityRemainPrecise: String(totals.remain),
  };
}

function toOfficialQuotaResource(raw: Record<string, unknown>): CodebuddyOfficialQuotaResource {
  const packageCode = typeof raw.PackageCode === 'string' ? raw.PackageCode : null;
  const packageName = typeof raw.PackageName === 'string' ? raw.PackageName : null;
  const cycleStartTime = typeof raw.CycleStartTime === 'string' ? raw.CycleStartTime : null;
  const cycleEndTime = typeof raw.CycleEndTime === 'string' ? raw.CycleEndTime : null;
  const deductionEndTime = parseNumeric(raw.DeductionEndTime);
  const expiredTime = typeof raw.ExpiredTime === 'string' ? raw.ExpiredTime : null;

  const total = parseCycleTotal(raw);
  const remain = parseCycleRemain(raw);
  const used = Math.max(0, total - remain);
  const usedPercent = total > 0 ? Math.max(0, Math.min(100, (used / total) * 100)) : 0;
  const remainPercent = total > 0 ? Math.max(0, Math.min(100, (remain / total) * 100)) : null;

  const cycleEndAt = parseDateTimeToEpoch(cycleEndTime);
  const expireAt = deductionEndTime ?? parseDateTimeToEpoch(expiredTime) ?? cycleEndAt;
  const refreshAt =
    cycleEndAt != null && expireAt != null && cycleEndAt !== expireAt
      ? cycleEndAt + 1000
      : null;

  const isBasePackage =
    packageCode === CB_PACKAGE_CODE.free ||
    packageCode === CB_PACKAGE_CODE.freeMon;

  return {
    packageCode,
    packageName,
    cycleStartTime,
    cycleEndTime,
    deductionEndTime,
    expiredTime,
    total,
    remain,
    used,
    usedPercent,
    remainPercent,
    refreshAt,
    expireAt,
    isBasePackage,
  };
}

/**
 * Build quota model aligned with official CodeBuddy web user-center logic:
 * resources = [merge(gift+freeMon), ...pro, ...activity, merge(free)],
 * extra is aggregated from extra packages.
 */
export function getCodebuddyOfficialQuotaModel(account: CodebuddyAccount): CodebuddyOfficialQuotaModel {
  const updatedAt = getAccountQuotaUpdatedAtMs(account);
  const empty: CodebuddyOfficialQuotaResource = {
    packageCode: CB_PACKAGE_CODE.extra,
    packageName: null,
    cycleStartTime: null,
    cycleEndTime: null,
    deductionEndTime: null,
    expiredTime: null,
    total: 0,
    remain: 0,
    used: 0,
    usedPercent: 0,
    remainPercent: null,
    refreshAt: null,
    expireAt: null,
    isBasePackage: false,
  };

  const all = extractResourceAccounts(account).filter((a) => isActiveResource(a));
  if (all.length === 0) {
    return { resources: [], extra: empty, updatedAt };
  }

  const pro = all.filter((a) => isProPackage(a));
  const extras = all.filter((a) => isExtraPackage(a));
  const trialOrFreeMon = all.filter((a) => isTrialOrFreeMonPackage(a));
  const free = all.filter((a) => {
    const code = typeof a.PackageCode === 'string' ? a.PackageCode : '';
    return code === CB_PACKAGE_CODE.free;
  });
  const activity = all.filter((a) => {
    const code = typeof a.PackageCode === 'string' ? a.PackageCode : '';
    return code === CB_PACKAGE_CODE.activity;
  });

  const mergedTrialOrFreeMon = aggregateCycleResources(trialOrFreeMon);
  const mergedFree = aggregateCycleResources(free);
  const ordered = [mergedTrialOrFreeMon, ...pro, ...activity, mergedFree].filter(
    (item): item is Record<string, unknown> => item != null && !!item.PackageCode,
  );
  const resources = ordered.map((item) => toOfficialQuotaResource(item));

  const mergedExtra = aggregateCycleResources(extras);
  const extra = mergedExtra ? toOfficialQuotaResource(mergedExtra) : empty;
  return { resources, extra, updatedAt };
}

/**
 * Merge all active non-extra resource accounts to produce an aggregated summary,
 * following the official CodeBuddy web client's multi-package merge logic (`Yr`).
 * Extra credit packages are excluded (use `getCodebuddyExtraCreditSummary` instead).
 */
export function getCodebuddyResourceSummary(account: CodebuddyAccount): CodebuddyResourceSummary | null {
  const all = extractResourceAccounts(account);
  if (all.length === 0) return null;

  const active = all.filter((a) => isActiveResource(a) && !isExtraPackage(a));
  if (active.length === 0) return null;

  const codePriority: Record<string, number> = {
    [CB_PACKAGE_CODE.proYear]: 0,
    [CB_PACKAGE_CODE.proMon]: 1,
    [CB_PACKAGE_CODE.gift]: 2,
    [CB_PACKAGE_CODE.activity]: 3,
    [CB_PACKAGE_CODE.freeMon]: 4,
    [CB_PACKAGE_CODE.free]: 5,
  };
  const primaryPkg = [...active].sort((a, b) => {
    const ca = typeof a.PackageCode === 'string' ? a.PackageCode : '';
    const cb = typeof b.PackageCode === 'string' ? b.PackageCode : '';
    return (codePriority[ca] ?? 99) - (codePriority[cb] ?? 99);
  })[0];

  let totalAgg = 0;
  let remainAgg = 0;
  let usedAgg = 0;
  for (const a of active) {
    totalAgg += parseNumeric(a.CapacitySizePrecise) ?? parseNumeric(a.CapacitySize) ?? 0;
    remainAgg += parseNumeric(a.CapacityRemainPrecise) ?? parseNumeric(a.CapacityRemain) ?? 0;
    usedAgg += parseNumeric(a.CapacityUsedPrecise) ?? parseNumeric(a.CapacityUsed) ?? 0;
  }

  const total = totalAgg || null;
  const remain = remainAgg;
  const used = usedAgg;
  const remainPercent = total && total > 0 ? Math.max(0, Math.min(100, (remain / total) * 100)) : null;
  const boundUpdatedAt = getAccountQuotaUpdatedAtMs(account);

  return {
    packageName: typeof primaryPkg.PackageName === 'string' ? primaryPkg.PackageName : null,
    cycleStartTime: typeof primaryPkg.CycleStartTime === 'string' ? primaryPkg.CycleStartTime : null,
    cycleEndTime: typeof primaryPkg.CycleEndTime === 'string' ? primaryPkg.CycleEndTime : null,
    remain,
    used,
    total,
    remainPercent,
    boundUpdatedAt,
  };
}

export interface CodebuddyExtraCreditSummary {
  remain: number;
  total: number;
  used: number;
  usedPercent: number;
  remainPercent: number | null;
}

/**
 * Aggregate extra credit packages (PackageCode === extra) from resource accounts.
 * Always returns a result (defaults to 0/0) so the UI can always render the section,
 * matching the official CodeBuddy web client behaviour.
 */
export function getCodebuddyExtraCreditSummary(account: CodebuddyAccount): CodebuddyExtraCreditSummary {
  const all = extractResourceAccounts(account);
  const extras = all.filter((a) => isActiveResource(a) && isExtraPackage(a));

  let totalAgg = 0;
  let remainAgg = 0;
  for (const a of extras) {
    totalAgg += parseNumeric(a.CapacitySizePrecise) ?? parseNumeric(a.CapacitySize) ?? 0;
    remainAgg += parseNumeric(a.CapacityRemainPrecise) ?? parseNumeric(a.CapacityRemain) ?? 0;
  }

  const usedAgg = Math.max(0, totalAgg - remainAgg);
  const usedPercent = totalAgg > 0 ? Math.max(0, Math.min(100, (usedAgg / totalAgg) * 100)) : 0;
  const remainPercent = totalAgg > 0 ? Math.max(0, Math.min(100, (remainAgg / totalAgg) * 100)) : null;
  return { remain: remainAgg, total: totalAgg, used: usedAgg, usedPercent, remainPercent };
}
