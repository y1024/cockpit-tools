import { Suspense, lazy, useCallback, useEffect, useRef, useState } from 'react';
import './App.css';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { FolderOpen, RefreshCw, X } from 'lucide-react';
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
import type { Update as UpdaterUpdate } from '@tauri-apps/plugin-updater';
import { parseUpdaterReleaseNotes } from './utils/updaterReleaseNotes';
import {
  createUpdaterCanceledError,
  isRetryableUpdaterError,
  isUpdaterCanceledError,
  retryWithBackoff,
  sanitizeUpdaterErrorMessage,
  UPDATE_CHECK_RETRY_DELAYS_MS,
  UPDATE_DOWNLOAD_RETRY_DELAYS_MS,
} from './utils/updaterRetry';

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
const WakeupVerificationPage = lazy(() =>
  import('./pages/WakeupVerificationPage').then((module) => ({
    default: module.WakeupVerificationPage,
  })),
);
const SettingsPage = lazy(() =>
  import('./pages/SettingsPage').then((module) => ({ default: module.SettingsPage })),
);
const ManualPage = lazy(() =>
  import('./pages/ManualPage').then((module) => ({ default: module.ManualPage })),
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
const VersionJumpNotification = lazy(() =>
  import('./components/VersionJumpNotification').then((module) => ({ default: module.VersionJumpNotification })),
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
  retry?:
    | { kind: 'default' }
    | { kind: 'instance'; instanceId?: string }
    | { kind: 'switchAccount'; accountId?: string };
};

const WAKEUP_ENABLED_KEY = 'agtools.wakeup.enabled';
const TASKS_STORAGE_KEY = 'agtools.wakeup.tasks';
const WAKEUP_FORCE_DISABLE_MIGRATION_KEY = 'agtools.wakeup.migration.force_disable_0_8_14';

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
type UpdateActionState = 'hidden' | 'available' | 'downloading' | 'ready';

type UpdateAction = {
  state: UpdateActionState;
  version: string | null;
  progress: number;
  requiresInstall: boolean;
};

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
  const [hasBreakoutSession, setHasBreakoutSession] = useState(false);
  const [appPathMissing, setAppPathMissing] = useState<AppPathMissingDetail | null>(null);
  const [appPathSetting, setAppPathSetting] = useState(false);
  const [appPathDetecting, setAppPathDetecting] = useState(false);
  const [appPathDraft, setAppPathDraft] = useState('');
  const [appPathActionError, setAppPathActionError] = useState('');
  const [versionJumpInfo, setVersionJumpInfo] = useState<{
    previous_version: string;
    current_version: string;
    release_notes: string;
    release_notes_zh: string;
  } | null>(null);
  const [silentUpdateVersion, setSilentUpdateVersion] = useState<string | null>(null);
  const [updateAction, setUpdateAction] = useState<UpdateAction>({
    state: 'hidden',
    version: null,
    progress: 0,
    requiresInstall: true,
  });
  const [updateRetryStatus, setUpdateRetryStatus] = useState('');
  const [updateDownloadError, setUpdateDownloadError] = useState('');
  const [updateErrorDetails, setUpdateErrorDetails] = useState('');
  const pendingSilentUpdateRef = useRef<UpdaterUpdate | null>(null);
  const activeUpdateDownloadRef = useRef<UpdaterUpdate | null>(null);
  const updateCancelRequestedRef = useRef(false);
  const updateDownloadTaskIdRef = useRef(0);
  const updateDownloadOwnerRef = useRef<'none' | 'shared' | 'silent'>('none');
  const { showModal, closeModal } = useGlobalModal();
  const trayRefreshInFlightRef = useRef(false);
  const openBreakout = useCallback(() => {
    setHasBreakoutSession(true);
    setShowBreakout(true);
  }, []);
  const handleBreakoutMinimize = useCallback(() => {
    setShowBreakout(false);
  }, []);
  const handleBreakoutTerminate = useCallback(() => {
    setShowBreakout(false);
    setHasBreakoutSession(false);
  }, []);
  const handleResumeBreakout = useCallback(() => {
    if (!hasBreakoutSession) return;
    setShowBreakout(true);
  }, [hasBreakoutSession]);
  const {
    count: easterEggClickCount,
    registerClick: handleEasterEggTriggerClick,
    reset: resetEasterEggTrigger,
  } = useEasterEggTrigger({
    threshold: 20,
    windowMs: 8000,
    onTrigger: openBreakout,
  });
  const handleBreakoutEntryTriggerClick = useCallback(() => {
    if (hasBreakoutSession) {
      resetEasterEggTrigger();
      handleResumeBreakout();
      return;
    }
    handleEasterEggTriggerClick();
  }, [handleEasterEggTriggerClick, handleResumeBreakout, hasBreakoutSession, resetEasterEggTrigger]);
  
  // 启用自动刷新 hook
  useAutoRefresh();

  const openUpdateNotification = useCallback((source: UpdateCheckSource) => {
    setUpdateCheckSource(source);
    if (source === 'manual') {
      window.dispatchEvent(new CustomEvent('update-check-started', { detail: { source } }));
    }
    setUpdateNotificationKey(Date.now());
    setShowUpdateNotification(true);
  }, []);

  const writeUpdateLog = useCallback((level: 'info' | 'warn' | 'error', message: string) => {
    void invoke('update_log', { level, message }).catch(() => {});
  }, []);

  const closeUpdaterHandle = useCallback(async (handle: UpdaterUpdate | null | undefined) => {
    if (!handle) {
      return;
    }
    await handle.close().catch(() => {});
  }, []);

  const handleApplyPendingUpdate = useCallback(async () => {
    const targetVersion = updateAction.version || silentUpdateVersion || '';
    const shouldInstall = updateAction.state === 'ready'
      ? updateAction.requiresInstall
      : Boolean(pendingSilentUpdateRef.current);
    try {
      writeUpdateLog(
        'info',
        `用户点击立即重启应用更新: version=${targetVersion || 'unknown'}, install_before_restart=${shouldInstall}`,
      );
      const pendingUpdate = pendingSilentUpdateRef.current;
      if (shouldInstall && pendingUpdate) {
        await pendingUpdate.install();
      }
      if (pendingUpdate) {
        await pendingUpdate.close();
        pendingSilentUpdateRef.current = null;
      }
      setSilentUpdateVersion(null);
      setUpdateRetryStatus('');
      setUpdateDownloadError('');
      setUpdateErrorDetails('');
      setUpdateAction({
        state: 'hidden',
        version: null,
        progress: 0,
        requiresInstall: true,
      });
      const { relaunch } = await import('@tauri-apps/plugin-process');
      await relaunch();
    } catch (error) {
      console.error('[App] Failed to apply pending update:', error);
      writeUpdateLog('error', `用户手动应用更新失败: error=${sanitizeUpdaterErrorMessage(error)}`);
    }
  }, [silentUpdateVersion, updateAction, writeUpdateLog]);

  const runSharedUpdateDownload = useCallback(async (expectedVersion: string) => {
    const taskId = Date.now();
    updateDownloadTaskIdRef.current = taskId;
    updateCancelRequestedRef.current = false;
    updateDownloadOwnerRef.current = 'shared';
    setUpdateRetryStatus('');
    setUpdateDownloadError('');
    setUpdateErrorDetails('');
    setUpdateAction({
      state: 'downloading',
      version: expectedVersion,
      progress: 0,
      requiresInstall: true,
    });
    writeUpdateLog('info', `统一更新任务开始下载: version=${expectedVersion}`);

    let usedAttempts = 0;
    try {
      const { check } = await import('@tauri-apps/plugin-updater');
      const downloadedUpdate = await retryWithBackoff(
        async (attempt) => {
          usedAttempts = attempt;
          if (updateCancelRequestedRef.current || updateDownloadTaskIdRef.current !== taskId) {
            throw createUpdaterCanceledError();
          }

          let candidate: UpdaterUpdate | null = null;
          try {
            candidate = await check();
            if (!candidate) {
              throw new Error('No update available from updater plugin');
            }
            activeUpdateDownloadRef.current = candidate;

            const candidateVersion = candidate.version;
            const { releaseNotes, releaseNotesZh } = parseUpdaterReleaseNotes(candidate.body);
            await invoke('save_pending_update_notes', {
              version: candidateVersion,
              releaseNotes,
              releaseNotesZh,
            }).catch((error) => {
              console.error('[App] Failed to cache shared update notes:', error);
              writeUpdateLog(
                'warn',
                `缓存统一更新说明失败: version=${candidateVersion}, error=${sanitizeUpdaterErrorMessage(error)}`,
              );
            });

            let downloaded = 0;
            let contentLength = 0;
            await candidate.download((event) => {
              if (updateCancelRequestedRef.current || updateDownloadTaskIdRef.current !== taskId) {
                throw createUpdaterCanceledError();
              }
              setUpdateAction((prev) => {
                if (prev.state !== 'downloading') {
                  return prev;
                }

                if (event.event === 'Started') {
                  contentLength = event.data.contentLength ?? 0;
                  return {
                    ...prev,
                    version: candidateVersion,
                    progress: 0,
                  };
                }

                if (event.event === 'Progress') {
                  downloaded += event.data.chunkLength;
                  const nextProgress = contentLength > 0
                    ? Math.min(100, Math.round((downloaded / contentLength) * 100))
                    : Math.min(95, prev.progress + 1);
                  return {
                    ...prev,
                    version: candidateVersion,
                    progress: nextProgress,
                  };
                }

                return {
                  ...prev,
                  version: candidateVersion,
                  progress: 100,
                };
              });
            });

            if (updateCancelRequestedRef.current || updateDownloadTaskIdRef.current !== taskId) {
              throw createUpdaterCanceledError();
            }

            return candidate;
          } catch (error) {
            if (candidate) {
              await closeUpdaterHandle(candidate);
            }
            if (activeUpdateDownloadRef.current === candidate) {
              activeUpdateDownloadRef.current = null;
            }
            throw error;
          }
        },
        {
          delaysMs: UPDATE_DOWNLOAD_RETRY_DELAYS_MS,
          shouldRetry: isRetryableUpdaterError,
          onRetry: ({ retryIndex, totalRetries, delayMs, error }) => {
            const compactError = sanitizeUpdaterErrorMessage(error);
            setUpdateRetryStatus(
              t('update_notification.downloadRetrying', {
                attempt: retryIndex,
                total: totalRetries,
              }),
            );
            writeUpdateLog(
              'warn',
              `统一更新下载失败，准备重试(${retryIndex}/${totalRetries})，delay=${delayMs}ms，error=${compactError}`,
            );
            setUpdateAction((prev) => {
              if (prev.state !== 'downloading') {
                return prev;
              }
              return {
                ...prev,
                progress: 0,
              };
            });
          },
        },
      );

      if (updateCancelRequestedRef.current || updateDownloadTaskIdRef.current !== taskId) {
        await closeUpdaterHandle(downloadedUpdate);
        return;
      }

      if (pendingSilentUpdateRef.current) {
        await closeUpdaterHandle(pendingSilentUpdateRef.current);
      }
      pendingSilentUpdateRef.current = downloadedUpdate;
      activeUpdateDownloadRef.current = null;
      setSilentUpdateVersion(downloadedUpdate.version);
      setUpdateRetryStatus('');
      setUpdateDownloadError('');
      setUpdateErrorDetails('');
      setUpdateAction({
        state: 'ready',
        version: downloadedUpdate.version,
        progress: 100,
        requiresInstall: true,
      });
      writeUpdateLog('info', `统一更新下载完成，等待重启安装: version=${downloadedUpdate.version}`);
    } catch (error) {
      if (isUpdaterCanceledError(error) || updateCancelRequestedRef.current || updateDownloadTaskIdRef.current !== taskId) {
        writeUpdateLog('info', `统一更新下载已取消: version=${expectedVersion}`);
        setUpdateRetryStatus(t('update_notification.updateCancelled', '已取消更新'));
        setUpdateDownloadError('');
        setUpdateErrorDetails('');
        return;
      }

      console.error('[App] Shared update download failed:', error);
      writeUpdateLog('error', `统一更新下载失败: error=${sanitizeUpdaterErrorMessage(error)}`);
      setUpdateRetryStatus('');
      setUpdateDownloadError(
        t('update_notification.autoUpdateFailedAfterRetries', {
          count: Math.max(usedAttempts, 1),
        }),
      );
      setUpdateErrorDetails(sanitizeUpdaterErrorMessage(error));
      setUpdateAction({
        state: 'available',
        version: expectedVersion,
        progress: 0,
        requiresInstall: true,
      });
      throw error;
    } finally {
      if (updateDownloadTaskIdRef.current === taskId && updateDownloadOwnerRef.current === 'shared') {
        updateDownloadOwnerRef.current = 'none';
      }
    }
  }, [closeUpdaterHandle, t, writeUpdateLog]);

  const cancelUpdateDownload = useCallback(async () => {
    if (updateAction.state !== 'downloading') {
      return;
    }
    if (updateDownloadOwnerRef.current !== 'shared') {
      writeUpdateLog('info', '当前下载任务不支持取消（非统一更新任务）');
      return;
    }

    const version = updateAction.version;
    updateCancelRequestedRef.current = true;
    updateDownloadTaskIdRef.current += 1;
    setUpdateRetryStatus('');
    setUpdateDownloadError('');
    setUpdateErrorDetails('');

    const active = activeUpdateDownloadRef.current;
    if (active) {
      await closeUpdaterHandle(active);
      activeUpdateDownloadRef.current = null;
    }

    if (version) {
      setUpdateAction({
        state: 'available',
        version,
        progress: 0,
        requiresInstall: true,
      });
    } else {
      setUpdateAction({
        state: 'hidden',
        version: null,
        progress: 0,
        requiresInstall: true,
      });
    }
    updateDownloadOwnerRef.current = 'none';
    writeUpdateLog('info', `用户取消统一更新下载: version=${version || 'unknown'}`);
  }, [closeUpdaterHandle, updateAction.state, updateAction.version, writeUpdateLog]);

  const handleQuickUpdateActionClick = useCallback(async () => {
    if (updateAction.state === 'downloading') {
      return;
    }

    if (updateAction.state === 'ready') {
      await handleApplyPendingUpdate();
      return;
    }

    if (updateAction.state !== 'available' || !updateAction.version) {
      return;
    }

    const expectedVersion = updateAction.version;
    try {
      await runSharedUpdateDownload(expectedVersion);
    } catch (error) {
      console.error('[App] Quick update download failed:', error);
      writeUpdateLog('error', `侧边栏更新失败: error=${sanitizeUpdaterErrorMessage(error)}`);
      openUpdateNotification('manual');
    }
  }, [handleApplyPendingUpdate, openUpdateNotification, runSharedUpdateDownload, updateAction, writeUpdateLog]);

  useEffect(() => {
    return () => {
      const pendingUpdate = pendingSilentUpdateRef.current;
      if (pendingUpdate) {
        void pendingUpdate.close();
        pendingSilentUpdateRef.current = null;
      }
      const activeUpdate = activeUpdateDownloadRef.current;
      if (activeUpdate) {
        void activeUpdate.close();
        activeUpdateDownloadRef.current = null;
      }
    };
  }, []);

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
    const syncWakeupStateOnStartup = async () => {
      try {
        // 一次性迁移：升级到该版本后先将唤醒总开关置为关闭，用户仍可手动再开启
        if (localStorage.getItem(WAKEUP_FORCE_DISABLE_MIGRATION_KEY) !== '1') {
          localStorage.setItem(WAKEUP_ENABLED_KEY, 'false');
          localStorage.setItem(WAKEUP_FORCE_DISABLE_MIGRATION_KEY, '1');
        }
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
        console.log('[App] Startup update check triggered; interval gating is ignored.');
        writeUpdateLog('info', '启动触发自动更新检查流程（忽略检查周期）');

        const settings = await invoke<{
          auto_check?: boolean;
          check_interval_hours?: number;
          auto_install?: boolean;
        }>('get_update_settings');
        const autoCheck = settings?.auto_check ?? true;
        const checkIntervalHours = Number(settings?.check_interval_hours ?? 0);
        const autoInstall = settings?.auto_install ?? false;
        writeUpdateLog(
          'info',
          `读取更新设置: auto_check=${autoCheck}, check_interval_hours=${checkIntervalHours}, auto_install=${autoInstall}`,
        );

        if (!autoCheck) {
          writeUpdateLog('info', '自动检查已关闭，跳过本次启动检查');
          return;
        }

        writeUpdateLog('info', '启动检查不受检查周期限制，立即执行更新检查');

        if (autoInstall) {
          // Silent update: check and download in background, install on restart
          console.log('[App] Auto-install enabled, attempting silent update...');
          writeUpdateLog('info', '后台自动更新已开启，尝试静默检查并下载');
          try {
            const { check } = await import('@tauri-apps/plugin-updater');
            const update = await retryWithBackoff(
              async () => check(),
              {
                delaysMs: UPDATE_CHECK_RETRY_DELAYS_MS,
                shouldRetry: isRetryableUpdaterError,
                onRetry: ({ retryIndex, totalRetries, delayMs, error }) => {
                  const compactError = sanitizeUpdaterErrorMessage(error);
                  console.warn(
                    `[App] Silent update check failed, retrying (${retryIndex}/${totalRetries}) in ${delayMs}ms:`,
                    error,
                  );
                  writeUpdateLog(
                    'warn',
                    `静默更新检查失败，准备重试(${retryIndex}/${totalRetries})，delay=${delayMs}ms，error=${compactError}`,
                  );
                },
              },
            );
            if (update) {
              console.log('[App] Update found, downloading silently with retry...');
              writeUpdateLog('info', `检测到新版本，开始静默下载: version=${update.version}`);
              updateDownloadOwnerRef.current = 'silent';
              setUpdateRetryStatus('');
              setUpdateDownloadError('');
              setUpdateErrorDetails('');
              setUpdateAction((prev) => {
                if (prev.state === 'ready' && prev.version === update.version) {
                  return prev;
                }
                return {
                  state: 'downloading',
                  version: update.version,
                  progress: 0,
                  requiresInstall: true,
                };
              });
              const { releaseNotes, releaseNotesZh } = parseUpdaterReleaseNotes(update.body);
              await invoke('save_pending_update_notes', {
                version: update.version,
                releaseNotes,
                releaseNotesZh,
              }).catch((error) => {
                console.error('[App] Failed to cache silent update notes:', error);
                writeUpdateLog(
                  'warn',
                  `缓存待安装更新说明失败: version=${update.version}, error=${sanitizeUpdaterErrorMessage(error)}`,
                );
              });

              const downloadedUpdate = await retryWithBackoff(
                async (attempt) => {
                  let candidate: UpdaterUpdate | null = null;
                  try {
                    if (attempt === 1) {
                      candidate = update;
                    } else {
                      candidate = await check();
                    }

                    if (!candidate) {
                      throw new Error('No update available from updater plugin');
                    }

                    let downloaded = 0;
                    let contentLength = 0;
                    const candidateVersion = candidate.version;
                    await candidate.download((event) => {
                      setUpdateAction((prev) => {
                        if (prev.state !== 'downloading') {
                          return prev;
                        }

                        if (event.event === 'Started') {
                          contentLength = event.data.contentLength ?? 0;
                          return {
                            ...prev,
                            version: candidateVersion,
                            progress: 0,
                          };
                        }

                        if (event.event === 'Progress') {
                          downloaded += event.data.chunkLength;
                          const nextProgress = contentLength > 0
                            ? Math.min(100, Math.round((downloaded / contentLength) * 100))
                            : Math.min(95, prev.progress + 1);
                          return {
                            ...prev,
                            version: candidateVersion,
                            progress: nextProgress,
                          };
                        }

                        return {
                          ...prev,
                          version: candidateVersion,
                          progress: 100,
                        };
                      });
                    });
                    return candidate;
                  } catch (error) {
                    if (candidate) {
                      await candidate.close().catch(() => {});
                    }
                    throw error;
                  }
                },
                {
                  delaysMs: UPDATE_DOWNLOAD_RETRY_DELAYS_MS,
                  shouldRetry: isRetryableUpdaterError,
                  onRetry: ({ retryIndex, totalRetries, delayMs, error }) => {
                    const compactError = sanitizeUpdaterErrorMessage(error);
                    setUpdateRetryStatus(
                      t('update_notification.downloadRetrying', {
                        attempt: retryIndex,
                        total: totalRetries,
                      }),
                    );
                    console.warn(
                      `[App] Silent update download failed, retrying (${retryIndex}/${totalRetries}) in ${delayMs}ms:`,
                      error,
                    );
                    writeUpdateLog(
                      'warn',
                      `静默更新下载失败，准备重试(${retryIndex}/${totalRetries})，delay=${delayMs}ms，error=${compactError}`,
                    );
                    setUpdateAction((prev) => {
                      if (prev.state !== 'downloading') {
                        return prev;
                      }
                      return {
                        ...prev,
                        progress: 0,
                      };
                    });
                  },
                },
              );

              if (pendingSilentUpdateRef.current) {
                await pendingSilentUpdateRef.current.close();
              }
              pendingSilentUpdateRef.current = downloadedUpdate;
              console.log('[App] Silent download complete, waiting for restart to install.');
              writeUpdateLog(
                'info',
                `静默更新下载完成，等待用户重启应用生效: version=${downloadedUpdate.version}`,
              );
              updateDownloadOwnerRef.current = 'none';
              setUpdateRetryStatus('');
              setUpdateDownloadError('');
              setUpdateErrorDetails('');
              setSilentUpdateVersion(downloadedUpdate.version);
              setUpdateAction({
                state: 'ready',
                version: downloadedUpdate.version,
                progress: 100,
                requiresInstall: true,
              });
            } else {
              console.log('[App] No update available.');
              writeUpdateLog('info', '更新检查完成：当前已是最新版本');
              updateDownloadOwnerRef.current = 'none';
              setUpdateRetryStatus('');
              setUpdateDownloadError('');
              setUpdateErrorDetails('');
              setUpdateAction((prev) => {
                if (prev.state === 'ready') {
                  return prev;
                }
                return {
                  state: 'hidden',
                  version: null,
                  progress: 0,
                  requiresInstall: true,
                };
              });
            }
          } catch (err) {
            console.error('[App] Silent update failed, falling back to manual:', err);
            updateDownloadOwnerRef.current = 'none';
            writeUpdateLog(
              'error',
              `静默更新失败，回退到手动更新弹窗: error=${sanitizeUpdaterErrorMessage(err)}`,
            );
            // Fallback to manual update notification
            openUpdateNotification('auto');
          }
        } else {
          // Manual update: show notification dialog
          writeUpdateLog('info', '后台自动更新关闭，展示手动更新弹窗');
          openUpdateNotification('auto');
        }

        await invoke('update_last_check_time');
        writeUpdateLog('info', '已更新 last_check_time，结束本次更新检查流程');
        console.log('[App] Update check cycle completed.');
      } catch (error) {
        console.error('Failed to check update settings:', error);
        writeUpdateLog('error', `更新检查流程异常中断: error=${sanitizeUpdaterErrorMessage(error)}`);
      }
    };

    void checkUpdates();
  }, [openUpdateNotification, writeUpdateLog]);

  // Version jump detection (post-update changelog)
  useEffect(() => {
    const detectVersionJump = async () => {
      try {
        const jumpInfo = await invoke<{
          previous_version: string;
          current_version: string;
          release_notes: string;
          release_notes_zh: string;
        } | null>('check_version_jump');
        if (jumpInfo) {
          console.log('[App] Version jump detected:', jumpInfo.previous_version, '->', jumpInfo.current_version);
          setVersionJumpInfo(jumpInfo);
        }
      } catch (error) {
        console.error('Failed to check version jump:', error);
      }
    };

    const timer = setTimeout(detectVersionJump, 1000);
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
      openUpdateNotification(source);
    };
    window.addEventListener('update-check-requested', handleUpdateRequest as EventListener);
    return () => {
      window.removeEventListener('update-check-requested', handleUpdateRequest as EventListener);
    };
  }, [openUpdateNotification]);

  const handleUpdateCheckResult = useCallback((result: UpdateCheckResult) => {
    const latestVersion = result.latestVersion;
    if (result.status === 'has_update' && latestVersion) {
      setUpdateAction((prev) => {
        if (prev.state === 'downloading' && prev.version === latestVersion) {
          return prev;
        }
        if (prev.state === 'ready' && prev.version === latestVersion) {
          return prev;
        }
        return {
          state: 'available',
          version: latestVersion,
          progress: 0,
          requiresInstall: true,
        };
      });
      setUpdateRetryStatus('');
    } else if (result.status === 'up_to_date') {
      setUpdateAction((prev) => {
        if (prev.state === 'ready' || prev.state === 'downloading') {
          return prev;
        }
        return {
          state: 'hidden',
          version: null,
          progress: 0,
          requiresInstall: true,
        };
      });
      setUpdateRetryStatus('');
      setUpdateDownloadError('');
      setUpdateErrorDetails('');
    }

    if (result.source === 'manual') {
      window.dispatchEvent(new CustomEvent('update-check-finished', { detail: result }));
    }
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
      setAppPathActionError('');
      return () => {
        active = false;
      };
    }
    setAppPathActionError('');
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
        setAppPathActionError('');
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
    setAppPathActionError('');
    try {
      const app = appPathMissing.app;
      const retry = appPathMissing.retry;
      await invoke('set_app_path', { app, path });
      if (retry?.kind === 'switchAccount' && retry.accountId) {
        await invoke('switch_account', { accountId: retry.accountId });
        await Promise.allSettled([
          useAccountStore.getState().fetchAccounts(),
          useAccountStore.getState().fetchCurrentAccount(),
        ]);
      } else if (retry?.kind === 'instance' && retry.instanceId) {
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
      setAppPathMissing(null);
      setAppPathSetting(false);
    } catch (error) {
      console.error('设置应用路径失败:', error);
      setAppPathActionError(String(error));
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
      setAppPathActionError('');
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
            case 'manual':
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

  useEffect(() => {
    const handleRequestNavigate = (e: Event) => {
      const custom = e as CustomEvent<Page>;
      if (custom.detail) {
        setPage(custom.detail);
      }
    };
    window.addEventListener('app-request-navigate', handleRequestNavigate as EventListener);
    return () => {
      window.removeEventListener('app-request-navigate', handleRequestNavigate as EventListener);
    };
  }, []);
  const suspenseFallback = (
    <div className="loading-state">
      {t('common.loading', '加载中...')}
    </div>
  );

  const appPathMissingAppName = appPathMissing
    ? appPathMissing.app === 'codex'
      ? 'Codex'
      : appPathMissing.app === 'vscode'
        ? 'VS Code'
        : appPathMissing.app === 'windsurf'
          ? 'Windsurf'
          : appPathMissing.app === 'kiro'
            ? 'Kiro'
            : 'Antigravity'
    : '';

  const appPathMissingPathLabel = appPathMissing
    ? appPathMissing.app === 'codex'
      ? t('quickSettings.codex.appPath', '启动路径')
      : appPathMissing.app === 'vscode'
        ? t('quickSettings.githubCopilot.appPath', 'VS Code 路径')
        : appPathMissing.app === 'windsurf'
          ? t('quickSettings.windsurf.appPath', 'Windsurf 路径')
          : appPathMissing.app === 'kiro'
            ? t('quickSettings.kiro.appPath', 'Kiro 路径')
            : t('quickSettings.antigravity.appPath', '启动路径')
    : t('quickSettings.antigravity.appPath', '启动路径');

  return (
    <div className="app-container">
      {/* 更新通知 */}
      {showUpdateNotification && (
        <Suspense fallback={null}>
          <UpdateNotification
            key={updateNotificationKey}
            source={updateCheckSource}
            preparedUpdateVersion={updateAction.state === 'ready' ? updateAction.version : null}
            onRestartUpdate={handleApplyPendingUpdate}
            actionState={updateAction.state}
            actionVersion={updateAction.version}
            actionProgress={updateAction.progress}
            actionRetryStatus={updateRetryStatus}
            actionError={updateDownloadError}
            actionErrorDetails={updateErrorDetails}
            onPrimaryAction={handleQuickUpdateActionClick}
            onCancelUpdate={cancelUpdateDownload}
            onResult={handleUpdateCheckResult}
            onStateChange={({ phase, version }) => {
              if (phase === 'ready') {
                const keepRequiresInstall =
                  updateAction.state === 'ready' &&
                  updateAction.version === version &&
                  updateAction.requiresInstall;

                if (!keepRequiresInstall && pendingSilentUpdateRef.current) {
                  void pendingSilentUpdateRef.current.close().catch(() => {});
                  pendingSilentUpdateRef.current = null;
                }
                setUpdateRetryStatus('');
                setUpdateDownloadError('');
                setUpdateErrorDetails('');
                setUpdateAction({
                  state: 'ready',
                  version,
                  progress: 100,
                  requiresInstall: keepRequiresInstall,
                });
                return;
              }
              setUpdateAction((prev) => {
                if (prev.state === 'downloading' && prev.version === version) {
                  return prev;
                }
                if (prev.state === 'ready' && prev.version === version) {
                  return prev;
                }
                return {
                  state: 'available',
                  version,
                  progress: 0,
                  requiresInstall: true,
                };
              });
              if (updateAction.state !== 'downloading' || updateAction.version !== version) {
                setUpdateRetryStatus('');
                setUpdateDownloadError('');
                setUpdateErrorDetails('');
              }
            }}
            onClose={() => setShowUpdateNotification(false)}
          />
        </Suspense>
      )}
      {/* 版本跳跃通知（更新后首次启动） */}
      {versionJumpInfo && (
        <Suspense fallback={null}>
          <VersionJumpNotification
            info={versionJumpInfo}
            onClose={() => setVersionJumpInfo(null)}
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

      {hasBreakoutSession && (
        <Suspense fallback={null}>
          <BreakoutModal
            open={showBreakout}
            onMinimize={handleBreakoutMinimize}
            onTerminate={handleBreakoutTerminate}
          />
        </Suspense>
      )}

      {appPathMissing && (
        <div className="qs-overlay">
          <div className="qs-modal app-path-missing-modal" onClick={(e) => e.stopPropagation()}>
            <div className="qs-header">
              <span className="qs-title">{t('appPath.missing.title', '未找到应用程序路径')}</span>
              <button
                className="qs-close"
                onClick={() => setAppPathMissing(null)}
                aria-label={t('common.close', '关闭')}
              >
                <X size={16} />
              </button>
            </div>

            <div className="qs-body">
              <div className="qs-section">
                <p className="app-path-missing-desc">
                  {t('appPath.missing.desc', '未找到 {{app}} 应用程序路径，请立即设置后继续启动。', {
                    app: appPathMissingAppName,
                  })}
                </p>
              </div>

              <div className="qs-section">
                <div className="qs-section-header">
                  <FolderOpen size={15} />
                  <span>{appPathMissingPathLabel}</span>
                </div>
                <div className="qs-path-control">
                  <input
                    type="text"
                    className="qs-path-input"
                    value={appPathDraft}
                    placeholder={t('settings.general.codexAppPathPlaceholder', '默认路径')}
                    onChange={(e) => setAppPathDraft(e.target.value)}
                    disabled={appPathSetting}
                  />
                  <div className="qs-path-actions">
                    <button
                      className="qs-btn"
                      onClick={handlePickMissingAppPath}
                      disabled={appPathSetting || appPathDetecting}
                    >
                      {t('settings.general.codexPathSelect', '选择')}
                    </button>
                    <button
                      className="qs-btn"
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
                      <RefreshCw size={12} className={appPathDetecting ? 'spin' : undefined} />
                    </button>
                  </div>
                </div>
                {appPathActionError ? (
                  <p className="app-path-missing-error">
                    {t('messages.switchFailed', { error: appPathActionError })}
                  </p>
                ) : null}
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
        onEasterEggTriggerClick={handleBreakoutEntryTriggerClick}
        hasBreakoutSession={hasBreakoutSession}
        updateActionState={updateAction.state}
        updateProgress={updateAction.progress}
        onUpdateActionClick={handleQuickUpdateActionClick}
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
              onEasterEggTriggerClick={handleBreakoutEntryTriggerClick}
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
          {page === 'verification' && <WakeupVerificationPage onNavigate={setPage} />}
          {page === 'manual' && (
            <ManualPage
              onNavigate={setPage}
              onOpenPlatformLayout={() => setShowPlatformLayoutModal(true)}
            />
          )}
          {page === 'settings' && <SettingsPage />}
        </Suspense>
      </div>
    </div>
  );
}

export default App;
