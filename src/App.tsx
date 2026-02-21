import { Suspense, lazy, useCallback, useEffect, useRef, useState } from 'react';
import './App.css';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { RefreshCw, X } from 'lucide-react';
import { SideNav } from './components/layout/SideNav';
import { GlobalModal } from './components/GlobalModal';
import type { QuickSettingsType } from './components/QuickSettingsPopover';
import { Page } from './types/navigation';
import { useAutoRefresh } from './hooks/useAutoRefresh';
import { useEasterEggTrigger } from './hooks/useEasterEggTrigger';
import { useGlobalModal } from './hooks/useGlobalModal';
import { changeLanguage, getCurrentLanguage, normalizeLanguage } from './i18n';
import { useAccountStore } from './stores/useAccountStore';
import { useCodexAccountStore } from './stores/useCodexAccountStore';
import { useGitHubCopilotAccountStore } from './stores/useGitHubCopilotAccountStore';
import { useWindsurfAccountStore } from './stores/useWindsurfAccountStore';
import { useKiroAccountStore } from './stores/useKiroAccountStore';
import type { UpdateCheckResult } from './components/UpdateNotification';

const DashboardPage = lazy(() =>
  import('./pages/DashboardPage').then((module) => ({ default: module.DashboardPage })),
);
const AccountsPage = lazy(() =>
  import('./pages/AccountsPage').then((module) => ({ default: module.AccountsPage })),
);
const CodexAccountsPage = lazy(() =>
  import('./pages/CodexAccountsPage').then((module) => ({ default: module.CodexAccountsPage })),
);
const GitHubCopilotAccountsPage = lazy(() =>
  import('./pages/GitHubCopilotAccountsPage').then((module) => ({
    default: module.GitHubCopilotAccountsPage,
  })),
);
const WindsurfAccountsPage = lazy(() =>
  import('./pages/WindsurfAccountsPage').then((module) => ({ default: module.WindsurfAccountsPage })),
);
const KiroAccountsPage = lazy(() =>
  import('./pages/KiroAccountsPage').then((module) => ({ default: module.KiroAccountsPage })),
);
const FingerprintsPage = lazy(() =>
  import('./pages/FingerprintsPage').then((module) => ({ default: module.FingerprintsPage })),
);
const WakeupTasksPage = lazy(() =>
  import('./pages/WakeupTasksPage').then((module) => ({ default: module.WakeupTasksPage })),
);
const SettingsPage = lazy(() =>
  import('./pages/SettingsPage').then((module) => ({ default: module.SettingsPage })),
);
const InstancesPage = lazy(() =>
  import('./pages/InstancesPage').then((module) => ({ default: module.InstancesPage })),
);
const PlatformLayoutModal = lazy(() =>
  import('./components/PlatformLayoutModal').then((module) => ({
    default: module.PlatformLayoutModal,
  })),
);
const UpdateNotification = lazy(() =>
  import('./components/UpdateNotification').then((module) => ({ default: module.UpdateNotification })),
);
const CloseConfirmDialog = lazy(() =>
  import('./components/CloseConfirmDialog').then((module) => ({ default: module.CloseConfirmDialog })),
);
const BreakoutModal = lazy(() =>
  import('./components/easter-egg/BreakoutModal').then((module) => ({ default: module.BreakoutModal })),
);

interface GeneralConfigTheme {
  theme: string;
}

interface GeneralConfig extends GeneralConfigTheme {
  opencode_app_path: string;
  antigravity_app_path: string;
  codex_app_path: string;
  vscode_app_path: string;
  windsurf_app_path: string;
  kiro_app_path: string;
}

type AppPathMissingDetail = {
  app: 'antigravity' | 'codex' | 'vscode' | 'windsurf' | 'kiro';
  retry?: { kind: 'default' | 'instance'; instanceId?: string };
};

const WAKEUP_ENABLED_KEY = 'agtools.wakeup.enabled';
const TASKS_STORAGE_KEY = 'agtools.wakeup.tasks';

type WakeupHistoryRecord = {
  id: string;
  timestamp: number;
  triggerType: string;
  triggerSource: string;
  taskName?: string;
  accountEmail: string;
  modelId: string;
  prompt?: string;
  success: boolean;
  message?: string;
  duration?: number;
};

type WakeupTaskResultPayload = {
  taskId: string;
  lastRunAt: number;
  records: WakeupHistoryRecord[];
};

