import React, { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useAccountStore } from '../stores/useAccountStore';
import { useCodexAccountStore } from '../stores/useCodexAccountStore';
import { useGitHubCopilotAccountStore } from '../stores/useGitHubCopilotAccountStore';
import { useWindsurfAccountStore } from '../stores/useWindsurfAccountStore';
import { useKiroAccountStore } from '../stores/useKiroAccountStore';
import { useCursorAccountStore } from '../stores/useCursorAccountStore';
import { useGeminiAccountStore } from '../stores/useGeminiAccountStore';
import { useCodebuddyAccountStore } from '../stores/useCodebuddyAccountStore';
import { useCodebuddyCnAccountStore } from '../stores/useCodebuddyCnAccountStore';
import { useQoderAccountStore } from '../stores/useQoderAccountStore';
import { useTraeAccountStore } from '../stores/useTraeAccountStore';
import { usePlatformLayoutStore } from '../stores/usePlatformLayoutStore';
import { Page } from '../types/navigation';
import { Users, CheckCircle2, Sparkles, RotateCw, Play, Github, HelpCircle } from 'lucide-react';
import { Account } from '../types/account';
import {
  CodebuddyAccount,
  getCodebuddyAccountDisplayEmail,
  getCodebuddyPlanBadge,
  getCodebuddyPlanBadgeClass,
  getCodebuddyResourceSummary,
  getCodebuddyExtraCreditSummary,
  getCodebuddyUsage,
} from '../types/codebuddy';
import {
  QoderAccount,
  getQoderAccountDisplayEmail,
  getQoderPlanBadge,
  getQoderSubscriptionInfo,
} from '../types/qoder';
import {
  TraeAccount,
  getTraeAccountDisplayEmail,
  getTraePlanBadge,
  getTraePlanBadgeClass,
  getTraeUsage,
} from '../types/trae';
import { CodexAccount } from '../types/codex';
import { GitHubCopilotAccount } from '../types/githubCopilot';
import {
  WindsurfAccount,
  getWindsurfCreditsSummary,
} from '../types/windsurf';
import {
  KiroAccount,
  getKiroCreditsSummary,
  isKiroAccountBanned,
} from '../types/kiro';
import { CursorAccount, getCursorUsage } from '../types/cursor';
import {
  GeminiAccount,
  getGeminiPlanBadgeClass,
  getGeminiPlanDisplayName,
  getGeminiTierQuotaSummary,
} from '../types/gemini';
import './DashboardPage.css';
import { AnnouncementCenter } from '../components/AnnouncementCenter';
import { RobotIcon } from '../components/icons/RobotIcon';
import { CodexIcon } from '../components/icons/CodexIcon';
import { WindsurfIcon } from '../components/icons/WindsurfIcon';
import { KiroIcon } from '../components/icons/KiroIcon';
import { CursorIcon } from '../components/icons/CursorIcon';
import { GeminiIcon } from '../components/icons/GeminiIcon';
import { CodebuddyIcon } from '../components/icons/CodebuddyIcon';
import { QoderIcon } from '../components/icons/QoderIcon';
import { TraeIcon } from '../components/icons/TraeIcon';
import { PlatformId, PLATFORM_PAGE_MAP } from '../types/platform';
import { getPlatformLabel, renderPlatformIcon } from '../utils/platformMeta';
import { isPrivacyModeEnabledByDefault, maskSensitiveValue } from '../utils/privacy';
import { DisplayGroup, getDisplayGroups } from '../services/groupService';
import {
  buildAntigravityAccountPresentation,
  buildCodexAccountPresentation,
  buildGitHubCopilotAccountPresentation,
  buildCursorAccountPresentation,
  buildKiroAccountPresentation,
  buildWindsurfAccountPresentation,
} from '../presentation/platformAccountPresentation';

interface DashboardPageProps {
  onNavigate: (page: Page) => void;
  onOpenPlatformLayout: () => void;
  onEasterEggTriggerClick: () => void;
}

const GHCP_CURRENT_ACCOUNT_ID_KEY = 'agtools.github_copilot.current_account_id';
const WINDSURF_CURRENT_ACCOUNT_ID_KEY = 'agtools.windsurf.current_account_id';
const KIRO_CURRENT_ACCOUNT_ID_KEY = 'agtools.kiro.current_account_id';
const CURSOR_CURRENT_ACCOUNT_ID_KEY = 'agtools.cursor.current_account_id';
const GEMINI_CURRENT_ACCOUNT_ID_KEY = 'agtools.gemini.current_account_id';
const CODEBUDDY_CURRENT_ACCOUNT_ID_KEY = 'agtools.codebuddy.current_account_id';
const CODEBUDDY_CN_CURRENT_ACCOUNT_ID_KEY = 'agtools.codebuddycn.current_account_id';
const QODER_CURRENT_ACCOUNT_ID_KEY = 'agtools.qoder.current_account_id';
const TRAE_CURRENT_ACCOUNT_ID_KEY = 'agtools.trae.current_account_id';
const DASHBOARD_DEFERRED_PREFETCH_DELAY_MS = 1200;
let dashboardStartupPrefetched = false;

function toFiniteNumber(value: number | null | undefined): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

