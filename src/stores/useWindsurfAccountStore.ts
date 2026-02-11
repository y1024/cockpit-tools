import { create } from 'zustand';
import {
  WindsurfAccount,
  getWindsurfAccountDisplayEmail,
  getWindsurfPlanBadge,
  getWindsurfUsage,
} from '../types/windsurf';
import * as windsurfService from '../services/windsurfService';

const WINDSURF_ACCOUNTS_CACHE_KEY = 'agtools.windsurf.accounts.cache';

const loadCachedAccounts = (): WindsurfAccount[] => {
  try {
    const raw = localStorage.getItem(WINDSURF_ACCOUNTS_CACHE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as WindsurfAccount[]) : [];
  } catch {
    return [];
  }
};

const persistAccountsCache = (accounts: WindsurfAccount[]) => {
  try {
    localStorage.setItem(WINDSURF_ACCOUNTS_CACHE_KEY, JSON.stringify(accounts));
  } catch {
    // ignore
  }
};

interface WindsurfAccountState {
  accounts: WindsurfAccount[];
  loading: boolean;
  error: string | null;

  fetchAccounts: () => Promise<void>;
  switchAccount: (accountId: string) => Promise<void>;
  deleteAccounts: (accountIds: string[]) => Promise<void>;
  refreshToken: (accountId: string) => Promise<void>;
  refreshAllTokens: () => Promise<void>;
  importFromJson: (jsonContent: string) => Promise<WindsurfAccount[]>;
  exportAccounts: (accountIds: string[]) => Promise<string>;
  updateAccountTags: (accountId: string, tags: string[]) => Promise<WindsurfAccount>;
}

export const useWindsurfAccountStore = create<WindsurfAccountState>((set, get) => ({
  accounts: loadCachedAccounts(),
  loading: false,
  error: null,

  fetchAccounts: async () => {
    set({ loading: true, error: null });
    try {
      const accounts = await windsurfService.listWindsurfAccounts();
      // 兼容：为复用 Codex 的 UI/交互，补齐 email/plan_type/quota 等派生字段
      const mapped = accounts.map((acc) => {
        const email = getWindsurfAccountDisplayEmail(acc);
        const usage = getWindsurfUsage(acc);
        const hourlyPct = usage.inlineSuggestionsUsedPercent ?? usage.chatMessagesUsedPercent;
        const weeklyPct = usage.chatMessagesUsedPercent ?? usage.inlineSuggestionsUsedPercent;
        const quota =
          hourlyPct == null && weeklyPct == null
            ? undefined
            : {
                hourly_percentage: hourlyPct ?? 0,
                weekly_percentage: weeklyPct ?? 0,
                hourly_reset_time: usage.allowanceResetAt ?? null,
                weekly_reset_time: usage.allowanceResetAt ?? null,
                raw_data: {
                  remainingCompletions: usage.remainingCompletions,
                  remainingChat: usage.remainingChat,
                  totalCompletions: usage.totalCompletions,
                  totalChat: usage.totalChat,
                },
              };
        return {
          ...acc,
          email,
          plan_type: getWindsurfPlanBadge(acc),
          quota,
        } as WindsurfAccount;
      });
      set({ accounts: mapped, loading: false });
      persistAccountsCache(mapped);
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  deleteAccounts: async (accountIds: string[]) => {
    if (accountIds.length === 0) return;
    if (accountIds.length === 1) {
      await windsurfService.deleteWindsurfAccount(accountIds[0]);
    } else {
      await windsurfService.deleteWindsurfAccounts(accountIds);
    }
    await get().fetchAccounts();
  },

  switchAccount: async (accountId: string) => {
    await windsurfService.injectWindsurfToVSCode(accountId);
    await get().fetchAccounts();
  },

  refreshToken: async (accountId: string) => {
    await windsurfService.refreshWindsurfToken(accountId);
    await get().fetchAccounts();
  },

  refreshAllTokens: async () => {
    await windsurfService.refreshAllWindsurfTokens();
    await get().fetchAccounts();
  },

  importFromJson: async (jsonContent: string) => {
    const accounts = await windsurfService.importWindsurfFromJson(jsonContent);
    await get().fetchAccounts();
    return accounts;
  },

  exportAccounts: async (accountIds: string[]) => {
    return await windsurfService.exportWindsurfAccounts(accountIds);
  },

  updateAccountTags: async (accountId: string, tags: string[]) => {
    const account = await windsurfService.updateWindsurfAccountTags(accountId, tags);
    await get().fetchAccounts();
    return account;
  },
}));