type QuotaAlertPayload = {
  platform?: string;
  current_account_id: string;
  current_email: string;
  threshold: number;
  lowest_percentage: number;
  low_models: string[];
  recommended_account_id?: string | null;
  recommended_email?: string | null;
  triggered_at: number;
};

type QuotaAlertPlatform = 'antigravity' | 'codex' | 'github_copilot' | 'windsurf' | 'kiro';
type UpdateCheckSource = 'auto' | 'manual';

function normalizeQuotaAlertPlatform(platform: string | undefined): QuotaAlertPlatform {
  switch (platform) {
    case 'codex':
      return 'codex';
    case 'github_copilot':
      return 'github_copilot';
    case 'windsurf':
      return 'windsurf';
    case 'kiro':
      return 'kiro';
    default:
      return 'antigravity';
  }
}

function getQuotaAlertPlatformLabel(
  platform: QuotaAlertPlatform,
  t: (key: string, defaultValue: string) => string,
): string {
  switch (platform) {
    case 'codex':
      return t('nav.codex', 'Codex');
    case 'github_copilot':
      return t('nav.githubCopilot', 'GitHub Copilot');
    case 'windsurf':
      return 'Windsurf';
    case 'kiro':
      return 'Kiro';
    default:
      return t('nav.overview', 'Antigravity');
  }
}

function getQuotaAlertTargetPage(platform: QuotaAlertPlatform): Page {
  switch (platform) {
    case 'codex':
      return 'codex';
    case 'github_copilot':
      return 'github-copilot';
    case 'windsurf':
      return 'windsurf';
    case 'kiro':
      return 'kiro';
    default:
      return 'overview';
  }
}

function getQuotaAlertQuickSettingsType(platform: QuotaAlertPlatform): QuickSettingsType {
  switch (platform) {
    case 'codex':
      return 'codex';
    case 'github_copilot':
      return 'github_copilot';
    case 'windsurf':
      return 'windsurf';
    case 'kiro':
      return 'kiro';
    default:
      return 'antigravity';
  }
}

