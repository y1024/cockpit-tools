import { QuotaData } from '../types/account';

// 显示顺序与分组管理一致：Claude 4.5, Gemini Pro, Gemini Flash, Gemini Image
export const DISPLAY_MODEL_ORDER = [
  { ids: ['claude-sonnet-4-5-thinking', 'claude-sonnet-4-5', 'claude-opus-4-6-thinking', 'claude-opus-4-5-thinking'], label: 'Claude 4.5' },
  { ids: ['gemini-3-pro-high', 'gemini-3-pro-low'], label: 'Gemini Pro' },
  { ids: ['gemini-3-flash'], label: 'Gemini Flash' },
  { ids: ['gemini-3-pro-image'], label: 'Gemini Image' },
];

export function matchModelName(modelName: string, target: string): boolean {
  return modelName === target || modelName.startsWith(`${target}-`);
}

export function getSubscriptionTier(quota?: QuotaData): string {
  const tier = quota?.subscription_tier || 'FREE';
  // 映射等级名称
  if (tier.includes('PRO') || tier.includes('pro')) return 'PRO';
  if (tier.includes('ULTRA') || tier.includes('ultra')) return 'ULTRA';
  return 'FREE';
}

export function getSubscriptionTierDisplay(quota?: QuotaData): string {
  const rawTier = quota?.subscription_tier?.trim();
  if (rawTier) return rawTier;
  return getSubscriptionTier(quota);
}

export function getQuotaClass(percentage: number): string {
  if (percentage >= 70) return 'high';
  if (percentage >= 30) return 'medium';
  return 'low';
}

type Translate = (key: string, options?: Record<string, unknown>) => string;

export function formatResetTime(resetTime: string, t: Translate): string {
  if (!resetTime) return '';
  try {
    const reset = new Date(resetTime);
    if (Number.isNaN(reset.getTime())) return '';
    const now = new Date();
    const diffMs = reset.getTime() - now.getTime();
    if (diffMs <= 0) return t('common.shared.quota.resetDone');

    const totalMinutes = Math.floor(diffMs / (1000 * 60));
    const days = Math.floor(totalMinutes / (60 * 24));
    const hours = Math.floor((totalMinutes % (60 * 24)) / 60);
    const minutes = totalMinutes % 60;

    let parts = [];
    if (days > 0) parts.push(`${days}d`);
    if (hours > 0) parts.push(`${hours}h`);
    if (minutes > 0) parts.push(`${minutes}m`);
    
    // If less than 1 minute but positive, show 1m or <1m. Let's use 1m for simplicity or <1m
    if (parts.length === 0) return '<1m';
    
    return parts.join(' ');
  } catch {
    return '';
  }
}

export function formatResetTimeAbsolute(resetTime: string): string {
  if (!resetTime) return '';
  const reset = new Date(resetTime);
  if (Number.isNaN(reset.getTime())) return '';
  const pad = (value: number) => String(value).padStart(2, '0');
  const month = pad(reset.getMonth() + 1);
  const day = pad(reset.getDate());
  const hours = pad(reset.getHours());
  const minutes = pad(reset.getMinutes());
  return `${month}/${day} ${hours}:${minutes}`;
}

export function formatResetTimeDisplay(resetTime: string, t: Translate): string {
  const resetDone = t('common.shared.quota.resetDone');
  const relative = formatResetTime(resetTime, t);
  const absolute = formatResetTimeAbsolute(resetTime);
  if (!relative && !absolute) return '';
  if (relative === resetDone) return relative;
  // If we have both, return "relative (absolute)"
  // If only one, return that one
  if (relative && absolute) {
    return `${relative} (${absolute})`;
  }
  return relative || absolute;
}

export function getDisplayModels(quota?: QuotaData) {
  if (!quota?.models) {
    console.log('[getDisplayModels] quota 或 models 为空:', { quota });
    return [];
  }
  
  const normalized = quota.models.map((model) => ({
    model,
    nameLower: model.name.toLowerCase(),
  }));
  
  const pickModel = (ids: string[]) =>
    normalized.find((item) => ids.some((id) => matchModelName(item.nameLower, id)))?.model;
  
  const result = DISPLAY_MODEL_ORDER
    .map((item) => pickModel(item.ids))
    .filter((model): model is (typeof quota.models)[number] => Boolean(model));
  
  // 调试日志：显示匹配过程
  if (result.length === 0 && quota.models.length > 0) {
    console.log('[getDisplayModels] 有模型数据但匹配失败:', {
      availableModels: quota.models.map(m => m.name),
      expectedIds: DISPLAY_MODEL_ORDER.flatMap(item => item.ids),
    });
  }
  
  return result;
}

export function getModelShortName(name: string): string {
  const normalized = name.toLowerCase();
  for (const item of DISPLAY_MODEL_ORDER) {
    if (item.ids.some((id) => matchModelName(normalized, id))) {
      return item.label;
    }
  }
  return name;
}
