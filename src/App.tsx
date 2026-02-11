import { useEffect, useState } from 'react';
import './App.css';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { X } from 'lucide-react';
import { AccountsPage } from './pages/AccountsPage';
import { CodexAccountsPage } from './pages/CodexAccountsPage';
import { GitHubCopilotAccountsPage } from './pages/GitHubCopilotAccountsPage';
import { WindsurfAccountsPage } from './pages/WindsurfAccountsPage';

import { FingerprintsPage } from './pages/FingerprintsPage';
import { WakeupTasksPage } from './pages/WakeupTasksPage';
import { SettingsPage } from './pages/SettingsPage';
import { InstancesPage } from './pages/InstancesPage';
import { SideNav } from './components/layout/SideNav';
import { UpdateNotification } from './components/UpdateNotification';
import { CloseConfirmDialog } from './components/CloseConfirmDialog';
import { Page } from './types/navigation';
import { useAutoRefresh } from './hooks/useAutoRefresh';
import { changeLanguage, getCurrentLanguage, normalizeLanguage } from './i18n';

import { DashboardPage } from './pages/DashboardPage';

interface GeneralConfigTheme {
  theme: string;
}

interface GeneralConfig extends GeneralConfigTheme {
  opencode_app_path: string;
  antigravity_app_path: string;
  codex_app_path: string;
  vscode_app_path: string;
}

type AppPathMissingDetail = {
  app: 'antigravity' | 'codex' | 'vscode';
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

function App() {
  const { t } = useTranslation();
  const [page, setPage] = useState<Page>('dashboard');
  const [showUpdateNotification, setShowUpdateNotification] = useState(false);
  const [updateNotificationKey, setUpdateNotificationKey] = useState(0);
  const [showCloseDialog, setShowCloseDialog] = useState(false);
  const [appPathMissing, setAppPathMissing] = useState<AppPathMissingDetail | null>(null);
  const [appPathSetting, setAppPathSetting] = useState(false);
  const [appPathDraft, setAppPathDraft] = useState('');
  
  // 启用自动刷新 hook
  useAutoRefresh();

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
      changeLanguage(nextLanguage);
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
    const handleUpdateRequest = () => {
      setUpdateNotificationKey(Date.now());
      setShowUpdateNotification(true);
    };
    window.addEventListener('update-check-requested', handleUpdateRequest);
    return () => {
      window.removeEventListener('update-check-requested', handleUpdateRequest);
    };
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    listen('tray:refresh_quota', async () => {
      try {
        await invoke('refresh_current_quota');
      } catch (error) {
        console.error('Failed to refresh Antigravity quotas:', error);
      }
      try {
        await invoke('refresh_current_codex_quota');
      } catch (error) {
        console.error('Failed to refresh Codex quotas:', error);
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
      if (detail.app !== 'antigravity' && detail.app !== 'codex' && detail.app !== 'vscode') return;
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
    if (!appPathMissing || appPathSetting) return;
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
        } else {
          await invoke('start_instance', { instanceId: retry.instanceId });
        }
      } else {
        if (app === 'codex') {
          await invoke('codex_start_instance', { instanceId: '__default__' });
        } else if (app === 'vscode') {
          await invoke('github_copilot_start_instance', { instanceId: '__default__' });
        } else {
          await invoke('start_instance', { instanceId: '__default__' });
        }
      }
    } catch (error) {
      console.error('设置应用路径失败:', error);
      setAppPathSetting(false);
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

  return (
    <div className="app-container">
      {/* 更新通知 */}
      {showUpdateNotification && (
        <UpdateNotification key={updateNotificationKey} onClose={() => setShowUpdateNotification(false)} />
      )}

      {/* 关闭确认对话框 */}
      {showCloseDialog && (
        <CloseConfirmDialog onClose={() => setShowCloseDialog(false)} />
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
                  <button className="btn btn-secondary" onClick={handlePickMissingAppPath} disabled={appPathSetting}>
                    {t('settings.general.codexPathSelect', '选择')}
                  </button>
                </div>
              </div>
            </div>
            <div className="modal-footer">
              <button className="btn btn-secondary" onClick={() => setAppPathMissing(null)} disabled={appPathSetting}>
                {t('common.cancel', '取消')}
              </button>
              <button
                className="btn btn-primary"
                onClick={handleSaveMissingAppPath}
                disabled={appPathSetting || !appPathDraft.trim()}
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
      <SideNav page={page} setPage={setPage} />

      <div className="main-wrapper">
        {/* overview 现在是合并后的账号总览页面 */}

        {page === 'dashboard' && <DashboardPage onNavigate={setPage} />}
        {page === 'overview' && <AccountsPage onNavigate={setPage} />}
        {page === 'codex' && <CodexAccountsPage />}
        {page === 'github-copilot' && <GitHubCopilotAccountsPage />}
        {page === 'windsurf' && <WindsurfAccountsPage />}
        {page === 'instances' && <InstancesPage onNavigate={setPage} />}
        {page === 'fingerprints' && <FingerprintsPage onNavigate={setPage} />}
        {page === 'wakeup' && <WakeupTasksPage onNavigate={setPage} />}
        {page === 'settings' && <SettingsPage />}
      </div>
    </div>
  );
}

export default App;