function App() {
  const { t } = useTranslation();
  const [page, setPage] = useState<Page>('dashboard');
  const [showUpdateNotification, setShowUpdateNotification] = useState(false);
  const [updateNotificationKey, setUpdateNotificationKey] = useState(0);
  const [updateCheckSource, setUpdateCheckSource] = useState<UpdateCheckSource>('auto');
  const [showCloseDialog, setShowCloseDialog] = useState(false);
  const [showPlatformLayoutModal, setShowPlatformLayoutModal] = useState(false);
  const [showBreakout, setShowBreakout] = useState(false);
  const [appPathMissing, setAppPathMissing] = useState<AppPathMissingDetail | null>(null);
  const [appPathSetting, setAppPathSetting] = useState(false);
  const [appPathDetecting, setAppPathDetecting] = useState(false);
  const [appPathDraft, setAppPathDraft] = useState('');
  const { showModal, closeModal } = useGlobalModal();
  const trayRefreshInFlightRef = useRef(false);
  const openBreakout = useCallback(() => setShowBreakout(true), []);
  const {
    count: easterEggClickCount,
    registerClick: handleEasterEggTriggerClick,
  } = useEasterEggTrigger({
    threshold: 20,
    windowMs: 8000,
    onTrigger: openBreakout,
  });
  
  // 启用自动刷新 hook
  useAutoRefresh();

  const openQuickSettingsForPlatform = useCallback((platform: QuotaAlertPlatform) => {
    const targetPage = getQuotaAlertTargetPage(platform);
    const targetType = getQuotaAlertQuickSettingsType(platform);
    closeModal();
    setPage(targetPage);
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        window.dispatchEvent(new CustomEvent('quick-settings:open', { detail: { type: targetType } }));
      });
    });
  }, [closeModal]);

  useEffect(() => {
    let cleanup: (() => void) | null = null;

    const applyTheme = (newTheme: string) => {
      if (newTheme === 'system') {
        const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
        document.documentElement.setAttribute('data-theme', isDark ? 'dark' : 'light');
      } else {
        document.documentElement.setAttribute('data-theme', newTheme);
      }
    };

    const watchSystemTheme = () => {
      const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
      const handleChange = () => applyTheme('system');

      if (mediaQuery.addEventListener) {
        mediaQuery.addEventListener('change', handleChange);
      } else {
        mediaQuery.addListener(handleChange);
      }

      return () => {
        if (mediaQuery.removeEventListener) {
          mediaQuery.removeEventListener('change', handleChange);
        } else {
          mediaQuery.removeListener(handleChange);
        }
      };
    };

    const initTheme = async () => {
      try {
        const config = await invoke<GeneralConfigTheme>('get_general_config');
        applyTheme(config.theme);
        if (config.theme === 'system') {
          cleanup = watchSystemTheme();
        }
      } catch (error) {
        console.error('Failed to load theme config:', error);
      }
    };

    initTheme();

    return () => {
      if (cleanup) {
        cleanup();
      }
    };
  }, []);

  useEffect(() => {
    const detectAppPathsOnStartup = async () => {
      try {
        await invoke('detect_app_path', { app: 'antigravity' });
        await invoke('detect_app_path', { app: 'vscode' });
        await invoke('detect_app_path', { app: 'windsurf' });
        await invoke('detect_app_path', { app: 'kiro' });
        const userAgent = navigator.userAgent.toLowerCase();
        if (userAgent.includes('mac')) {
          await invoke('detect_app_path', { app: 'codex' });
        }
      } catch (error) {
        console.error('启动路径探测失败:', error);
      }
    };
    detectAppPathsOnStartup();
  }, []);

  useEffect(() => {
    const syncWakeupStateOnStartup = async () => {
      try {
        const enabled = localStorage.getItem(WAKEUP_ENABLED_KEY) === 'true';
        const tasksRaw = localStorage.getItem(TASKS_STORAGE_KEY);
        const tasks = tasksRaw ? JSON.parse(tasksRaw) : [];
        await invoke('wakeup_sync_state', { enabled, tasks });
      } catch (error) {
        console.error('唤醒任务状态同步失败:', error);
      }
    };
    syncWakeupStateOnStartup();
  }, []);

  // Check for updates on startup
  useEffect(() => {
    const checkUpdates = async () => {
      try {
        console.log('[App] Checking if we should check for updates...');
        const shouldCheck = await invoke<boolean>('should_check_updates');
        console.log('[App] Should check updates:', shouldCheck);

        if (shouldCheck) {
          setUpdateCheckSource('auto');
          setUpdateNotificationKey(Date.now());
          setShowUpdateNotification(true);
          // 标记已经检查过了
          await invoke('update_last_check_time');
          console.log('[App] Update check cycle initiated and last check time updated.');
        }
      } catch (error) {
        console.error('Failed to check update settings:', error);
      }
    };

    // Delay check to avoid blocking initial render
    const timer = setTimeout(checkUpdates, 2000);
    return () => clearTimeout(timer);
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    listen<string>('settings:language_changed', (event) => {
      const nextLanguage = normalizeLanguage(String(event.payload || ''));
      if (!nextLanguage || nextLanguage === getCurrentLanguage()) {
        return;
      }
      void changeLanguage(nextLanguage);
      window.dispatchEvent(new CustomEvent('general-language-updated', { detail: { language: nextLanguage } }));
    }).then((fn) => { unlisten = fn; });

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;

    listen<QuotaAlertPayload>('quota:alert', (event) => {
      const payload = event.payload;
      if (!payload || !payload.current_account_id) {
        return;
      }

      const platform = normalizeQuotaAlertPlatform(payload.platform);
      const platformLabel = getQuotaAlertPlatformLabel(platform, t);
      const hasRecommendation = Boolean(payload.recommended_account_id && payload.recommended_email);
      const modelsText = payload.low_models.length > 0
        ? payload.low_models.join(', ')
        : t('quotaAlert.modal.unknownModel', '未知模型');

      showModal({
        title: t('quotaAlert.modal.title', '配额预警'),
        description: t(
          'quotaAlert.modal.desc',
          '当前账号配额已达到预警阈值，请尽快处理。'
        ),
        width: 'md',
        closeOnOverlay: false,
        content: (
          <div className="quota-alert-modal-content">
            <div className="quota-alert-modal-row">
              <span>{t('quotaAlert.modal.platform', '平台')}</span>
              <strong>{platformLabel}</strong>
            </div>
            <div className="quota-alert-modal-row">
              <span>{t('quotaAlert.modal.account', '当前账号')}</span>
              <strong>{payload.current_email}</strong>
            </div>
            <div className="quota-alert-modal-row">
              <span>{t('quotaAlert.modal.threshold', '预警阈值')}</span>
              <strong>{payload.threshold}%</strong>
            </div>
            <div className="quota-alert-modal-row">
              <span>{t('quotaAlert.modal.lowest', '当前最低')}</span>
              <strong>{payload.lowest_percentage}%</strong>
            </div>
            <div className="quota-alert-modal-row quota-alert-modal-row--stack">
              <span>{t('quotaAlert.modal.models', '触发模型')}</span>
              <strong>{modelsText}</strong>
            </div>
            <div className="quota-alert-modal-row">
              <span>{t('quotaAlert.modal.recommended', '建议切换')}</span>
              <strong>
                {payload.recommended_email || t('quotaAlert.modal.noRecommendation', '暂无可切换账号')}
              </strong>
            </div>
          </div>
        ),
        actions: [
          {
            id: 'quota-alert-later',
            label: t('quotaAlert.modal.later', '稍后处理'),
            variant: 'secondary',
          },
          {
            id: 'quota-alert-open-settings',
            label: t('quotaAlert.modal.openSettings', '调整预警设置'),
            variant: 'secondary',
            autoClose: false,
            onClick: () => {
              openQuickSettingsForPlatform(platform);
            },
          },
          ...(hasRecommendation
            ? [{
                id: 'quota-alert-switch',
                label: t('quotaAlert.modal.switchNow', '快捷切号到 {{email}}', {
                  email: payload.recommended_email as string,
                }),
                variant: 'primary' as const,
                autoClose: false,
                onClick: async () => {
                  try {
                    const targetAccountId = payload.recommended_account_id as string;
                    if (platform === 'codex') {
                      await useCodexAccountStore.getState().switchAccount(targetAccountId);
                      setPage('codex');
                    } else if (platform === 'github_copilot') {
                      await useGitHubCopilotAccountStore.getState().switchAccount(targetAccountId);
                      setPage('github-copilot');
                    } else if (platform === 'windsurf') {
                      await useWindsurfAccountStore.getState().switchAccount(targetAccountId);
                      setPage('windsurf');
                    } else if (platform === 'kiro') {
                      await useKiroAccountStore.getState().switchAccount(targetAccountId);
                      setPage('kiro');
                    } else {
                      await useAccountStore.getState().switchAccount(targetAccountId);
                      setPage('overview');
                    }
                    closeModal();
                  } catch (error) {
                    showModal({
                      title: t('quotaAlert.modal.switchFailedTitle', '切号失败'),
                      description: t('quotaAlert.modal.switchFailedBody', '快捷切号失败：{{error}}', {
                        error: String(error),
                      }),
                      width: 'sm',
                      actions: [
                        {
                          id: 'quota-alert-switch-failed-ok',
                          label: t('common.confirm', '确定'),
                          variant: 'primary',
                        },
                      ],
                    });
                  }
                },
              }]
            : []),
        ],
      });
    }).then((fn) => {
      if (disposed) {
        fn();
        return;
      }
      unlisten = fn;
    });

    return () => {
      disposed = true;
      if (unlisten) {
        unlisten();
      }
    };
  }, [closeModal, openQuickSettingsForPlatform, showModal, t]);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    const handleWakeupResult = (payload: WakeupTaskResultPayload) => {
      if (!payload || typeof payload.taskId !== 'string') return;

      // 更新任务的最后运行时间
      const tasksRaw = localStorage.getItem(TASKS_STORAGE_KEY);
      if (tasksRaw) {
        try {
          const tasks = JSON.parse(tasksRaw) as Array<{ id: string; lastRunAt?: number }>;
          const nextTasks = tasks.map((task) =>
            task.id === payload.taskId ? { ...task, lastRunAt: payload.lastRunAt } : task
          );
          localStorage.setItem(TASKS_STORAGE_KEY, JSON.stringify(nextTasks));
        } catch (error) {
          console.error('更新唤醒任务时间失败:', error);
        }
      }

      // 历史记录已由后端写入文件，这里只需通知前端刷新
      window.dispatchEvent(new CustomEvent('wakeup-task-result', { detail: payload }));
      window.dispatchEvent(new Event('wakeup-tasks-updated'));
    };

    listen<WakeupTaskResultPayload>('wakeup://task-result', (event) => {
      handleWakeupResult(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  useEffect(() => {
    const handleUpdateRequest = (event: Event) => {
      const detail = (event as CustomEvent<{ source?: UpdateCheckSource }>).detail;
      const source: UpdateCheckSource = detail?.source === 'manual' ? 'manual' : 'auto';
      setUpdateCheckSource(source);
      if (source === 'manual') {
        window.dispatchEvent(new CustomEvent('update-check-started', { detail: { source } }));
      }
      setUpdateNotificationKey(Date.now());
      setShowUpdateNotification(true);
    };
    window.addEventListener('update-check-requested', handleUpdateRequest as EventListener);
    return () => {
      window.removeEventListener('update-check-requested', handleUpdateRequest as EventListener);
    };
  }, []);

  const handleUpdateCheckResult = useCallback((result: UpdateCheckResult) => {
    if (result.source !== 'manual') {
      return;
    }
    window.dispatchEvent(new CustomEvent('update-check-finished', { detail: result }));
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    const refreshTasks = [
      {
        command: 'refresh_current_quota',
        errorMessage: 'Failed to refresh Antigravity quotas:',
      },
      {
        command: 'refresh_current_codex_quota',
        errorMessage: 'Failed to refresh Codex quotas:',
      },
      {
        command: 'refresh_all_github_copilot_tokens',
        errorMessage: 'Failed to refresh GitHub Copilot quotas:',
      },
      {
        command: 'refresh_all_windsurf_tokens',
        errorMessage: 'Failed to refresh Windsurf quotas:',
      },
      {
        command: 'refresh_all_kiro_tokens',
        errorMessage: 'Failed to refresh Kiro quotas:',
      },
    ] as const;

    listen('tray:refresh_quota', async () => {
      if (trayRefreshInFlightRef.current) {
        return;
      }
      trayRefreshInFlightRef.current = true;

      try {
        await Promise.all(
          refreshTasks.map(({ command, errorMessage }) =>
            invoke(command).catch((error) => {
              console.error(errorMessage, error);
            }),
          ),
        );
      } finally {
        trayRefreshInFlightRef.current = false;
      }
    }).then((fn) => { unlisten = fn; });

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    const handlePayload = (payload: unknown) => {
      if (!payload || typeof payload !== 'object') return;
      const detail = payload as AppPathMissingDetail;
      if (
        detail.app !== 'antigravity' &&
        detail.app !== 'codex' &&
        detail.app !== 'vscode' &&
        detail.app !== 'windsurf' &&
        detail.app !== 'kiro'
      ) {
        return;
      }
      setAppPathMissing(detail);
    };

    listen('app:path_missing', (event) => {
      handlePayload(event.payload);
    }).then((fn) => { unlisten = fn; });

    const handleWindowEvent = (event: Event) => {
      const custom = event as CustomEvent<AppPathMissingDetail>;
      handlePayload(custom.detail);
    };
    window.addEventListener('app-path-missing', handleWindowEvent as EventListener);

    return () => {
      if (unlisten) {
        unlisten();
      }
      window.removeEventListener('app-path-missing', handleWindowEvent as EventListener);
    };
  }, []);

  useEffect(() => {
    let active = true;
    if (!appPathMissing) {
      setAppPathDraft('');
      setAppPathDetecting(false);
      return () => {
        active = false;
      };
    }
    (async () => {
      try {
        const config = await invoke<GeneralConfig>('get_general_config');
        const currentPath =
          appPathMissing.app === 'codex'
            ? config.codex_app_path
            : appPathMissing.app === 'vscode'
              ? config.vscode_app_path
              : appPathMissing.app === 'windsurf'
                ? config.windsurf_app_path
              : appPathMissing.app === 'kiro'
                ? config.kiro_app_path
              : config.antigravity_app_path;
        if (active) {
          setAppPathDraft(currentPath || '');
        }
      } catch (error) {
        console.error('Failed to load app path config:', error);
      }
    })();
    return () => {
      active = false;
    };
  }, [appPathMissing]);

  const handlePickMissingAppPath = async () => {
    if (appPathSetting) return;
    try {
      const selected = await open({
        multiple: false,
        directory: false,
      });
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (path) {
        setAppPathDraft(path);
      }
    } catch (error) {
      console.error('选择应用路径失败:', error);
    }
  };

  const handleSaveMissingAppPath = async () => {
    if (!appPathMissing || appPathSetting || appPathDetecting) return;
    const path = appPathDraft.trim();
    if (!path) return;
    setAppPathSetting(true);
    try {
      const app = appPathMissing.app;
      const retry = appPathMissing.retry;
      await invoke('set_app_path', { app, path });
      setAppPathMissing(null);
      setAppPathSetting(false);
      if (retry?.kind === 'instance' && retry.instanceId) {
        if (app === 'codex') {
          await invoke('codex_start_instance', { instanceId: retry.instanceId });
        } else if (app === 'vscode') {
          await invoke('github_copilot_start_instance', { instanceId: retry.instanceId });
        } else if (app === 'windsurf') {
          await invoke('windsurf_start_instance', { instanceId: retry.instanceId });
        } else if (app === 'kiro') {
          await invoke('kiro_start_instance', { instanceId: retry.instanceId });
        } else {
          await invoke('start_instance', { instanceId: retry.instanceId });
        }
      } else {
        if (app === 'codex') {
          await invoke('codex_start_instance', { instanceId: '__default__' });
        } else if (app === 'vscode') {
          await invoke('github_copilot_start_instance', { instanceId: '__default__' });
        } else if (app === 'windsurf') {
          await invoke('windsurf_start_instance', { instanceId: '__default__' });
        } else if (app === 'kiro') {
          await invoke('kiro_start_instance', { instanceId: '__default__' });
        } else {
          await invoke('start_instance', { instanceId: '__default__' });
        }
      }
    } catch (error) {
      console.error('设置应用路径失败:', error);
      setAppPathSetting(false);
    }
  };

  const handleResetMissingAppPath = async () => {
    if (!appPathMissing || appPathSetting || appPathDetecting) return;
    setAppPathDetecting(true);
    try {
      const detected = await invoke<string | null>('detect_app_path', {
        app: appPathMissing.app,
        force: true,
      });
      setAppPathDraft((detected || '').trim());
    } catch (error) {
      console.error('自动探测应用路径失败:', error);
    } finally {
      setAppPathDetecting(false);
    }
  };

  // 监听窗口关闭请求事件
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    listen('window:close_requested', () => {
      setShowCloseDialog(true);
    }).then((fn) => { unlisten = fn; });

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

        listen<string>('tray:navigate', (event) => {
          const target = String(event.payload || '');
          switch (target) {
            case 'overview':
            case 'codex':
            case 'github-copilot':
            case 'windsurf':
            case 'kiro':
            case 'settings':
              setPage(target as Page);
              break;
            default:
              break;
          }
        }).then((fn) => { unlisten = fn; });

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  // 窗口拖拽处理
  const handleDragStart = () => {
    getCurrentWindow().startDragging();
  };
  const suspenseFallback = (
    <div className="loading-state">
      {t('common.loading', '加载中...')}
    </div>
  );

  return (
    <div className="app-container">
      {/* 更新通知 */}
      {showUpdateNotification && (
        <Suspense fallback={null}>
          <UpdateNotification
            key={updateNotificationKey}
            source={updateCheckSource}
            onResult={handleUpdateCheckResult}
            onClose={() => setShowUpdateNotification(false)}
          />
        </Suspense>
      )}
      <GlobalModal />

      {/* 关闭确认对话框 */}
      {showCloseDialog && (
        <Suspense fallback={null}>
          <CloseConfirmDialog onClose={() => setShowCloseDialog(false)} />
        </Suspense>
      )}

      {showBreakout && (
        <Suspense fallback={null}>
          <BreakoutModal onClose={() => setShowBreakout(false)} />
        </Suspense>
      )}

      {appPathMissing && (
        <div className="modal-overlay">
          <div className="modal" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 520 }}>
            <div className="modal-header">
              <h2>{t('appPath.missing.title', '未找到应用程序路径')}</h2>
              <button
                className="modal-close"
                onClick={() => setAppPathMissing(null)}
                aria-label={t('common.close', '关闭')}
              >
                <X />
              </button>
            </div>
            <div className="modal-body">
              <p style={{ margin: 0, color: 'var(--text-primary)' }}>
                {t('appPath.missing.desc', '未找到 {{app}} 应用程序路径，请立即设置后继续启动。', {
                  app:
                    appPathMissing.app === 'codex'
                      ? 'Codex'
                      : appPathMissing.app === 'vscode'
                        ? 'VS Code'
                        : appPathMissing.app === 'windsurf'
                          ? 'Windsurf'
                        : appPathMissing.app === 'kiro'
                          ? 'Kiro'
                        : 'Antigravity',
                })}
              </p>
              <div style={{ marginTop: 16 }}>
                <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                  <input
                    type="text"
                    className="settings-input settings-input--path"
                    value={appPathDraft}
                    placeholder={t('settings.general.codexAppPathPlaceholder', '默认路径')}
                    onChange={(e) => setAppPathDraft(e.target.value)}
                    disabled={appPathSetting}
                  />
                  <button
                    className="btn btn-secondary"
                    onClick={handlePickMissingAppPath}
                    disabled={appPathSetting || appPathDetecting}
                  >
                    {t('settings.general.codexPathSelect', '选择')}
                  </button>
                  <button
                    className="btn btn-secondary"
                    onClick={handleResetMissingAppPath}
                    disabled={appPathSetting || appPathDetecting}
                    title={
                      appPathDetecting
                        ? t('common.loading', '加载中...')
                        : (
                          appPathMissing.app === 'vscode'
                            ? t('settings.general.vscodePathReset', '重置默认')
                            : appPathMissing.app === 'windsurf'
                              ? t('settings.general.windsurfPathReset', '重置默认')
                              : appPathMissing.app === 'kiro'
                                ? t('settings.general.kiroPathReset', '重置默认')
                              : t('settings.general.codexPathReset', '重置默认')
                        )
                    }
                  >
                    <RefreshCw size={14} className={appPathDetecting ? 'spin' : undefined} />
                    {appPathDetecting
                      ? t('common.loading', '加载中...')
                      : (
                        appPathMissing.app === 'vscode'
                          ? t('settings.general.vscodePathReset', '重置默认')
                          : appPathMissing.app === 'windsurf'
                            ? t('settings.general.windsurfPathReset', '重置默认')
                            : appPathMissing.app === 'kiro'
                              ? t('settings.general.kiroPathReset', '重置默认')
                            : t('settings.general.codexPathReset', '重置默认')
                      )}
                  </button>
                </div>
              </div>
            </div>
            <div className="modal-footer">
              <button
                className="btn btn-secondary"
                onClick={() => setAppPathMissing(null)}
                disabled={appPathSetting || appPathDetecting}
              >
                {t('common.cancel', '取消')}
              </button>
              <button
                className="btn btn-primary"
                onClick={handleSaveMissingAppPath}
                disabled={appPathSetting || appPathDetecting || !appPathDraft.trim()}
              >
                {t('common.save', '保存')}
              </button>
            </div>
          </div>
        </div>
      )}
      
      {/* 顶部固定拖拽区域 */}
      <div 
        className="drag-region"
        data-tauri-drag-region 
        onMouseDown={handleDragStart}
      />
      
      {/* 左侧悬浮导航 */}
      <SideNav
        page={page}
        setPage={setPage}
        onOpenPlatformLayout={() => setShowPlatformLayoutModal(true)}
        easterEggClickCount={easterEggClickCount}
        onEasterEggTriggerClick={handleEasterEggTriggerClick}
      />

      <Suspense fallback={null}>
        <PlatformLayoutModal open={showPlatformLayoutModal} onClose={() => setShowPlatformLayoutModal(false)} />
      </Suspense>

      <div className="main-wrapper">
        {/* overview 现在是合并后的账号总览页面 */}
        <Suspense fallback={suspenseFallback}>
          {page === 'dashboard' && (
            <DashboardPage
              onNavigate={setPage}
              onOpenPlatformLayout={() => setShowPlatformLayoutModal(true)}
              onEasterEggTriggerClick={handleEasterEggTriggerClick}
            />
          )}
          {page === 'overview' && <AccountsPage onNavigate={setPage} />}
          {page === 'codex' && <CodexAccountsPage />}
          {page === 'github-copilot' && <GitHubCopilotAccountsPage />}
          {page === 'windsurf' && <WindsurfAccountsPage />}
          {page === 'kiro' && <KiroAccountsPage />}
          {page === 'instances' && <InstancesPage onNavigate={setPage} />}
          {page === 'fingerprints' && <FingerprintsPage onNavigate={setPage} />}
          {page === 'wakeup' && <WakeupTasksPage onNavigate={setPage} />}
          {page === 'settings' && <SettingsPage />}
        </Suspense>
      </div>
    </div>
  );
}

export default App;