export function DashboardPage({ onNavigate, onOpenPlatformLayout, onEasterEggTriggerClick }: DashboardPageProps) {
  const { t, i18n } = useTranslation();
  const { orderedPlatformIds, hiddenPlatformIds } = usePlatformLayoutStore();
  const visiblePlatformOrder = useMemo(
    () => orderedPlatformIds.filter((platformId) => !hiddenPlatformIds.includes(platformId)),
    [orderedPlatformIds, hiddenPlatformIds],
  );
  const [privacyModeEnabled, setPrivacyModeEnabled] = React.useState<boolean>(() =>
    isPrivacyModeEnabledByDefault()
  );
  const maskAccountText = React.useCallback(
    (value?: string | null) => maskSensitiveValue(value, privacyModeEnabled),
    [privacyModeEnabled],
  );
  const [agDisplayGroups, setAgDisplayGroups] = React.useState<DisplayGroup[]>([]);

  React.useEffect(() => {
    const syncPrivacyMode = () => {
      setPrivacyModeEnabled(isPrivacyModeEnabledByDefault());
    };

    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible') {
        syncPrivacyMode();
      }
    };

    window.addEventListener('focus', syncPrivacyMode);
    window.addEventListener('storage', syncPrivacyMode);
    document.addEventListener('visibilitychange', handleVisibilityChange);
    return () => {
      window.removeEventListener('focus', syncPrivacyMode);
      window.removeEventListener('storage', syncPrivacyMode);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, []);

  
  // Antigravity Data
  const { 
    accounts: agAccounts, 
    currentAccount: agCurrent,
    switchAccount: switchAgAccount,
    fetchAccounts: fetchAgAccounts,
    fetchCurrentAccount: fetchAgCurrent
  } = useAccountStore();

  // Codex Data
  const { 
    accounts: codexAccounts, 
    currentAccount: codexCurrent,
    switchAccount: switchCodexAccount,
    fetchAccounts: fetchCodexAccounts,
    fetchCurrentAccount: fetchCodexCurrent
  } = useCodexAccountStore();

  // GitHub Copilot Data
  const {
    accounts: githubCopilotAccounts,
    fetchAccounts: fetchGitHubCopilotAccounts,
    switchAccount: switchGitHubCopilotAccount,
  } = useGitHubCopilotAccountStore();

  // Windsurf Data
  const {
    accounts: windsurfAccounts,
    fetchAccounts: fetchWindsurfAccounts,
    switchAccount: switchWindsurfAccount,
  } = useWindsurfAccountStore();

  // Kiro Data
  const {
    accounts: kiroAccounts,
    fetchAccounts: fetchKiroAccounts,
    switchAccount: switchKiroAccount,
  } = useKiroAccountStore();

  // Cursor Data
  const {
    accounts: cursorAccounts,
    fetchAccounts: fetchCursorAccounts,
    switchAccount: switchCursorAccount,
  } = useCursorAccountStore();

  // Gemini Data
  const {
    accounts: geminiAccounts,
    fetchAccounts: fetchGeminiAccounts,
    switchAccount: switchGeminiAccount,
  } = useGeminiAccountStore();

  const {
    accounts: codebuddyAccounts,
    fetchAccounts: fetchCodebuddyAccounts,
    switchAccount: switchCodebuddyAccount,
  } = useCodebuddyAccountStore();

  const {
    accounts: codebuddyCnAccounts,
    fetchAccounts: fetchCodebuddyCnAccounts,
    switchAccount: switchCodebuddyCnAccount,
  } = useCodebuddyCnAccountStore();

  const {
    accounts: qoderAccounts,
    fetchAccounts: fetchQoderAccounts,
    switchAccount: switchQoderAccount,
  } = useQoderAccountStore();

  const {
    accounts: traeAccounts,
    fetchAccounts: fetchTraeAccounts,
    switchAccount: switchTraeAccount,
  } = useTraeAccountStore();

  const agCurrentId = agCurrent?.id;
  const codexCurrentId = codexCurrent?.id;

  const agCurrentAccount = useMemo(() => {
    if (!agCurrentId) return null;
    return agAccounts.find((account) => account.id === agCurrentId) ?? agCurrent ?? null;
  }, [agAccounts, agCurrent, agCurrentId]);

  const codexCurrentAccount = useMemo(() => {
    if (!codexCurrentId) return null;
    return codexAccounts.find((account) => account.id === codexCurrentId) ?? codexCurrent ?? null;
  }, [codexAccounts, codexCurrent, codexCurrentId]);

  React.useEffect(() => {
    let disposed = false;
    let deferredTimer: number | null = null;

    const loadDisplayGroups = () => {
      getDisplayGroups()
        .then((groups) => {
          if (!disposed) {
            setAgDisplayGroups(groups);
          }
        })
        .catch((error) => {
          console.error('Failed to load display groups:', error);
        });
    };

    // 首屏优先：先拉 Antigravity 数据，其它平台延后，避免启动期并发请求过多。
    void Promise.allSettled([fetchAgAccounts(), fetchAgCurrent()]);
    loadDisplayGroups();

    const loadDeferredPlatforms = () => {
      if (disposed) {
        return;
      }
      void Promise.allSettled([
        fetchCodexAccounts(),
        fetchCodexCurrent(),
        fetchGitHubCopilotAccounts(),
        fetchWindsurfAccounts(),
        fetchKiroAccounts(),
        fetchCursorAccounts(),
        fetchGeminiAccounts(),
        fetchCodebuddyAccounts(),
        fetchCodebuddyCnAccounts(),
        fetchQoderAccounts(),
        fetchTraeAccounts(),
      ]);
    };

    if (!dashboardStartupPrefetched) {
      dashboardStartupPrefetched = true;
      deferredTimer = window.setTimeout(loadDeferredPlatforms, DASHBOARD_DEFERRED_PREFETCH_DELAY_MS);
    } else {
      loadDeferredPlatforms();
    }

    return () => {
      disposed = true;
      if (deferredTimer !== null) {
        window.clearTimeout(deferredTimer);
      }
    };
  }, []);

  // Statistics
  const stats = useMemo(() => {
    return {
      total:
        agAccounts.length +
        codexAccounts.length +
        githubCopilotAccounts.length +
        windsurfAccounts.length +
        kiroAccounts.length +
        cursorAccounts.length +
        geminiAccounts.length +
        codebuddyAccounts.length +
        codebuddyCnAccounts.length +
        qoderAccounts.length +
        traeAccounts.length,
      antigravity: agAccounts.length,
      codex: codexAccounts.length,
      githubCopilot: githubCopilotAccounts.length,
      windsurf: windsurfAccounts.length,
      kiro: kiroAccounts.length,
      cursor: cursorAccounts.length,
      gemini: geminiAccounts.length,
      codebuddy: codebuddyAccounts.length,
      codebuddy_cn: codebuddyCnAccounts.length,
      qoder: qoderAccounts.length,
      trae: traeAccounts.length,
    };
  }, [agAccounts, codexAccounts, githubCopilotAccounts, windsurfAccounts, kiroAccounts, cursorAccounts, geminiAccounts, codebuddyAccounts, codebuddyCnAccounts, qoderAccounts, traeAccounts]);

  // Refresh States
  const [refreshing, setRefreshing] = React.useState<Set<string>>(new Set());
  const [switching, setSwitching] = React.useState<Set<string>>(new Set());
  const [githubCopilotCurrentId, setGitHubCopilotCurrentId] = React.useState<string | null>(() => {
    try {
      return localStorage.getItem(GHCP_CURRENT_ACCOUNT_ID_KEY);
    } catch {
      return null;
    }
  });
  const [windsurfCurrentId, setWindsurfCurrentId] = React.useState<string | null>(() => {
    try {
      return localStorage.getItem(WINDSURF_CURRENT_ACCOUNT_ID_KEY);
    } catch {
      return null;
    }
  });
  const [kiroCurrentId, setKiroCurrentId] = React.useState<string | null>(() => {
    try {
      return localStorage.getItem(KIRO_CURRENT_ACCOUNT_ID_KEY);
    } catch {
      return null;
    }
  });
  const [cursorCurrentId, setCursorCurrentId] = React.useState<string | null>(() => {
    try {
      return localStorage.getItem(CURSOR_CURRENT_ACCOUNT_ID_KEY);
    } catch {
      return null;
    }
  });
  const [geminiCurrentId, setGeminiCurrentId] = React.useState<string | null>(() => {
    try {
      return localStorage.getItem(GEMINI_CURRENT_ACCOUNT_ID_KEY);
    } catch {
      return null;
    }
  });
  const [codebuddyCurrentId, setCodebuddyCurrentId] = React.useState<string | null>(() => {
    try {
      return localStorage.getItem(CODEBUDDY_CURRENT_ACCOUNT_ID_KEY);
    } catch {
      return null;
    }
  });
  const [codebuddyCnCurrentId, setCodebuddyCnCurrentId] = React.useState<string | null>(() => {
    try {
      return localStorage.getItem(CODEBUDDY_CN_CURRENT_ACCOUNT_ID_KEY);
    } catch {
      return null;
    }
  });
  const [qoderCurrentId, setQoderCurrentId] = React.useState<string | null>(() => {
    try {
      return localStorage.getItem(QODER_CURRENT_ACCOUNT_ID_KEY);
    } catch {
      return null;
    }
  });
  const [traeCurrentId, setTraeCurrentId] = React.useState<string | null>(() => {
    try {
      return localStorage.getItem(TRAE_CURRENT_ACCOUNT_ID_KEY);
    } catch {
      return null;
    }
  });
  const [cardRefreshing, setCardRefreshing] = React.useState<{
    ag: boolean;
    codex: boolean;
    githubCopilot: boolean;
    windsurf: boolean;
    kiro: boolean;
    cursor: boolean;
    gemini: boolean;
    codebuddy: boolean;
    codebuddyCn: boolean;
    qoder: boolean;
    trae: boolean;
  }>({
    ag: false,
    codex: false,
    githubCopilot: false,
    windsurf: false,
    kiro: false,
    cursor: false,
    gemini: false,
    codebuddy: false,
    codebuddyCn: false,
    qoder: false,
    trae: false,
  });

  // Refresh Handlers
  const handleRefreshAg = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing(prev => new Set(prev).add(accountId));
    try {
      await useAccountStore.getState().refreshQuota(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing(prev => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshCodex = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing(prev => new Set(prev).add(accountId));
    try {
      await useCodexAccountStore.getState().refreshQuota(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing(prev => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshGitHubCopilot = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing(prev => new Set(prev).add(accountId));
    try {
      await useGitHubCopilotAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing(prev => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshWindsurf = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useWindsurfAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshKiro = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useKiroAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshCursor = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useCursorAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshGemini = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useGeminiAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshAgCard = async () => {
    if (cardRefreshing.ag) return;
    setCardRefreshing(prev => ({ ...prev, ag: true }));
    const idsToRefresh = [agCurrentId, agRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useAccountStore.getState().refreshQuota(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing(prev => ({ ...prev, ag: false }));
    }
  };

  const handleRefreshCodexCard = async () => {
    if (cardRefreshing.codex) return;
    setCardRefreshing(prev => ({ ...prev, codex: true }));
    const idsToRefresh = [codexCurrentId, codexRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useCodexAccountStore.getState().refreshQuota(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing(prev => ({ ...prev, codex: false }));
    }
  };

  const handleRefreshGitHubCopilotCard = async () => {
    if (cardRefreshing.githubCopilot) return;
    setCardRefreshing(prev => ({ ...prev, githubCopilot: true }));
    const idsToRefresh = [githubCopilotCurrent?.id, githubCopilotRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useGitHubCopilotAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing(prev => ({ ...prev, githubCopilot: false }));
    }
  };

  const handleRefreshWindsurfCard = async () => {
    if (cardRefreshing.windsurf) return;
    setCardRefreshing((prev) => ({ ...prev, windsurf: true }));
    const idsToRefresh = [windsurfCurrent?.id, windsurfRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useWindsurfAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, windsurf: false }));
    }
  };

  const handleRefreshKiroCard = async () => {
    if (cardRefreshing.kiro) return;
    setCardRefreshing((prev) => ({ ...prev, kiro: true }));
    const idsToRefresh = [kiroCurrent?.id, kiroRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useKiroAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, kiro: false }));
    }
  };

  const handleRefreshCursorCard = async () => {
    if (cardRefreshing.cursor) return;
    setCardRefreshing((prev) => ({ ...prev, cursor: true }));
    const idsToRefresh = [cursorCurrent?.id, cursorRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useCursorAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, cursor: false }));
    }
  };

  const handleRefreshGeminiCard = async () => {
    if (cardRefreshing.gemini) return;
    setCardRefreshing((prev) => ({ ...prev, gemini: true }));
    const idsToRefresh = [geminiCurrent?.id, geminiRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useGeminiAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, gemini: false }));
    }
  };

  const handleSwitchGitHubCopilot = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchGitHubCopilotAccount(accountId);
      setGitHubCopilotCurrentId(accountId);
      localStorage.setItem(GHCP_CURRENT_ACCOUNT_ID_KEY, accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchWindsurf = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchWindsurfAccount(accountId);
      setWindsurfCurrentId(accountId);
      localStorage.setItem(WINDSURF_CURRENT_ACCOUNT_ID_KEY, accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchKiro = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchKiroAccount(accountId);
      setKiroCurrentId(accountId);
      localStorage.setItem(KIRO_CURRENT_ACCOUNT_ID_KEY, accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchCursor = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchCursorAccount(accountId);
      setCursorCurrentId(accountId);
      localStorage.setItem(CURSOR_CURRENT_ACCOUNT_ID_KEY, accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchGemini = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchGeminiAccount(accountId);
      setGeminiCurrentId(accountId);
      localStorage.setItem(GEMINI_CURRENT_ACCOUNT_ID_KEY, accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshCodebuddy = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useCodebuddyAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshCodebuddyCn = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useCodebuddyCnAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshQoder = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useQoderAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshTrae = async (accountId: string) => {
    if (refreshing.has(accountId)) return;
    setRefreshing((prev) => new Set(prev).add(accountId));
    try {
      await useTraeAccountStore.getState().refreshToken(accountId);
    } catch (error) {
      console.error('Refresh failed:', error);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleRefreshCodebuddyCard = async () => {
    if (cardRefreshing.codebuddy) return;
    setCardRefreshing((prev) => ({ ...prev, codebuddy: true }));
    const idsToRefresh = [codebuddyCurrent?.id, codebuddyRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useCodebuddyAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, codebuddy: false }));
    }
  };

  const handleRefreshCodebuddyCnCard = async () => {
    if (cardRefreshing.codebuddyCn) return;
    setCardRefreshing((prev) => ({ ...prev, codebuddyCn: true }));
    const idsToRefresh = [codebuddyCnCurrent?.id, codebuddyCnRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useCodebuddyCnAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, codebuddyCn: false }));
    }
  };

  const handleRefreshQoderCard = async () => {
    if (cardRefreshing.qoder) return;
    setCardRefreshing((prev) => ({ ...prev, qoder: true }));
    const idsToRefresh = [qoderCurrent?.id, qoderRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useQoderAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, qoder: false }));
    }
  };

  const handleRefreshTraeCard = async () => {
    if (cardRefreshing.trae) return;
    setCardRefreshing((prev) => ({ ...prev, trae: true }));
    const idsToRefresh = [traeCurrent?.id, traeRecommended?.id].filter(Boolean) as string[];
    try {
      for (const id of idsToRefresh) {
        await useTraeAccountStore.getState().refreshToken(id);
      }
    } catch (error) {
      console.error('Card refresh failed:', error);
    } finally {
      setCardRefreshing((prev) => ({ ...prev, trae: false }));
    }
  };

  const handleSwitchCodebuddy = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchCodebuddyAccount(accountId);
      setCodebuddyCurrentId(accountId);
      localStorage.setItem(CODEBUDDY_CURRENT_ACCOUNT_ID_KEY, accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchCodebuddyCn = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchCodebuddyCnAccount(accountId);
      setCodebuddyCnCurrentId(accountId);
      localStorage.setItem(CODEBUDDY_CN_CURRENT_ACCOUNT_ID_KEY, accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchQoder = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchQoderAccount(accountId);
      setQoderCurrentId(accountId);
      localStorage.setItem(QODER_CURRENT_ACCOUNT_ID_KEY, accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  const handleSwitchTrae = async (accountId: string) => {
    if (switching.has(accountId)) return;
    setSwitching((prev) => new Set(prev).add(accountId));
    try {
      await switchTraeAccount(accountId);
      setTraeCurrentId(accountId);
      localStorage.setItem(TRAE_CURRENT_ACCOUNT_ID_KEY, accountId);
    } catch (error) {
      console.error('Switch failed:', error);
    } finally {
      setSwitching((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  };

  // Antigravity Recommendation Logic
  const agRecommended = useMemo(() => {
    if (agAccounts.length <= 1) return null;
    
    // Simple logic: find account with highest overall quota that isn't current
    const others = agAccounts.filter((a) => {
      if (a.id === agCurrentId) return false;
      if (a.disabled) return false;
      if (a.quota?.is_forbidden) return false;
      if (!a.quota?.models || a.quota.models.length === 0) return false;
      return true;
    });
    if (others.length === 0) return null;

    return others.reduce((prev, curr) => {
      // Calculate a score based on quotas
      const getScore = (acc: Account) => {
        if (!acc.quota?.models) return -1;
        // Average percentage of all models
        const total = acc.quota.models.reduce((sum, m) => sum + m.percentage, 0);
        return total / acc.quota.models.length;
      };
      
      return getScore(curr) > getScore(prev) ? curr : prev;
    });
  }, [agAccounts, agCurrentId]);

  // Codex Recommendation Logic
  const codexRecommended = useMemo(() => {
    if (codexAccounts.length <= 1) return null;

    const others = codexAccounts.filter((a) => {
      if (a.id === codexCurrentId) return false;
      if (!a.quota) return false;
      return true;
    });
    if (others.length === 0) return null;

    return others.reduce((prev, curr) => {
      const getScore = (acc: CodexAccount) => {
        if (!acc.quota) return -1;
        return (acc.quota.hourly_percentage + acc.quota.weekly_percentage) / 2;
      };
      return getScore(curr) > getScore(prev) ? curr : prev;
    });
  }, [codexAccounts, codexCurrentId]);

  const githubCopilotCurrent = useMemo(() => {
    if (githubCopilotAccounts.length === 0) return null;
    if (githubCopilotCurrentId) {
      const current = githubCopilotAccounts.find((account) => account.id === githubCopilotCurrentId);
      if (current) return current;
    }
    return githubCopilotAccounts.reduce((prev, curr) => {
      const prevScore = prev.last_used || prev.created_at || 0;
      const currScore = curr.last_used || curr.created_at || 0;
      return currScore > prevScore ? curr : prev;
    });
  }, [githubCopilotAccounts, githubCopilotCurrentId]);

  const windsurfCurrent = useMemo(() => {
    if (windsurfAccounts.length === 0) return null;
    if (windsurfCurrentId) {
      const current = windsurfAccounts.find((account) => account.id === windsurfCurrentId);
      if (current) return current;
    }
    return windsurfAccounts.reduce((prev, curr) => {
      const prevScore = prev.last_used || prev.created_at || 0;
      const currScore = curr.last_used || curr.created_at || 0;
      return currScore > prevScore ? curr : prev;
    });
  }, [windsurfAccounts, windsurfCurrentId]);

  const kiroCurrent = useMemo(() => {
    if (kiroAccounts.length === 0) return null;
    if (kiroCurrentId) {
      const current = kiroAccounts.find((account) => account.id === kiroCurrentId);
      if (current) return current;
    }
    return kiroAccounts.reduce((prev, curr) => {
      const prevScore = prev.last_used || prev.created_at || 0;
      const currScore = curr.last_used || curr.created_at || 0;
      return currScore > prevScore ? curr : prev;
    });
  }, [kiroAccounts, kiroCurrentId]);

  const cursorCurrent = useMemo(() => {
    if (cursorAccounts.length === 0) return null;
    if (cursorCurrentId) {
      const current = cursorAccounts.find((account) => account.id === cursorCurrentId);
      if (current) return current;
    }
    return cursorAccounts.reduce((prev, curr) => {
      const prevScore = prev.last_used || prev.created_at || 0;
      const currScore = curr.last_used || curr.created_at || 0;
      return currScore > prevScore ? curr : prev;
    });
  }, [cursorAccounts, cursorCurrentId]);

  const geminiCurrent = useMemo(() => {
    if (geminiAccounts.length === 0) return null;
    if (geminiCurrentId) {
      const current = geminiAccounts.find((account) => account.id === geminiCurrentId);
      if (current) return current;
    }
    return geminiAccounts.reduce((prev, curr) => {
      const prevScore = prev.last_used || prev.created_at || 0;
      const currScore = curr.last_used || curr.created_at || 0;
      return currScore > prevScore ? curr : prev;
    });
  }, [geminiAccounts, geminiCurrentId]);

  React.useEffect(() => {
    if (!cursorCurrentId) return;
    const exists = cursorAccounts.some((account) => account.id === cursorCurrentId);
    if (exists) return;
    setCursorCurrentId(null);
    localStorage.removeItem(CURSOR_CURRENT_ACCOUNT_ID_KEY);
  }, [cursorAccounts, cursorCurrentId]);

  React.useEffect(() => {
    if (!geminiCurrentId) return;
    const exists = geminiAccounts.some((account) => account.id === geminiCurrentId);
    if (exists) return;
    setGeminiCurrentId(null);
    localStorage.removeItem(GEMINI_CURRENT_ACCOUNT_ID_KEY);
  }, [geminiAccounts, geminiCurrentId]);

  React.useEffect(() => {
    if (!githubCopilotCurrentId) return;
    const exists = githubCopilotAccounts.some((account) => account.id === githubCopilotCurrentId);
    if (exists) return;
    setGitHubCopilotCurrentId(null);
    localStorage.removeItem(GHCP_CURRENT_ACCOUNT_ID_KEY);
  }, [githubCopilotAccounts, githubCopilotCurrentId]);

  React.useEffect(() => {
    if (!windsurfCurrentId) return;
    const exists = windsurfAccounts.some((account) => account.id === windsurfCurrentId);
    if (exists) return;
    setWindsurfCurrentId(null);
    localStorage.removeItem(WINDSURF_CURRENT_ACCOUNT_ID_KEY);
  }, [windsurfAccounts, windsurfCurrentId]);

  React.useEffect(() => {
    if (!kiroCurrentId) return;
    const exists = kiroAccounts.some((account) => account.id === kiroCurrentId);
    if (exists) return;
    setKiroCurrentId(null);
    localStorage.removeItem(KIRO_CURRENT_ACCOUNT_ID_KEY);
  }, [kiroAccounts, kiroCurrentId]);

  const codebuddyCurrent = useMemo(() => {
    if (codebuddyAccounts.length === 0) return null;
    if (codebuddyCurrentId) {
      const current = codebuddyAccounts.find((account) => account.id === codebuddyCurrentId);
      if (current) return current;
    }
    return codebuddyAccounts.reduce((prev, curr) => {
      const prevScore = prev.last_used || prev.created_at || 0;
      const currScore = curr.last_used || curr.created_at || 0;
      return currScore > prevScore ? curr : prev;
    });
  }, [codebuddyAccounts, codebuddyCurrentId]);

  const codebuddyCnCurrent = useMemo(() => {
    if (codebuddyCnAccounts.length === 0) return null;
    if (codebuddyCnCurrentId) {
      const current = codebuddyCnAccounts.find((account) => account.id === codebuddyCnCurrentId);
      if (current) return current;
    }
    return codebuddyCnAccounts.reduce((prev, curr) => {
      const prevScore = prev.last_used || prev.created_at || 0;
      const currScore = curr.last_used || curr.created_at || 0;
      return currScore > prevScore ? curr : prev;
    });
  }, [codebuddyCnAccounts, codebuddyCnCurrentId]);

  const qoderCurrent = useMemo(() => {
    if (qoderAccounts.length === 0) return null;
    if (qoderCurrentId) {
      const current = qoderAccounts.find((account) => account.id === qoderCurrentId);
      if (current) return current;
    }
    return qoderAccounts.reduce((prev, curr) => {
      const prevScore = prev.last_used || prev.created_at || 0;
      const currScore = curr.last_used || curr.created_at || 0;
      return currScore > prevScore ? curr : prev;
    });
  }, [qoderAccounts, qoderCurrentId]);

  const traeCurrent = useMemo(() => {
    if (traeAccounts.length === 0) return null;
    if (traeCurrentId) {
      const current = traeAccounts.find((account) => account.id === traeCurrentId);
      if (current) return current;
    }
    return traeAccounts.reduce((prev, curr) => {
      const prevScore = prev.last_used || prev.created_at || 0;
      const currScore = curr.last_used || curr.created_at || 0;
      return currScore > prevScore ? curr : prev;
    });
  }, [traeAccounts, traeCurrentId]);

  React.useEffect(() => {
    if (!codebuddyCurrentId) return;
    const exists = codebuddyAccounts.some((account) => account.id === codebuddyCurrentId);
    if (exists) return;
    setCodebuddyCurrentId(null);
    localStorage.removeItem(CODEBUDDY_CURRENT_ACCOUNT_ID_KEY);
  }, [codebuddyAccounts, codebuddyCurrentId]);

  React.useEffect(() => {
    if (!codebuddyCnCurrentId) return;
    const exists = codebuddyCnAccounts.some((account) => account.id === codebuddyCnCurrentId);
    if (exists) return;
    setCodebuddyCnCurrentId(null);
    localStorage.removeItem(CODEBUDDY_CN_CURRENT_ACCOUNT_ID_KEY);
  }, [codebuddyCnAccounts, codebuddyCnCurrentId]);

  React.useEffect(() => {
    if (!qoderCurrentId) return;
    const exists = qoderAccounts.some((account) => account.id === qoderCurrentId);
    if (exists) return;
    setQoderCurrentId(null);
    localStorage.removeItem(QODER_CURRENT_ACCOUNT_ID_KEY);
  }, [qoderAccounts, qoderCurrentId]);

  React.useEffect(() => {
    if (!traeCurrentId) return;
    const exists = traeAccounts.some((account) => account.id === traeCurrentId);
    if (exists) return;
    setTraeCurrentId(null);
    localStorage.removeItem(TRAE_CURRENT_ACCOUNT_ID_KEY);
  }, [traeAccounts, traeCurrentId]);

  const githubCopilotRecommended = useMemo(() => {
    if (githubCopilotAccounts.length <= 1) return null;
    const currentId = githubCopilotCurrent?.id;
    const others = githubCopilotAccounts.filter((a) => a.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (acc: GitHubCopilotAccount) => {
      const scores = [acc.quota?.hourly_percentage, acc.quota?.weekly_percentage].filter(
        (value): value is number => typeof value === 'number',
      );
      if (scores.length === 0) return 101;
      return scores.reduce((sum, value) => sum + value, 0) / scores.length;
    };

    return others.reduce((prev, curr) => (getScore(curr) < getScore(prev) ? curr : prev));
  }, [githubCopilotAccounts, githubCopilotCurrent?.id]);

  const windsurfRecommended = useMemo(() => {
    if (windsurfAccounts.length <= 1) return null;
    const currentId = windsurfCurrent?.id;
    const others = windsurfAccounts.filter((account) => account.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (account: WindsurfAccount) => {
      const credits = getWindsurfCreditsSummary(account);
      const promptLeft = toFiniteNumber(credits.promptCreditsLeft);
      const addOnLeft = toFiniteNumber(credits.addOnCredits);

      if (promptLeft != null) {
        return promptLeft * 1000 + (addOnLeft ?? 0);
      }

      const quotaValues = [account.quota?.hourly_percentage, account.quota?.weekly_percentage].filter(
        (value): value is number => typeof value === 'number',
      );
      if (quotaValues.length > 0) {
        const avgUsed = quotaValues.reduce((sum, value) => sum + value, 0) / quotaValues.length;
        return 100 - avgUsed;
      }

      return (account.last_used || account.created_at || 0) / 1e9;
    };

    return others.reduce((prev, curr) => (getScore(curr) > getScore(prev) ? curr : prev));
  }, [windsurfAccounts, windsurfCurrent?.id]);

  const kiroRecommended = useMemo(() => {
    if (kiroAccounts.length <= 1) return null;
    const currentId = kiroCurrent?.id;
    const others = kiroAccounts.filter(
      (account) => account.id !== currentId && !isKiroAccountBanned(account),
    );
    if (others.length === 0) return null;

    const getScore = (account: KiroAccount) => {
      const credits = getKiroCreditsSummary(account);
      const promptLeft = toFiniteNumber(credits.promptCreditsLeft);
      const addOnLeft = toFiniteNumber(credits.addOnCredits);

      if (promptLeft != null) {
        return promptLeft * 1000 + (addOnLeft ?? 0);
      }

      const quotaValues = [account.quota?.hourly_percentage, account.quota?.weekly_percentage].filter(
        (value): value is number => typeof value === 'number',
      );
      if (quotaValues.length > 0) {
        const avgUsed = quotaValues.reduce((sum, value) => sum + value, 0) / quotaValues.length;
        return 100 - avgUsed;
      }

      return (account.last_used || account.created_at || 0) / 1e9;
    };

    return others.reduce((prev, curr) => (getScore(curr) > getScore(prev) ? curr : prev));
  }, [kiroAccounts, kiroCurrent?.id]);

  const cursorRecommended = useMemo(() => {
    if (cursorAccounts.length <= 1) return null;
    const currentId = cursorCurrent?.id;
    const others = cursorAccounts.filter((a) => a.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (account: CursorAccount) => {
      const usage = getCursorUsage(account);
      const planLimit = toFiniteNumber(usage.planLimitCents);
      const planUsedRaw = toFiniteNumber(usage.planUsedCents);
      const hasPlanBudget = planLimit != null && planLimit > 0;
      const planUsed = planUsedRaw != null ? Math.max(planUsedRaw, 0) : null;
      const remainingBudget = hasPlanBudget
        ? Math.max((planLimit ?? 0) - (planUsed ?? 0), 0)
        : -1;

      const totalUsedPercent = toFiniteNumber(
        usage.totalPercentUsed ??
          (hasPlanBudget && planUsed != null && planLimit != null && planLimit > 0
            ? (planUsed / planLimit) * 100
            : null),
      );
      const usedPercentList = [
        totalUsedPercent,
        toFiniteNumber(usage.autoPercentUsed),
        toFiniteNumber(usage.apiPercentUsed),
      ].filter((value): value is number => value != null);
      const avgUsedPercent = usedPercentList.length > 0
        ? usedPercentList.reduce((sum, value) => sum + value, 0) / usedPercentList.length
        : 101;

      return {
        hasPlanBudget,
        remainingBudget,
        avgUsedPercent,
        freshness: account.last_used || account.created_at || 0,
      };
    };

    return others.reduce((best, candidate) => {
      const bestScore = getScore(best);
      const candidateScore = getScore(candidate);

      // 优先推荐有明确套餐额度（limit > 0）的账号，避免 0/0 FREE 抢占推荐位。
      if (bestScore.hasPlanBudget !== candidateScore.hasPlanBudget) {
        return candidateScore.hasPlanBudget ? candidate : best;
      }

      // 主排序：按剩余额度（limit - used）降序。
      if (bestScore.remainingBudget !== candidateScore.remainingBudget) {
        return candidateScore.remainingBudget > bestScore.remainingBudget
          ? candidate
          : best;
      }

      // 兜底：同剩余额度时，已用百分比更低优先；再按最近使用时间。
      if (bestScore.avgUsedPercent !== candidateScore.avgUsedPercent) {
        return candidateScore.avgUsedPercent < bestScore.avgUsedPercent
          ? candidate
          : best;
      }

      return candidateScore.freshness > bestScore.freshness ? candidate : best;
    });
  }, [cursorAccounts, cursorCurrent?.id]);

  const geminiRecommended = useMemo(() => {
    if (geminiAccounts.length <= 1) return null;
    const currentId = geminiCurrent?.id;
    const others = geminiAccounts.filter((a) => a.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (account: GeminiAccount) => {
      const tiers = getGeminiTierQuotaSummary(account);
      const remainingValues = [tiers.pro.remainingPercent, tiers.flash.remainingPercent].filter(
        (value): value is number => typeof value === 'number' && Number.isFinite(value),
      );
      const totalUsed = remainingValues.length > 0
        ? 100 - Math.min(...remainingValues)
        : null;
      return {
        remainingPercent: totalUsed == null ? -1 : 100 - totalUsed,
        freshness: account.last_used || account.created_at || 0,
      };
    };

    return others.reduce((best, candidate) => {
      const bestScore = getScore(best);
      const candidateScore = getScore(candidate);
      if (candidateScore.remainingPercent !== bestScore.remainingPercent) {
        return candidateScore.remainingPercent > bestScore.remainingPercent
          ? candidate
          : best;
      }
      return candidateScore.freshness > bestScore.freshness ? candidate : best;
    });
  }, [geminiAccounts, geminiCurrent?.id]);

  const codebuddyRecommended = useMemo(() => {
    if (codebuddyAccounts.length <= 1) return null;
    const currentId = codebuddyCurrent?.id;
    const others = codebuddyAccounts.filter((a) => a.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (account: CodebuddyAccount) => {
      const resource = getCodebuddyResourceSummary(account);
      const extra = getCodebuddyExtraCreditSummary(account);
      const remain = resource?.remainPercent ?? (extra.remainPercent ?? -1);
      return {
        remainPercent: remain,
        freshness: account.last_used || account.created_at || 0,
      };
    };

    return others.reduce((best, candidate) => {
      const bestScore = getScore(best);
      const candidateScore = getScore(candidate);
      if (candidateScore.remainPercent !== bestScore.remainPercent) {
        return candidateScore.remainPercent > bestScore.remainPercent ? candidate : best;
      }
      return candidateScore.freshness > bestScore.freshness ? candidate : best;
    });
  }, [codebuddyAccounts, codebuddyCurrent?.id]);

  const codebuddyCnRecommended = useMemo(() => {
    if (codebuddyCnAccounts.length <= 1) return null;
    const currentId = codebuddyCnCurrent?.id;
    const others = codebuddyCnAccounts.filter((a) => a.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (account: CodebuddyAccount) => {
      const resource = getCodebuddyResourceSummary(account);
      const extra = getCodebuddyExtraCreditSummary(account);
      const remain = resource?.remainPercent ?? (extra.remainPercent ?? -1);
      return {
        remainPercent: remain,
        freshness: account.last_used || account.created_at || 0,
      };
    };

    return others.reduce((best, candidate) => {
      const bestScore = getScore(best);
      const candidateScore = getScore(candidate);
      if (candidateScore.remainPercent !== bestScore.remainPercent) {
        return candidateScore.remainPercent > bestScore.remainPercent ? candidate : best;
      }
      return candidateScore.freshness > bestScore.freshness ? candidate : best;
    });
  }, [codebuddyCnAccounts, codebuddyCnCurrent?.id]);

  const qoderRecommended = useMemo(() => {
    if (qoderAccounts.length <= 1) return null;
    const currentId = qoderCurrent?.id;
    const others = qoderAccounts.filter((a) => a.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (account: QoderAccount) => {
      const sub = getQoderSubscriptionInfo(account);
      const usedPercent = sub.totalUsagePercentage ?? sub.userQuota.percentage ?? 101;
      return {
        remaining: 100 - usedPercent,
        freshness: account.last_used || account.created_at || 0,
      };
    };

    return others.reduce((best, candidate) => {
      const bestScore = getScore(best);
      const candidateScore = getScore(candidate);
      if (candidateScore.remaining !== bestScore.remaining) {
        return candidateScore.remaining > bestScore.remaining ? candidate : best;
      }
      return candidateScore.freshness > bestScore.freshness ? candidate : best;
    });
  }, [qoderAccounts, qoderCurrent?.id]);

  const traeRecommended = useMemo(() => {
    if (traeAccounts.length <= 1) return null;
    const currentId = traeCurrent?.id;
    const others = traeAccounts.filter((a) => a.id !== currentId);
    if (others.length === 0) return null;

    const getScore = (account: TraeAccount) => {
      const usage = getTraeUsage(account);
      const usedPercent = usage.usedPercent ?? 101;
      return {
        remaining: 100 - usedPercent,
        freshness: account.last_used || account.created_at || 0,
      };
    };

    return others.reduce((best, candidate) => {
      const bestScore = getScore(best);
      const candidateScore = getScore(candidate);
      if (candidateScore.remaining !== bestScore.remaining) {
        return candidateScore.remaining > bestScore.remaining ? candidate : best;
      }
      return candidateScore.freshness > bestScore.freshness ? candidate : best;
    });
  }, [traeAccounts, traeCurrent?.id]);

  // Render Helpers
  const renderAgAccountContent = (account: Account | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildAntigravityAccountPresentation(account, agDisplayGroups, t);
    const quotaDisplayItems = presentation.quotaItems.slice(0, 4);

    return (
      <div className="account-mini-card">
        <div className="account-mini-header">
           <div className="account-info-row">
             <span className="account-email" title={maskAccountText(presentation.displayName)}>
               {maskAccountText(presentation.displayName)}
             </span>
             <span className={`tier-tag ${presentation.planClass}`}>{presentation.planLabel}</span>
           </div>
        </div>
        
        <div className="account-mini-quotas">
          {quotaDisplayItems.map((item) => (
            <div key={item.key} className="mini-quota-row-stacked">
              <div className="mini-quota-header">
                <span className="model-name">{item.label}</span>
                <span className={`model-pct ${item.quotaClass}`}>{item.valueText}</span>
              </div>
              <div className="mini-progress-track">
                <div 
                  className={`mini-progress-bar ${item.quotaClass}`}
                  style={{ width: `${item.percentage}%` }}
                />
              </div>
              {item.resetText && (
                <div className="mini-reset-time">
                  {item.resetText}
                </div>
              )}
            </div>
          ))}
          {quotaDisplayItems.length === 0 && <span className="no-data-text">{t('dashboard.noData', '暂无数据')}</span>}
        </div>

        <div className="account-mini-actions icon-only-row">
           <button 
             className="mini-icon-btn" 
             onClick={() => handleRefreshAg(account.id)}
             title={t('common.refresh', '刷新')}
             disabled={refreshing.has(account.id)}
           >
             <RotateCw size={14} className={refreshing.has(account.id) ? 'loading-spinner' : ''} />
           </button>
           <button 
             className="mini-icon-btn"
             onClick={() => switchAgAccount(account.id)}
             title={t('dashboard.switch', '切换')}
           >
             <Play size={14} />
           </button>
        </div>
      </div>
    );
  };

  const renderCodexAccountContent = (account: CodexAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildCodexAccountPresentation(account, t);
    const quotaWindows = presentation.quotaItems;
    
    return (
      <div className="account-mini-card">
        <div className="account-mini-header">
           <div className="account-info-row">
             <span className="account-email" title={maskAccountText(presentation.displayName)}>
               {maskAccountText(presentation.displayName)}
             </span>
             <span className={`tier-tag ${presentation.planClass}`}>{presentation.planLabel}</span>
           </div>
        </div>
        
        <div className="account-mini-quotas">
          {quotaWindows.length === 0 && (
            <span className="no-data-text">{t('dashboard.noData', '暂无数据')}</span>
          )}
          {quotaWindows.map((window) => (
            <div key={window.key} className="mini-quota-row-stacked">
              <div className="mini-quota-header">
                <span className="model-name">{window.label}</span>
                <span className={`model-pct ${window.quotaClass}`}>
                  {window.valueText}
                </span>
              </div>
              <div className="mini-progress-track">
                <div
                  className={`mini-progress-bar ${window.quotaClass}`}
                  style={{ width: `${window.percentage}%` }}
                />
              </div>
              {window.resetText && (
                <div className="mini-reset-time">
                  {window.resetText}
                </div>
              )}
            </div>
          ))}
        </div>

        <div className="account-mini-actions icon-only-row">
           <button 
             className="mini-icon-btn" 
             onClick={() => handleRefreshCodex(account.id)}
             title={t('common.refresh', '刷新')}
             disabled={refreshing.has(account.id)}
           >
             <RotateCw size={14} className={refreshing.has(account.id) ? 'loading-spinner' : ''} />
           </button>
           <button 
             className="mini-icon-btn"
             onClick={() => switchCodexAccount(account.id)}
             title={t('dashboard.switch', '切换')}
           >
             <Play size={14} />
           </button>
        </div>
      </div>
    );
  };

  const renderGitHubCopilotAccountContent = (account: GitHubCopilotAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildGitHubCopilotAccountPresentation(account, t);
    const inlineMetric = presentation.quotaItems.find((item) => item.key === 'inline') || null;
    const chatMetric = presentation.quotaItems.find((item) => item.key === 'chat') || null;
    const premiumMetric = presentation.quotaItems.find((item) => item.key === 'premium') || null;
    const isRefreshing = refreshing.has(account.id);
    const isSwitching = switching.has(account.id);

    return (
      <div className="account-mini-card">
        <div className="account-mini-header">
          <div className="account-info-row">
            <span className="account-email" title={maskAccountText(presentation.displayName)}>
              {maskAccountText(presentation.displayName)}
            </span>
            <span className={`tier-tag ${presentation.planClass}`}>{presentation.planLabel}</span>
          </div>
        </div>

        <div className="account-mini-quotas">
          <div className="mini-quota-row-stacked">
            <div className="mini-quota-header">
              <span className="model-name">{inlineMetric?.label || t('common.shared.quota.hourly', 'Inline Suggestions')}</span>
              <span className={`model-pct ${inlineMetric?.quotaClass || ''}`}>
                {inlineMetric?.valueText || '-'}
              </span>
            </div>
            <div className="mini-progress-track">
              <div
                className={`mini-progress-bar ${inlineMetric?.quotaClass || ''}`}
                style={{ width: `${inlineMetric?.percentage ?? 0}%` }}
              />
            </div>
            {inlineMetric?.resetText && (
              <div className="mini-reset-time">
                {inlineMetric.resetText}
              </div>
            )}
          </div>

          <div className="mini-quota-row-stacked">
            <div className="mini-quota-header">
              <span className="model-name">{chatMetric?.label || t('common.shared.quota.weekly', 'Chat messages')}</span>
              <span className={`model-pct ${chatMetric?.quotaClass || ''}`}>
                {chatMetric?.valueText || '-'}
              </span>
            </div>
            <div className="mini-progress-track">
              <div
                className={`mini-progress-bar ${chatMetric?.quotaClass || ''}`}
                style={{ width: `${chatMetric?.percentage ?? 0}%` }}
              />
            </div>
            {chatMetric?.resetText && (
              <div className="mini-reset-time">
                {chatMetric.resetText}
              </div>
            )}
          </div>

          <div className="mini-quota-row-stacked">
            <div className="mini-quota-header">
              <span className="model-name">{premiumMetric?.label || t('githubCopilot.columns.premium', 'Premium requests')}</span>
              <span className={`model-pct ${premiumMetric?.quotaClass || ''}`}>
                {premiumMetric?.valueText || '-'}
              </span>
            </div>
            <div className="mini-progress-track">
              <div
                className={`mini-progress-bar ${premiumMetric?.quotaClass || ''}`}
                style={{ width: `${premiumMetric?.percentage ?? 0}%` }}
              />
            </div>
          </div>
        </div>

        <div className="account-mini-actions icon-only-row">
          <button
            className="mini-icon-btn"
            onClick={() => handleRefreshGitHubCopilot(account.id)}
            title={t('common.refresh', '刷新')}
            disabled={isRefreshing || isSwitching}
          >
            <RotateCw size={14} className={isRefreshing ? 'loading-spinner' : ''} />
          </button>
          <button
            className="mini-icon-btn"
            onClick={() => handleSwitchGitHubCopilot(account.id)}
            title={t('dashboard.switch', '切换')}
            disabled={isSwitching}
          >
            {isSwitching ? <RotateCw size={14} className="loading-spinner" /> : <Play size={14} />}
          </button>
        </div>
      </div>
    );
  };

  const renderWindsurfAccountContent = (account: WindsurfAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildWindsurfAccountPresentation(account, t);
    const promptMetric = presentation.quotaItems.find((item) => item.key === 'prompt') || null;
    const addOnMetric = presentation.quotaItems.find((item) => item.key === 'addon') || null;
    const isRefreshing = refreshing.has(account.id);
    const isSwitching = switching.has(account.id);

    return (
      <div className="account-mini-card">
        <div className="account-mini-header">
          <div className="account-info-row">
            <span className="account-email" title={maskAccountText(presentation.displayName)}>
              {maskAccountText(presentation.displayName)}
            </span>
            <span className={`tier-tag ${presentation.planClass}`}>{presentation.planLabel}</span>
          </div>
        </div>

        <div className="account-mini-quotas">
          <div className="mini-quota-row-stacked">
            <div className="mini-quota-header">
              <span className="model-name">
                {promptMetric?.label || t('common.shared.columns.promptCredits', 'User Prompt credits')}
              </span>
              <span className={`model-pct ${promptMetric?.quotaClass || ''}`}>
                {promptMetric?.valueText || '-'}
              </span>
            </div>
            <div className="mini-progress-track">
              <div
                className={`mini-progress-bar ${promptMetric?.quotaClass || ''}`}
                style={{ width: `${promptMetric?.percentage ?? 0}%` }}
              />
            </div>
            <div className="mini-reset-time">
              {promptMetric?.resetText || presentation.cycleText || t('common.shared.credits.planEndsUnknown', '配额周期时间未知')}
            </div>
          </div>

          <div className="mini-quota-row-stacked">
            <div className="mini-quota-header">
              <span className="model-name">
                {addOnMetric?.label || t('common.shared.columns.addOnPromptCredits', 'Add-on prompt credits')}
              </span>
              <span className={`model-pct ${addOnMetric?.quotaClass || ''}`}>
                {addOnMetric?.valueText || '-'}
              </span>
            </div>
            <div className="mini-progress-track">
              <div
                className={`mini-progress-bar ${addOnMetric?.quotaClass || ''}`}
                style={{ width: `${addOnMetric?.percentage ?? 0}%` }}
              />
            </div>
            <div className="mini-reset-time">
              {addOnMetric?.resetText || presentation.cycleText || t('common.shared.credits.planEndsUnknown', '配额周期时间未知')}
            </div>
          </div>
        </div>

        <div className="account-mini-actions icon-only-row">
          <button
            className="mini-icon-btn"
            onClick={() => handleRefreshWindsurf(account.id)}
            title={t('common.refresh', '刷新')}
            disabled={isRefreshing || isSwitching}
          >
            <RotateCw size={14} className={isRefreshing ? 'loading-spinner' : ''} />
          </button>
          <button
            className="mini-icon-btn"
            onClick={() => handleSwitchWindsurf(account.id)}
            title={t('dashboard.switch', '切换')}
            disabled={isSwitching}
          >
            {isSwitching ? <RotateCw size={14} className="loading-spinner" /> : <Play size={14} />}
          </button>
        </div>
      </div>
    );
  };

  const renderKiroAccountContent = (account: KiroAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildKiroAccountPresentation(account, t);
    const promptMetric = presentation.quotaItems.find((item) => item.key === 'prompt') || null;
    const addOnMetric = presentation.quotaItems.find((item) => item.key === 'addon') || null;
    const isRefreshing = refreshing.has(account.id);
    const isSwitching = switching.has(account.id);
    const hasAddOnCredits = Boolean(addOnMetric);

    return (
      <div className="account-mini-card">
        <div className="account-mini-header">
          <div className="account-info-row">
            <span className="account-email" title={maskAccountText(presentation.displayName)}>
              {maskAccountText(presentation.displayName)}
            </span>
            <span className={`tier-tag ${presentation.planClass}`}>{presentation.planLabel}</span>
          </div>
        </div>

        <div className="account-mini-quotas">
          <div className="mini-quota-row-stacked">
            <div className="mini-quota-header">
              <span className="model-name">
                {promptMetric?.label || t('common.shared.columns.promptCredits', 'User Prompt credits')}
              </span>
              <span className={`model-pct ${promptMetric?.quotaClass || ''}`}>
                {promptMetric?.valueText || '-'}
              </span>
            </div>
            <div className="mini-progress-track">
              <div
                className={`mini-progress-bar ${promptMetric?.quotaClass || ''}`}
                style={{ width: `${promptMetric?.percentage ?? 0}%` }}
              />
            </div>
            <div className="mini-reset-time">
              {promptMetric?.resetText || presentation.cycleText || t('common.shared.credits.planEndsUnknown', '配额周期时间未知')}
            </div>
          </div>

          {hasAddOnCredits && (
            <div className="mini-quota-row-stacked">
              <div className="mini-quota-header">
                <span className="model-name">
                  {addOnMetric?.label || t('common.shared.columns.addOnPromptCredits', 'Add-on prompt credits')}
                </span>
                <span className={`model-pct ${addOnMetric?.quotaClass || ''}`}>
                  {addOnMetric?.valueText || '-'}
                </span>
              </div>
              <div className="mini-progress-track">
                <div
                  className={`mini-progress-bar ${addOnMetric?.quotaClass || ''}`}
                  style={{ width: `${addOnMetric?.percentage ?? 0}%` }}
                />
              </div>
              <div className="mini-reset-time">
                {addOnMetric?.resetText ||
                  presentation.cycleText ||
                  t('common.shared.credits.planEndsUnknown', '配额周期时间未知')}
              </div>
            </div>
          )}
        </div>

        <div className="account-mini-actions icon-only-row">
          <button
            className="mini-icon-btn"
            onClick={() => handleRefreshKiro(account.id)}
            title={t('common.refresh', '刷新')}
            disabled={isRefreshing || isSwitching}
          >
            <RotateCw size={14} className={isRefreshing ? 'loading-spinner' : ''} />
          </button>
          <button
            className="mini-icon-btn"
            onClick={() => handleSwitchKiro(account.id)}
            title={t('dashboard.switch', '切换')}
            disabled={isSwitching || presentation.isBanned}
          >
            {isSwitching ? <RotateCw size={14} className="loading-spinner" /> : <Play size={14} />}
          </button>
        </div>
      </div>
    );
  };

  const renderCursorAccountContent = (account: CursorAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const presentation = buildCursorAccountPresentation(account, t);
    const authIdText = (account.auth_id || '').trim();
    const maskedAuthIdText = authIdText ? maskAccountText(authIdText) : '--';
    const totalMetric = presentation.quotaItems.find((item) => item.key === 'total') || null;
    const secondaryMetrics = presentation.quotaItems.filter((item) =>
      item.key === 'auto' || item.key === 'api' || item.key === 'on_demand',
    );
    const isRefreshing = refreshing.has(account.id);
    const isSwitching = switching.has(account.id);

    return (
      <div className="account-mini-card">
        <div className="account-mini-header">
          <div className="account-info-row">
            <span className="account-email" title={maskAccountText(presentation.displayName)}>
              {maskAccountText(presentation.displayName)}
            </span>
            <span className={`tier-tag ${presentation.planClass}`}>{presentation.planLabel}</span>
          </div>
        </div>
        <div className="account-mini-subline" title={`Auth ID: ${maskedAuthIdText}`}>
          Auth ID: {maskedAuthIdText}
        </div>

        <div className="account-mini-quotas">
          <div className="mini-quota-row-stacked">
            <div className="mini-quota-header">
              <span className="model-name">{totalMetric?.label || 'Total Usage'}</span>
              <span className={`model-pct ${totalMetric?.quotaClass || ''}`}>
                {totalMetric?.valueText || '-'}
              </span>
            </div>
            <div className="mini-progress-track">
              <div
                className={`mini-progress-bar ${totalMetric?.quotaClass || ''}`}
                style={{ width: `${totalMetric?.percentage ?? 0}%` }}
              />
            </div>
            {totalMetric?.resetText && (
              <div className="mini-reset-time">{totalMetric.resetText}</div>
            )}
          </div>

          {secondaryMetrics.map((metric) => (
            <div className="mini-quota-row-stacked" key={metric.key}>
              <div className="mini-quota-header">
                <span className="model-name">{metric.label}</span>
                <span className={`model-pct ${metric.quotaClass || ''}`}>{metric.valueText || '-'}</span>
              </div>
              <div className="mini-progress-track">
                <div
                  className={`mini-progress-bar ${metric.quotaClass || ''}`}
                  style={{ width: `${metric.percentage ?? 0}%` }}
                />
              </div>
              {metric.resetText && (
                <div className="mini-reset-time">{metric.resetText}</div>
              )}
            </div>
          ))}
        </div>

        <div className="account-mini-actions icon-only-row">
          <button
            className="mini-icon-btn"
            onClick={() => handleRefreshCursor(account.id)}
            title={t('common.refresh', '刷新')}
            disabled={isRefreshing || isSwitching}
          >
            <RotateCw size={14} className={isRefreshing ? 'loading-spinner' : ''} />
          </button>
          <button
            className="mini-icon-btn"
            onClick={() => handleSwitchCursor(account.id)}
            title={t('dashboard.switch', '切换')}
            disabled={isSwitching || presentation.isBanned}
          >
            {isSwitching ? <RotateCw size={14} className="loading-spinner" /> : <Play size={14} />}
          </button>
        </div>
      </div>
    );
  };

  const renderGeminiAccountContent = (account: GeminiAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const tierSummary = getGeminiTierQuotaSummary(account);
    const planLabel = getGeminiPlanDisplayName(account);
    const planClass = getGeminiPlanBadgeClass(undefined, account);
    const isRefreshing = refreshing.has(account.id);
    const isSwitching = switching.has(account.id);

    const formatRelativeDuration = (seconds: number) => {
      const safe = Math.max(0, Math.floor(seconds));
      const totalMinutes = Math.floor(safe / 60);
      if (totalMinutes < 1) {
        return t('common.shared.time.lessThanMinute', '<1分钟');
      }
      const days = Math.floor(totalMinutes / (60 * 24));
      const hours = Math.floor((totalMinutes % (60 * 24)) / 60);
      const minutes = totalMinutes % 60;
      if (days > 0 && hours > 0) {
        return t('common.shared.time.relativeDaysHours', '{{days}}天{{hours}}小时', { days, hours });
      }
      if (days > 0) {
        return t('common.shared.time.relativeDays', '{{days}}天', { days });
      }
      if (hours > 0 && minutes > 0) {
        return t('common.shared.time.relativeHoursMinutes', '{{hours}}小时{{minutes}}分钟', { hours, minutes });
      }
      if (hours > 0) {
        return t('common.shared.time.relativeHours', '{{hours}}小时', { hours });
      }
      return t('common.shared.time.relativeMinutes', '{{minutes}}分钟', { minutes });
    };

    const updatedAt = account.last_used || account.created_at || 0;
    const updatedDiffSeconds = Math.floor(Date.now() / 1000) - updatedAt;
    const updatedText = t('gemini.updated.label', 'Updated {{relative}} ago', {
      relative: formatRelativeDuration(updatedDiffSeconds),
    });

    const buildTierMetric = (
      key: 'pro' | 'flash',
      remainingPercent: number | null,
      resetAt: number | null,
    ) => {
      const hasRemaining = typeof remainingPercent === 'number' && Number.isFinite(remainingPercent);
      const safeRemaining = hasRemaining ? Math.max(0, Math.min(100, Math.round(remainingPercent))) : 0;
      const usedPercent = 100 - safeRemaining;
      const quotaClass = usedPercent >= 90 ? 'low' : usedPercent >= 70 ? 'medium' : 'high';
      const leftText = hasRemaining
        ? t('gemini.quota.left', '{{value}}% left', { value: safeRemaining })
        : '--';
      const resetText =
        typeof resetAt === 'number' && Number.isFinite(resetAt)
          ? (() => {
              const diffSeconds = Math.floor(resetAt - Date.now() / 1000);
              if (diffSeconds <= 0) {
                return t('gemini.quota.resetsSoon', 'Resets soon');
              }
              return t('gemini.quota.resetsIn', 'Resets in {{relative}}', {
                relative: formatRelativeDuration(diffSeconds),
              });
            })()
          : t('gemini.quota.resetsUnknown', 'Reset time unknown');

      return {
        key,
        label: t(`gemini.quota.${key}`, key === 'pro' ? 'Pro' : 'Flash'),
        percentage: safeRemaining,
        quotaClass,
        leftText,
        resetText,
      };
    };

    const tierMetrics = [
      buildTierMetric('pro', tierSummary.pro.remainingPercent, tierSummary.pro.resetAt),
      buildTierMetric('flash', tierSummary.flash.remainingPercent, tierSummary.flash.resetAt),
    ];

    return (
      <div className="account-mini-card gemini-mini-card">
        <div className="account-mini-header">
          <div className="account-info-row">
            <span className="account-email" title={maskAccountText(account.email || account.id)}>
              {maskAccountText(account.email || account.id)}
            </span>
            {planLabel && planLabel !== 'UNKNOWN' && (
              <span className={`tier-tag ${planClass} raw-value`}>{planLabel}</span>
            )}
          </div>
        </div>
        <div className="account-mini-subline">{updatedText}</div>

        <div className="account-mini-quotas">
          {tierMetrics.map((metric) => (
            <div className="mini-quota-row-stacked" key={metric.key}>
              <div className="mini-quota-header">
                <span className="model-name">{metric.label}</span>
              </div>
              <div className="mini-progress-track">
                <div
                  className={`mini-progress-bar ${metric.quotaClass || ''}`}
                  style={{ width: `${metric.percentage ?? 0}%` }}
                />
              </div>
              <div className="mini-quota-meta-row">
                <span className={`mini-left-text ${metric.quotaClass}`}>{metric.leftText}</span>
                <span className="mini-reset-time-inline">{metric.resetText}</span>
              </div>
            </div>
          ))}
        </div>

        <div className="account-mini-actions icon-only-row">
          <button
            className="mini-icon-btn"
            onClick={() => handleRefreshGemini(account.id)}
            title={t('common.refresh', '刷新')}
            disabled={isRefreshing || isSwitching}
          >
            <RotateCw size={14} className={isRefreshing ? 'loading-spinner' : ''} />
          </button>
          <button
            className="mini-icon-btn"
            onClick={() => handleSwitchGemini(account.id)}
            title={t('dashboard.switch', '切换')}
            disabled={isSwitching}
          >
            {isSwitching ? <RotateCw size={14} className="loading-spinner" /> : <Play size={14} />}
          </button>
        </div>
      </div>
    );
  };

  const renderCodebuddyAccountContent = (account: CodebuddyAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const displayName = getCodebuddyAccountDisplayEmail(account);
    const badge = getCodebuddyPlanBadge(account);
    const badgeClass = getCodebuddyPlanBadgeClass(badge);
    const usage = getCodebuddyUsage(account);
    const isRefreshing = refreshing.has(account.id);
    const isSwitching = switching.has(account.id);
    const locale = i18n.language || 'zh-CN';
    const usageStatusText = !usage.dosageNotifyCode
      ? '--'
      : usage.isNormal
        ? t('codebuddy.usageNormal', '正常')
        : locale.startsWith('zh')
          ? (usage.dosageNotifyZh || usage.dosageNotifyCode)
          : (usage.dosageNotifyEn || usage.dosageNotifyCode);
    const usageStatusClass = !usage.dosageNotifyCode ? '' : (usage.isNormal ? 'high' : 'critical');

    return (
      <div className="account-mini-card">
        <div className="account-mini-header">
          <div className="account-info-row">
            <span className="account-email" title={maskAccountText(displayName)}>
              {maskAccountText(displayName)}
            </span>
            <span className={`tier-tag ${badgeClass} raw-value`}>{badge}</span>
          </div>
        </div>

        <div className="account-mini-quotas">
          <div className="mini-quota-row-stacked">
            <div className="mini-quota-header">
              <span className="model-name">{t('codebuddy.usage', '用量状态')}</span>
              <span className={usageStatusClass ? `model-pct ${usageStatusClass}` : 'no-data-text'}>
                {usageStatusText}
              </span>
            </div>
          </div>
        </div>

        <div className="account-mini-actions icon-only-row">
          <button
            className="mini-icon-btn"
            onClick={() => handleRefreshCodebuddy(account.id)}
            title={t('common.refresh', '刷新')}
            disabled={isRefreshing || isSwitching}
          >
            <RotateCw size={14} className={isRefreshing ? 'loading-spinner' : ''} />
          </button>
          <button
            className="mini-icon-btn"
            onClick={() => handleSwitchCodebuddy(account.id)}
            title={t('dashboard.switch', '切换')}
            disabled={isSwitching}
          >
            {isSwitching ? <RotateCw size={14} className="loading-spinner" /> : <Play size={14} />}
          </button>
        </div>
      </div>
    );
  };

  const renderCodebuddyCnAccountContent = (account: CodebuddyAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const displayName = getCodebuddyAccountDisplayEmail(account);
    const badge = getCodebuddyPlanBadge(account);
    const badgeClass = getCodebuddyPlanBadgeClass(badge);
    const usage = getCodebuddyUsage(account);
    const isRefreshing = refreshing.has(account.id);
    const isSwitching = switching.has(account.id);
    const locale = i18n.language || 'zh-CN';
    const usageStatusText = !usage.dosageNotifyCode
      ? '--'
      : usage.isNormal
        ? t('codebuddy.usageNormal', '正常')
        : locale.startsWith('zh')
          ? (usage.dosageNotifyZh || usage.dosageNotifyCode)
          : (usage.dosageNotifyEn || usage.dosageNotifyCode);
    const usageStatusClass = !usage.dosageNotifyCode ? '' : (usage.isNormal ? 'high' : 'critical');
    return (
      <div className="account-mini-card">
        <div className="account-mini-header">
          <div className="account-info-row">
            <span className="account-email" title={maskAccountText(displayName)}>
              {maskAccountText(displayName)}
            </span>
            <span className={`tier-tag ${badgeClass} raw-value`}>{badge}</span>
          </div>
        </div>

        <div className="account-mini-quotas">
          <div className="mini-quota-row-stacked">
            <div className="mini-quota-header">
              <span className="model-name">{t('codebuddy.usage', '用量状态')}</span>
              <span className={usageStatusClass ? `model-pct ${usageStatusClass}` : 'no-data-text'}>
                {usageStatusText}
              </span>
            </div>
          </div>
        </div>

        <div className="account-mini-actions icon-only-row">
          <button
            className="mini-icon-btn"
            onClick={() => handleRefreshCodebuddyCn(account.id)}
            title={t('common.refresh', '刷新')}
            disabled={isRefreshing || isSwitching}
          >
            <RotateCw size={14} className={isRefreshing ? 'loading-spinner' : ''} />
          </button>
          <button
            className="mini-icon-btn"
            onClick={() => handleSwitchCodebuddyCn(account.id)}
            title={t('dashboard.switch', '切换')}
            disabled={isSwitching}
          >
            {isSwitching ? <RotateCw size={14} className="loading-spinner" /> : <Play size={14} />}
          </button>
        </div>
      </div>
    );
  };

  const renderQoderAccountContent = (account: QoderAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const displayName = getQoderAccountDisplayEmail(account);
    const planTag = getQoderPlanBadge(account);
    const sub = getQoderSubscriptionInfo(account);
    const isRefreshing = refreshing.has(account.id);
    const isSwitching = switching.has(account.id);

    const usedPercent = sub.totalUsagePercentage ?? sub.userQuota.percentage ?? null;
    const remainPercent = usedPercent != null ? Math.max(0, Math.min(100, Math.round(100 - usedPercent))) : null;
    const quotaClass = remainPercent != null ? (remainPercent <= 10 ? 'low' : remainPercent <= 30 ? 'medium' : 'high') : '';
    const hasAddOn = sub.addOnQuota.total != null && sub.addOnQuota.total > 0;
    const addOnRemain = hasAddOn ? (sub.addOnQuota.remaining ?? 0) : 0;
    const addOnTotal = hasAddOn ? (sub.addOnQuota.total ?? 0) : 0;
    const addOnPercent = addOnTotal > 0 ? Math.round((addOnRemain / addOnTotal) * 100) : 0;
    const addOnClass = addOnPercent <= 10 ? 'low' : addOnPercent <= 30 ? 'medium' : 'high';

    return (
      <div className="account-mini-card">
        <div className="account-mini-header">
          <div className="account-info-row">
            <span className="account-email" title={maskAccountText(displayName)}>
              {maskAccountText(displayName)}
            </span>
            <span className="tier-tag plan-badge raw-value">{planTag}</span>
          </div>
        </div>

        <div className="account-mini-quotas">
          {remainPercent != null ? (
            <div className="mini-quota-row-stacked">
              <div className="mini-quota-header">
                <span className="model-name">{t('qoder.columns.credits', 'Credits')}</span>
                <span className={`model-pct ${quotaClass}`}>
                  {remainPercent}% {t('common.shared.remaining', '剩余')}
                </span>
              </div>
              <div className="mini-progress-track">
                <div
                  className={`mini-progress-bar ${quotaClass}`}
                  style={{ width: `${remainPercent}%` }}
                />
              </div>
            </div>
          ) : (
            <span className="no-data-text">{t('dashboard.noData', '暂无数据')}</span>
          )}
          {hasAddOn && (
            <div className="mini-quota-row-stacked">
              <div className="mini-quota-header">
                <span className="model-name">{t('qoder.columns.addOn', 'Add-on')}</span>
                <span className={`model-pct ${addOnClass}`}>
                  {addOnRemain}/{addOnTotal}
                </span>
              </div>
              <div className="mini-progress-track">
                <div
                  className={`mini-progress-bar ${addOnClass}`}
                  style={{ width: `${addOnPercent}%` }}
                />
              </div>
            </div>
          )}
        </div>

        <div className="account-mini-actions icon-only-row">
          <button
            className="mini-icon-btn"
            onClick={() => handleRefreshQoder(account.id)}
            title={t('common.refresh', '刷新')}
            disabled={isRefreshing || isSwitching}
          >
            <RotateCw size={14} className={isRefreshing ? 'loading-spinner' : ''} />
          </button>
          <button
            className="mini-icon-btn"
            onClick={() => handleSwitchQoder(account.id)}
            title={t('dashboard.switch', '切换')}
            disabled={isSwitching}
          >
            {isSwitching ? <RotateCw size={14} className="loading-spinner" /> : <Play size={14} />}
          </button>
        </div>
      </div>
    );
  };

  const renderTraeAccountContent = (account: TraeAccount | null) => {
    if (!account) return <div className="empty-slot">{t('dashboard.noAccount', '无账号')}</div>;

    const displayName = getTraeAccountDisplayEmail(account);
    const planTag = getTraePlanBadge(account);
    const planClass = getTraePlanBadgeClass(planTag);
    const usage = getTraeUsage(account);
    const isRefreshing = refreshing.has(account.id);
    const isSwitching = switching.has(account.id);

    const usedPercent = usage.usedPercent;
    const remainPercent = usedPercent != null ? Math.max(0, Math.min(100, 100 - Math.round(usedPercent))) : null;
    const quotaClass = remainPercent != null ? (remainPercent <= 10 ? 'low' : remainPercent <= 30 ? 'medium' : 'high') : '';
    const spentText = usage.spentUsd != null && usage.totalUsd != null
      ? `$${usage.spentUsd.toFixed(2)} / $${usage.totalUsd.toFixed(2)}`
      : null;

    return (
      <div className="account-mini-card">
        <div className="account-mini-header">
          <div className="account-info-row">
            <span className="account-email" title={maskAccountText(displayName)}>
              {maskAccountText(displayName)}
            </span>
            <span className={`tier-tag plan-badge plan-${planClass} raw-value`}>{planTag}</span>
          </div>
        </div>

        <div className="account-mini-quotas">
          {remainPercent != null ? (
            <div className="mini-quota-row-stacked">
              <div className="mini-quota-header">
                <span className="model-name">{t('trae.columns.usage', 'Usage')}</span>
                <span className={`model-pct ${quotaClass}`}>
                  {remainPercent}% {t('common.shared.remaining', '剩余')}
                </span>
              </div>
              <div className="mini-progress-track">
                <div
                  className={`mini-progress-bar ${quotaClass}`}
                  style={{ width: `${remainPercent}%` }}
                />
              </div>
              {spentText && (
                <div className="mini-reset-time">{spentText}</div>
              )}
            </div>
          ) : (
            <span className="no-data-text">{t('dashboard.noData', '暂无数据')}</span>
          )}
        </div>

        <div className="account-mini-actions icon-only-row">
          <button
            className="mini-icon-btn"
            onClick={() => handleRefreshTrae(account.id)}
            title={t('common.refresh', '刷新')}
            disabled={isRefreshing || isSwitching}
          >
            <RotateCw size={14} className={isRefreshing ? 'loading-spinner' : ''} />
          </button>
          <button
            className="mini-icon-btn"
            onClick={() => handleSwitchTrae(account.id)}
            title={t('dashboard.switch', '切换')}
            disabled={isSwitching}
          >
            {isSwitching ? <RotateCw size={14} className="loading-spinner" /> : <Play size={14} />}
          </button>
        </div>
      </div>
    );
  };

  const platformCounts: Record<PlatformId, number> = {
    antigravity: stats.antigravity,
    codex: stats.codex,
    'github-copilot': stats.githubCopilot,
    windsurf: stats.windsurf,
    kiro: stats.kiro,
    cursor: stats.cursor,
    gemini: stats.gemini,
    codebuddy: stats.codebuddy,
    codebuddy_cn: stats.codebuddy_cn,
    qoder: stats.qoder,
    trae: stats.trae,
  };

  const visibleCardPlatformIds = visiblePlatformOrder;
  const isSinglePlatformMode = visibleCardPlatformIds.length === 1;
  const cardRows = useMemo(() => {
    const rows: PlatformId[][] = [];
    for (let i = 0; i < visibleCardPlatformIds.length; i += 2) {
      rows.push(visibleCardPlatformIds.slice(i, i + 2));
    }
    return rows;
  }, [visibleCardPlatformIds]);

  const renderPlatformCard = (platformId: PlatformId) => {
    if (platformId === 'antigravity') {
      return (
        <div className="main-card antigravity-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <RobotIcon className="" style={{ width: 18, height: 18 }} />
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <button
              className="header-action-btn"
              onClick={handleRefreshAgCard}
              disabled={cardRefreshing.ag}
              title={t('common.refresh', '刷新')}
            >
              <RotateCw size={14} className={cardRefreshing.ag ? 'loading-spinner' : ''} />
              <span>{t('common.refresh', '刷新')}</span>
            </button>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderAgAccountContent(agCurrentAccount)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {agRecommended ? (
                renderAgAccountContent(agRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('overview')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'codex') {
      return (
        <div className="main-card codex-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <CodexIcon size={18} />
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <button
              className="header-action-btn"
              onClick={handleRefreshCodexCard}
              disabled={cardRefreshing.codex}
              title={t('common.refresh', '刷新')}
            >
              <RotateCw size={14} className={cardRefreshing.codex ? 'loading-spinner' : ''} />
              <span>{t('common.refresh', '刷新')}</span>
            </button>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderCodexAccountContent(codexCurrentAccount)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {codexRecommended ? (
                renderCodexAccountContent(codexRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('codex')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'github-copilot') {
      return (
        <div className="main-card github-copilot-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <Github size={18} />
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <button
              className="header-action-btn"
              onClick={handleRefreshGitHubCopilotCard}
              disabled={cardRefreshing.githubCopilot}
              title={t('common.refresh', '刷新')}
            >
              <RotateCw size={14} className={cardRefreshing.githubCopilot ? 'loading-spinner' : ''} />
              <span>{t('common.refresh', '刷新')}</span>
            </button>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderGitHubCopilotAccountContent(githubCopilotCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {githubCopilotRecommended ? (
                renderGitHubCopilotAccountContent(githubCopilotRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('github-copilot')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'windsurf') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <WindsurfIcon className="" style={{ width: 18, height: 18 }} />
              <h3>Windsurf</h3>
            </div>
            <button
              className="header-action-btn"
              onClick={handleRefreshWindsurfCard}
              disabled={cardRefreshing.windsurf}
              title={t('common.refresh', '刷新')}
            >
              <RotateCw size={14} className={cardRefreshing.windsurf ? 'loading-spinner' : ''} />
              <span>{t('common.refresh', '刷新')}</span>
            </button>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderWindsurfAccountContent(windsurfCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {windsurfRecommended ? (
                renderWindsurfAccountContent(windsurfRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('windsurf')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'kiro') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <KiroIcon style={{ width: 18, height: 18 }} />
              <h3>Kiro</h3>
            </div>
            <button
              className="header-action-btn"
              onClick={handleRefreshKiroCard}
              disabled={cardRefreshing.kiro}
              title={t('common.refresh', '刷新')}
            >
              <RotateCw size={14} className={cardRefreshing.kiro ? 'loading-spinner' : ''} />
              <span>{t('common.refresh', '刷新')}</span>
            </button>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderKiroAccountContent(kiroCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {kiroRecommended ? (
                renderKiroAccountContent(kiroRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('kiro')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'cursor') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <CursorIcon style={{ width: 18, height: 18 }} />
              <h3>Cursor</h3>
            </div>
            <button
              className="header-action-btn"
              onClick={handleRefreshCursorCard}
              disabled={cardRefreshing.cursor}
              title={t('common.refresh', '刷新')}
            >
              <RotateCw size={14} className={cardRefreshing.cursor ? 'loading-spinner' : ''} />
              <span>{t('common.refresh', '刷新')}</span>
            </button>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderCursorAccountContent(cursorCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {cursorRecommended ? (
                renderCursorAccountContent(cursorRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('cursor')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'gemini') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <GeminiIcon style={{ width: 18, height: 18 }} />
              <h3>Gemini Cli</h3>
            </div>
            <button
              className="header-action-btn"
              onClick={handleRefreshGeminiCard}
              disabled={cardRefreshing.gemini}
              title={t('common.refresh', '刷新')}
            >
              <RotateCw size={14} className={cardRefreshing.gemini ? 'loading-spinner' : ''} />
              <span>{t('common.refresh', '刷新')}</span>
            </button>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderGeminiAccountContent(geminiCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {geminiRecommended ? (
                renderGeminiAccountContent(geminiRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('gemini')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'codebuddy') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <CodebuddyIcon style={{ width: 18, height: 18 }} />
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <button
              className="header-action-btn"
              onClick={handleRefreshCodebuddyCard}
              disabled={cardRefreshing.codebuddy}
              title={t('common.refresh', '刷新')}
            >
              <RotateCw size={14} className={cardRefreshing.codebuddy ? 'loading-spinner' : ''} />
              <span>{t('common.refresh', '刷新')}</span>
            </button>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderCodebuddyAccountContent(codebuddyCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {codebuddyRecommended ? (
                renderCodebuddyAccountContent(codebuddyRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('codebuddy')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'codebuddy_cn') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <CodebuddyIcon style={{ width: 18, height: 18 }} />
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <button
              className="header-action-btn"
              onClick={handleRefreshCodebuddyCnCard}
              disabled={cardRefreshing.codebuddyCn}
              title={t('common.refresh', '刷新')}
            >
              <RotateCw size={14} className={cardRefreshing.codebuddyCn ? 'loading-spinner' : ''} />
              <span>{t('common.refresh', '刷新')}</span>
            </button>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderCodebuddyCnAccountContent(codebuddyCnCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {codebuddyCnRecommended ? (
                renderCodebuddyCnAccountContent(codebuddyCnRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('codebuddy-cn')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'qoder') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <QoderIcon style={{ width: 18, height: 18 }} />
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <button
              className="header-action-btn"
              onClick={handleRefreshQoderCard}
              disabled={cardRefreshing.qoder}
              title={t('common.refresh', '刷新')}
            >
              <RotateCw size={14} className={cardRefreshing.qoder ? 'loading-spinner' : ''} />
              <span>{t('common.refresh', '刷新')}</span>
            </button>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderQoderAccountContent(qoderCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {qoderRecommended ? (
                renderQoderAccountContent(qoderRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('qoder')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    if (platformId === 'trae') {
      return (
        <div className="main-card windsurf-card" key={platformId}>
          <div className="main-card-header">
            <div className="header-title">
              <TraeIcon style={{ width: 18, height: 18 }} />
              <h3>{getPlatformLabel(platformId, t)}</h3>
            </div>
            <button
              className="header-action-btn"
              onClick={handleRefreshTraeCard}
              disabled={cardRefreshing.trae}
              title={t('common.refresh', '刷新')}
            >
              <RotateCw size={14} className={cardRefreshing.trae ? 'loading-spinner' : ''} />
              <span>{t('common.refresh', '刷新')}</span>
            </button>
          </div>

          <div className="split-content">
            <div className="split-half current-half">
              <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
              {renderTraeAccountContent(traeCurrent)}
            </div>

            <div className="split-divider"></div>

            <div className="split-half recommend-half">
              <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
              {traeRecommended ? (
                renderTraeAccountContent(traeRecommended)
              ) : (
                <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
              )}
            </div>
          </div>

          <button className="card-footer-action" onClick={() => onNavigate('trae')}>
            {t('dashboard.viewAllAccounts', '查看所有账号')}
          </button>
        </div>
      );
    }

    // Generic fallback for any unknown/future platform
    return (
      <div className="main-card windsurf-card" key={platformId}>
        <div className="main-card-header">
          <div className="header-title">
            {renderPlatformIcon(platformId, 18)}
            <h3>{getPlatformLabel(platformId, t)}</h3>
          </div>
        </div>

        <div className="split-content">
          <div className="split-half current-half">
            <span className="half-label"><CheckCircle2 size={12} /> {t('dashboard.current', '当前账户')}</span>
            <div className="empty-slot-text">{t('dashboard.noData', '暂无数据')}</div>
          </div>

          <div className="split-divider"></div>

          <div className="split-half recommend-half">
            <span className="half-label"><Sparkles size={12} /> {t('dashboard.recommended', '推荐账号')}</span>
            <div className="empty-slot-text">{t('dashboard.noRecommendation', '暂无更好推荐')}</div>
          </div>
        </div>

        <button className="card-footer-action" onClick={() => onNavigate(PLATFORM_PAGE_MAP[platformId])}>
          {t('dashboard.viewAllAccounts', '查看所有账号')}
        </button>
      </div>
    );
  };

  return (
    <main className="main-content dashboard-page fade-in">
      <div className="page-tabs-row" style={{ minHeight: '60px' }}>
         <div className="page-tabs-label dashboard-title-label">
           <span>{t('nav.dashboard', '仪表盘')}</span>
           <button
             className="header-action-btn dashboard-manual-btn dashboard-title-manual-btn"
             onClick={() => window.dispatchEvent(new CustomEvent('app-request-navigate', { detail: 'manual' }))}
             title={t('manual.navTitle', '功能使用手册')}
             aria-label={t('manual.navTitle', '功能使用手册')}
           >
             <HelpCircle size={16} />
           </button>
         </div>
         <div className="dashboard-top-actions">
           <button className="header-action-btn" onClick={onOpenPlatformLayout}>
             <span>{t('platformLayout.title', '平台布局')}</span>
           </button>
           <AnnouncementCenter onNavigate={onNavigate} variant="inline" trigger="button" />
         </div>
      </div>

      {/* Top Stats */}
      <div className="stats-row">
        <div className="stat-card">
          <div className="stat-icon-bg primary"><Users size={24} /></div>
          <div className="stat-info">
            <span className="stat-label">{t('dashboard.totalAccounts', '账号总数')}</span>
            <span className="stat-value">{stats.total}</span>
          </div>
        </div>

        {visiblePlatformOrder.map((platformId) => {
          const iconClass =
            platformId === 'antigravity'
              ? 'success'
              : platformId === 'codex'
              ? 'info'
              : platformId === 'github-copilot'
              ? 'github'
              : platformId === 'kiro'
                ? 'github'
              : platformId === 'cursor'
                ? 'info'
              : platformId === 'gemini'
                ? 'info'
              : 'windsurf';
          return (
            <button
              className="stat-card stat-card-button"
              key={platformId}
              onClick={() => onNavigate(PLATFORM_PAGE_MAP[platformId])}
              title={t('dashboard.switchTo', '切换到此账号')}
            >
              <div
                className={`stat-icon-bg ${iconClass} stat-icon-trigger`}
                onClick={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  onEasterEggTriggerClick();
                }}
              >
                {renderPlatformIcon(platformId, 24)}
              </div>
              <div className="stat-info">
                <span className="stat-label">{getPlatformLabel(platformId, t)}</span>
                <span className="stat-value">{platformCounts[platformId]}</span>
              </div>
            </button>
          );
        })}
      </div>

      {/* Main Comparison Section */}
      <div className="cards-section">
        {cardRows.map((row, rowIndex) => (
          <div
            className={`cards-split-row${isSinglePlatformMode ? ' single-platform-row' : ''}`}
            key={`row-${rowIndex}`}
          >
            {row.map((platformId) => renderPlatformCard(platformId))}
            {!isSinglePlatformMode && row.length < 2 && <div className="main-card main-card-placeholder" />}
          </div>
        ))}
      </div>

    </main>
  );
}
