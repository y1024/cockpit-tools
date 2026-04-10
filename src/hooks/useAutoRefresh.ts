import { useCallback, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAccountStore } from '../stores/useAccountStore';
import { useCodexAccountStore } from '../stores/useCodexAccountStore';
import { useGitHubCopilotAccountStore } from '../stores/useGitHubCopilotAccountStore';
import { useWindsurfAccountStore } from '../stores/useWindsurfAccountStore';
import { useKiroAccountStore } from '../stores/useKiroAccountStore';
import { useCursorAccountStore } from '../stores/useCursorAccountStore';
import { useGeminiAccountStore } from '../stores/useGeminiAccountStore';
import { useCodebuddyAccountStore } from '../stores/useCodebuddyAccountStore';
import { useCodebuddyCnAccountStore } from '../stores/useCodebuddyCnAccountStore';
import { useWorkbuddyAccountStore } from '../stores/useWorkbuddyAccountStore';
import { useQoderAccountStore } from '../stores/useQoderAccountStore';
import { useTraeAccountStore } from '../stores/useTraeAccountStore';
import { useZedAccountStore } from '../stores/useZedAccountStore';
import {
  CURRENT_ACCOUNT_REFRESH_PLATFORMS,
  loadCurrentAccountRefreshMinutesMap,
} from '../utils/currentAccountRefresh';

interface GeneralConfig {
  language: string;
  theme: string;
  auto_refresh_minutes: number;
  codex_auto_refresh_minutes: number;
  ghcp_auto_refresh_minutes: number;
  windsurf_auto_refresh_minutes: number;
  kiro_auto_refresh_minutes: number;
  cursor_auto_refresh_minutes: number;
  gemini_auto_refresh_minutes: number;
  codebuddy_auto_refresh_minutes: number;
  codebuddy_cn_auto_refresh_minutes: number;
  workbuddy_auto_refresh_minutes: number;
  qoder_auto_refresh_minutes: number;
  trae_auto_refresh_minutes: number;
  zed_auto_refresh_minutes: number;
  auto_switch_enabled: boolean;
  codex_auto_switch_enabled?: boolean;
  codex_quota_alert_enabled?: boolean;
  close_behavior: string;
  opencode_app_path?: string;
  antigravity_app_path?: string;
  codex_app_path?: string;
  vscode_app_path?: string;
  windsurf_app_path?: string;
  kiro_app_path?: string;
  cursor_app_path?: string;
  codebuddy_app_path?: string;
  codebuddy_cn_app_path?: string;
  qoder_app_path?: string;
  trae_app_path?: string;
  zed_app_path?: string;
  opencode_sync_on_switch?: boolean;
  opencode_auth_overwrite_on_switch?: boolean;
  codex_launch_on_switch?: boolean;
  cursor_quota_alert_enabled?: boolean;
  cursor_quota_alert_threshold?: number;
  gemini_quota_alert_enabled?: boolean;
  gemini_quota_alert_threshold?: number;
}

const STARTUP_AUTO_REFRESH_SETUP_DELAY_MS = 2500;

export function useAutoRefresh() {
  const refreshAllQuotas = useAccountStore((state) => state.refreshAllQuotas);
  const syncCurrentFromClient = useAccountStore((state) => state.syncCurrentFromClient);
  const fetchAccounts = useAccountStore((state) => state.fetchAccounts);
  const fetchCurrentAccount = useAccountStore((state) => state.fetchCurrentAccount);

  const refreshAllCodexQuotas = useCodexAccountStore((state) => state.refreshAllQuotas);
  const fetchCodexAccounts = useCodexAccountStore((state) => state.fetchAccounts);
  const fetchCurrentCodexAccount = useCodexAccountStore((state) => state.fetchCurrentAccount);
  const refreshAllGhcpTokens = useGitHubCopilotAccountStore((state) => state.refreshAllTokens);
  const fetchCurrentGhcpAccountId = useGitHubCopilotAccountStore((state) => state.fetchCurrentAccountId);
  const refreshGhcpToken = useGitHubCopilotAccountStore((state) => state.refreshToken);
  const refreshAllWindsurfTokens = useWindsurfAccountStore((state) => state.refreshAllTokens);
  const fetchCurrentWindsurfAccountId = useWindsurfAccountStore((state) => state.fetchCurrentAccountId);
  const refreshWindsurfToken = useWindsurfAccountStore((state) => state.refreshToken);
  const refreshAllKiroTokens = useKiroAccountStore((state) => state.refreshAllTokens);
  const fetchCurrentKiroAccountId = useKiroAccountStore((state) => state.fetchCurrentAccountId);
  const refreshKiroToken = useKiroAccountStore((state) => state.refreshToken);
  const refreshAllCursorTokens = useCursorAccountStore((state) => state.refreshAllTokens);
  const fetchCurrentCursorAccountId = useCursorAccountStore((state) => state.fetchCurrentAccountId);
  const refreshCursorToken = useCursorAccountStore((state) => state.refreshToken);
  const refreshAllGeminiTokens = useGeminiAccountStore((state) => state.refreshAllTokens);
  const fetchCurrentGeminiAccountId = useGeminiAccountStore((state) => state.fetchCurrentAccountId);
  const refreshGeminiToken = useGeminiAccountStore((state) => state.refreshToken);
  const refreshAllCodebuddyTokens = useCodebuddyAccountStore((state) => state.refreshAllTokens);
  const fetchCurrentCodebuddyAccountId = useCodebuddyAccountStore((state) => state.fetchCurrentAccountId);
  const refreshCodebuddyToken = useCodebuddyAccountStore((state) => state.refreshToken);
  const refreshAllCodebuddyCnTokens = useCodebuddyCnAccountStore((state) => state.refreshAllTokens);
  const fetchCurrentCodebuddyCnAccountId = useCodebuddyCnAccountStore((state) => state.fetchCurrentAccountId);
  const refreshCodebuddyCnToken = useCodebuddyCnAccountStore((state) => state.refreshToken);
  const refreshAllWorkbuddyTokens = useWorkbuddyAccountStore((state) => state.refreshAllTokens);
  const fetchCurrentWorkbuddyAccountId = useWorkbuddyAccountStore((state) => state.fetchCurrentAccountId);
  const refreshWorkbuddyToken = useWorkbuddyAccountStore((state) => state.refreshToken);
  const refreshAllQoderTokens = useQoderAccountStore((state) => state.refreshAllTokens);
  const fetchCurrentQoderAccountId = useQoderAccountStore((state) => state.fetchCurrentAccountId);
  const refreshQoderToken = useQoderAccountStore((state) => state.refreshToken);
  const refreshAllTraeTokens = useTraeAccountStore((state) => state.refreshAllTokens);
  const fetchCurrentTraeAccountId = useTraeAccountStore((state) => state.fetchCurrentAccountId);
  const refreshTraeToken = useTraeAccountStore((state) => state.refreshToken);
  const refreshAllZedTokens = useZedAccountStore((state) => state.refreshAllTokens);
  const fetchCurrentZedAccountId = useZedAccountStore((state) => state.fetchCurrentAccountId);
  const refreshZedToken = useZedAccountStore((state) => state.refreshToken);

  const agIntervalRef = useRef<number | null>(null);
  const agCurrentIntervalRef = useRef<number | null>(null);
  const codexIntervalRef = useRef<number | null>(null);
  const codexCurrentRefreshIntervalRef = useRef<number | null>(null);
  const ghcpIntervalRef = useRef<number | null>(null);
  const ghcpCurrentIntervalRef = useRef<number | null>(null);
  const windsurfIntervalRef = useRef<number | null>(null);
  const windsurfCurrentIntervalRef = useRef<number | null>(null);
  const kiroIntervalRef = useRef<number | null>(null);
  const kiroCurrentIntervalRef = useRef<number | null>(null);
  const cursorIntervalRef = useRef<number | null>(null);
  const cursorCurrentIntervalRef = useRef<number | null>(null);
  const geminiIntervalRef = useRef<number | null>(null);
  const geminiCurrentIntervalRef = useRef<number | null>(null);
  const codebuddyIntervalRef = useRef<number | null>(null);
  const codebuddyCurrentIntervalRef = useRef<number | null>(null);
  const codebuddyCnIntervalRef = useRef<number | null>(null);
  const codebuddyCnCurrentIntervalRef = useRef<number | null>(null);
  const workbuddyIntervalRef = useRef<number | null>(null);
  const workbuddyCurrentIntervalRef = useRef<number | null>(null);
  const qoderIntervalRef = useRef<number | null>(null);
  const qoderCurrentIntervalRef = useRef<number | null>(null);
  const traeIntervalRef = useRef<number | null>(null);
  const traeCurrentIntervalRef = useRef<number | null>(null);
  const zedIntervalRef = useRef<number | null>(null);
  const zedCurrentIntervalRef = useRef<number | null>(null);

  const agRefreshingRef = useRef(false);
  const agCurrentRefreshingRef = useRef(false);
  const codexRefreshingRef = useRef(false);
  const codexCurrentRefreshingRef = useRef(false);
  const ghcpRefreshingRef = useRef(false);
  const ghcpCurrentRefreshingRef = useRef(false);
  const windsurfRefreshingRef = useRef(false);
  const windsurfCurrentRefreshingRef = useRef(false);
  const kiroRefreshingRef = useRef(false);
  const kiroCurrentRefreshingRef = useRef(false);
  const cursorRefreshingRef = useRef(false);
  const cursorCurrentRefreshingRef = useRef(false);
  const geminiRefreshingRef = useRef(false);
  const geminiCurrentRefreshingRef = useRef(false);
  const codebuddyRefreshingRef = useRef(false);
  const codebuddyCurrentRefreshingRef = useRef(false);
  const codebuddyCnRefreshingRef = useRef(false);
  const codebuddyCnCurrentRefreshingRef = useRef(false);
  const workbuddyRefreshingRef = useRef(false);
  const workbuddyCurrentRefreshingRef = useRef(false);
  const qoderRefreshingRef = useRef(false);
  const qoderCurrentRefreshingRef = useRef(false);
  const traeRefreshingRef = useRef(false);
  const traeCurrentRefreshingRef = useRef(false);
  const zedRefreshingRef = useRef(false);
  const zedCurrentRefreshingRef = useRef(false);

  const setupRunningRef = useRef(false);
  const setupPendingRef = useRef(false);
  const destroyedRef = useRef(false);

  const clearAllIntervals = useCallback(() => {
    if (agIntervalRef.current) {
      window.clearInterval(agIntervalRef.current);
      agIntervalRef.current = null;
    }
    if (agCurrentIntervalRef.current) {
      window.clearInterval(agCurrentIntervalRef.current);
      agCurrentIntervalRef.current = null;
    }
    if (codexIntervalRef.current) {
      window.clearInterval(codexIntervalRef.current);
      codexIntervalRef.current = null;
    }
    if (codexCurrentRefreshIntervalRef.current) {
      window.clearInterval(codexCurrentRefreshIntervalRef.current);
      codexCurrentRefreshIntervalRef.current = null;
    }
    if (ghcpIntervalRef.current) {
      window.clearInterval(ghcpIntervalRef.current);
      ghcpIntervalRef.current = null;
    }
    if (ghcpCurrentIntervalRef.current) {
      window.clearInterval(ghcpCurrentIntervalRef.current);
      ghcpCurrentIntervalRef.current = null;
    }
    if (windsurfIntervalRef.current) {
      window.clearInterval(windsurfIntervalRef.current);
      windsurfIntervalRef.current = null;
    }
    if (windsurfCurrentIntervalRef.current) {
      window.clearInterval(windsurfCurrentIntervalRef.current);
      windsurfCurrentIntervalRef.current = null;
    }
    if (kiroIntervalRef.current) {
      window.clearInterval(kiroIntervalRef.current);
      kiroIntervalRef.current = null;
    }
    if (kiroCurrentIntervalRef.current) {
      window.clearInterval(kiroCurrentIntervalRef.current);
      kiroCurrentIntervalRef.current = null;
    }
    if (cursorIntervalRef.current) {
      window.clearInterval(cursorIntervalRef.current);
      cursorIntervalRef.current = null;
    }
    if (cursorCurrentIntervalRef.current) {
      window.clearInterval(cursorCurrentIntervalRef.current);
      cursorCurrentIntervalRef.current = null;
    }
    if (geminiIntervalRef.current) {
      window.clearInterval(geminiIntervalRef.current);
      geminiIntervalRef.current = null;
    }
    if (geminiCurrentIntervalRef.current) {
      window.clearInterval(geminiCurrentIntervalRef.current);
      geminiCurrentIntervalRef.current = null;
    }
    if (codebuddyIntervalRef.current) {
      window.clearInterval(codebuddyIntervalRef.current);
      codebuddyIntervalRef.current = null;
    }
    if (codebuddyCurrentIntervalRef.current) {
      window.clearInterval(codebuddyCurrentIntervalRef.current);
      codebuddyCurrentIntervalRef.current = null;
    }
    if (codebuddyCnIntervalRef.current) {
      window.clearInterval(codebuddyCnIntervalRef.current);
      codebuddyCnIntervalRef.current = null;
    }
    if (codebuddyCnCurrentIntervalRef.current) {
      window.clearInterval(codebuddyCnCurrentIntervalRef.current);
      codebuddyCnCurrentIntervalRef.current = null;
    }
    if (workbuddyIntervalRef.current) {
      window.clearInterval(workbuddyIntervalRef.current);
      workbuddyIntervalRef.current = null;
    }
    if (workbuddyCurrentIntervalRef.current) {
      window.clearInterval(workbuddyCurrentIntervalRef.current);
      workbuddyCurrentIntervalRef.current = null;
    }
    if (qoderIntervalRef.current) {
      window.clearInterval(qoderIntervalRef.current);
      qoderIntervalRef.current = null;
    }
    if (qoderCurrentIntervalRef.current) {
      window.clearInterval(qoderCurrentIntervalRef.current);
      qoderCurrentIntervalRef.current = null;
    }
    if (traeIntervalRef.current) {
      window.clearInterval(traeIntervalRef.current);
      traeIntervalRef.current = null;
    }
    if (traeCurrentIntervalRef.current) {
      window.clearInterval(traeCurrentIntervalRef.current);
      traeCurrentIntervalRef.current = null;
    }
    if (zedIntervalRef.current) {
      window.clearInterval(zedIntervalRef.current);
      zedIntervalRef.current = null;
    }
    if (zedCurrentIntervalRef.current) {
      window.clearInterval(zedCurrentIntervalRef.current);
      zedCurrentIntervalRef.current = null;
    }
  }, []);

  const setupAutoRefresh = useCallback(async () => {
    const setupStartedAt = performance.now();
    console.log('[StartupPerf][AutoRefresh] setupAutoRefresh start');

    if (destroyedRef.current) {
      console.log('[StartupPerf][AutoRefresh] setupAutoRefresh aborted: destroyed flag set');
      return;
    }

    if (setupRunningRef.current) {
      setupPendingRef.current = true;
      console.log('[StartupPerf][AutoRefresh] setupAutoRefresh skipped: previous run still active');
      return;
    }

    setupRunningRef.current = true;

    try {
      do {
        setupPendingRef.current = false;

        try {
          const configInvokeStartedAt = performance.now();
          const config = await invoke<GeneralConfig>('get_general_config');
          console.log(
            `[StartupPerf][AutoRefresh] get_general_config completed in ${(performance.now() - configInvokeStartedAt).toFixed(2)}ms`,
          );
          if (destroyedRef.current) {
            console.log('[StartupPerf][AutoRefresh] setupAutoRefresh aborted after config load: destroyed flag set');
            return;
          }

          // 检测配额重置任务状态及唤醒总开关
          const wakeupEnabled = localStorage.getItem('agtools.wakeup.enabled') === 'true';
          if (wakeupEnabled) {
            const tasksJson = localStorage.getItem('agtools.wakeup.tasks');
            if (tasksJson) {
              try {
                const tasks = JSON.parse(tasksJson);
                const hasActiveResetTask = Array.isArray(tasks) && tasks.some(
                  (task: unknown) => {
                    if (!task || typeof task !== 'object') {
                      return false;
                    }
                    const taskObj = task as {
                      enabled?: boolean;
                      schedule?: { wakeOnReset?: boolean };
                    };
                    return Boolean(taskObj.enabled && taskObj.schedule?.wakeOnReset);
                  },
                );

                // 如果有活跃的重置任务，且刷新间隔为禁用(-1)或大于2分钟，则强制修正为2分钟
                if (hasActiveResetTask && (config.auto_refresh_minutes === -1 || config.auto_refresh_minutes > 2)) {
                  console.log(`[AutoRefresh] 检测到活跃的配额重置任务，自动修正刷新间隔: ${config.auto_refresh_minutes} -> 2`);
                  const saveConfigStartedAt = performance.now();
                  await invoke('save_general_config', {
                    language: config.language,
                    theme: config.theme,
                    autoRefreshMinutes: 2,
                    codexAutoRefreshMinutes: config.codex_auto_refresh_minutes,
                    ghcpAutoRefreshMinutes: config.ghcp_auto_refresh_minutes,
                    windsurfAutoRefreshMinutes: config.windsurf_auto_refresh_minutes,
                    kiroAutoRefreshMinutes: config.kiro_auto_refresh_minutes,
                    cursorAutoRefreshMinutes: config.cursor_auto_refresh_minutes,
                    geminiAutoRefreshMinutes: config.gemini_auto_refresh_minutes,
                    codebuddyAutoRefreshMinutes: config.codebuddy_auto_refresh_minutes,
                    codebuddyCnAutoRefreshMinutes: config.codebuddy_cn_auto_refresh_minutes,
                    workbuddyAutoRefreshMinutes: config.workbuddy_auto_refresh_minutes,
                    qoderAutoRefreshMinutes: config.qoder_auto_refresh_minutes,
                    traeAutoRefreshMinutes: config.trae_auto_refresh_minutes,
                    zedAutoRefreshMinutes: config.zed_auto_refresh_minutes,
                    closeBehavior: config.close_behavior || 'ask',
                    opencodeAppPath: config.opencode_app_path ?? '',
                    antigravityAppPath: config.antigravity_app_path ?? '',
                    codexAppPath: config.codex_app_path ?? '',
                    vscodeAppPath: config.vscode_app_path ?? '',
                    windsurfAppPath: config.windsurf_app_path ?? '',
                    kiroAppPath: config.kiro_app_path ?? '',
                    cursorAppPath: config.cursor_app_path ?? '',
                    codebuddyAppPath: config.codebuddy_app_path ?? '',
                    codebuddyCnAppPath: config.codebuddy_cn_app_path ?? '',
                    qoderAppPath: config.qoder_app_path ?? '',
                    traeAppPath: config.trae_app_path ?? '',
                    zedAppPath: config.zed_app_path ?? '',
                    opencodeSyncOnSwitch: config.opencode_sync_on_switch ?? false,
                    opencodeAuthOverwriteOnSwitch:
                      config.opencode_auth_overwrite_on_switch ?? false,
                    codexLaunchOnSwitch: config.codex_launch_on_switch ?? true,
                    cursorQuotaAlertEnabled: config.cursor_quota_alert_enabled ?? false,
                    cursorQuotaAlertThreshold: config.cursor_quota_alert_threshold ?? 20,
                    geminiQuotaAlertEnabled: config.gemini_quota_alert_enabled ?? false,
                    geminiQuotaAlertThreshold: config.gemini_quota_alert_threshold ?? 20,
                  });
                  console.log(
                    `[StartupPerf][AutoRefresh] save_general_config completed in ${(performance.now() - saveConfigStartedAt).toFixed(2)}ms`,
                  );
                  config.auto_refresh_minutes = 2;
                }
              } catch (e) {
                console.error('[AutoRefresh] 解析任务列表失败:', e);
              }
            }
          }

          if (destroyedRef.current) {
            console.log('[StartupPerf][AutoRefresh] setupAutoRefresh aborted before interval setup: destroyed flag set');
            return;
          }

          clearAllIntervals();

          if (config.auto_refresh_minutes > 0) {
            console.log(`[AutoRefresh] Antigravity 已启用: 每 ${config.auto_refresh_minutes} 分钟`);
            const agMs = config.auto_refresh_minutes * 60 * 1000;

            agIntervalRef.current = window.setInterval(async () => {
              if (agRefreshingRef.current) {
                return;
              }
              agRefreshingRef.current = true;

              try {
                console.log('[AutoRefresh] 触发定时配额刷新...');
                await syncCurrentFromClient();
                await refreshAllQuotas();
              } catch (e) {
                console.error('[AutoRefresh] 刷新失败:', e);
              } finally {
                agRefreshingRef.current = false;
              }
            }, agMs);
          } else {
            console.log('[AutoRefresh] Antigravity 已禁用');
          }

          if (config.codex_auto_refresh_minutes > 0) {
            console.log(`[AutoRefresh] Codex 已启用: 每 ${config.codex_auto_refresh_minutes} 分钟`);
            const codexMs = config.codex_auto_refresh_minutes * 60 * 1000;

            codexIntervalRef.current = window.setInterval(async () => {
              if (codexRefreshingRef.current) {
                return;
              }
              codexRefreshingRef.current = true;

              try {
                console.log('[AutoRefresh] 触发 Codex 配额刷新...');
                await refreshAllCodexQuotas();
              } catch (e) {
                console.error('[AutoRefresh] Codex 刷新失败:', e);
              } finally {
                codexRefreshingRef.current = false;
              }
            }, codexMs);
          } else {
            console.log('[AutoRefresh] Codex 已禁用');
          }

          const currentRefreshMinutesMap = loadCurrentAccountRefreshMinutesMap();
          const scheduleCurrentRefresh = (
            label: string,
            intervalRef: { current: number | null },
            refreshingRef: { current: boolean },
            minutes: number,
            enabled: boolean,
            run: () => Promise<void>,
          ) => {
            if (!enabled) {
              intervalRef.current = null;
              console.log(`[AutoRefresh] ${label} 当前账号刷新已禁用（配额自动刷新未开启）`);
              return;
            }
            const refreshMs = minutes * 60 * 1000;
            console.log(`[AutoRefresh] ${label} 当前账号刷新: 每 ${minutes} 分钟`);
            intervalRef.current = window.setInterval(async () => {
              if (refreshingRef.current) {
                return;
              }
              refreshingRef.current = true;
              try {
                await run();
              } catch (error) {
                console.error(`[AutoRefresh] ${label} 当前账号刷新失败:`, error);
              } finally {
                refreshingRef.current = false;
              }
            }, refreshMs);
          };

          const runProviderCurrentRefresh = async (
            fetchCurrentProviderAccountId: () => Promise<string | null>,
            refreshProviderToken: (accountId: string) => Promise<void>,
          ) => {
            const accountId = await fetchCurrentProviderAccountId();
            if (!accountId) {
              return;
            }
            await refreshProviderToken(accountId);
          };

          scheduleCurrentRefresh(
            'Antigravity',
            agCurrentIntervalRef,
            agCurrentRefreshingRef,
            currentRefreshMinutesMap.antigravity,
            config.auto_refresh_minutes > 0,
            async () => {
              if (agRefreshingRef.current) {
                return;
              }
              await syncCurrentFromClient();
              if (!useAccountStore.getState().currentAccount?.id) {
                await fetchCurrentAccount();
              }
              if (!useAccountStore.getState().currentAccount?.id) {
                return;
              }
              await invoke('refresh_current_quota');
              await fetchAccounts();
              await fetchCurrentAccount();
            },
          );

          scheduleCurrentRefresh(
            'Codex',
            codexCurrentRefreshIntervalRef,
            codexCurrentRefreshingRef,
            currentRefreshMinutesMap.codex,
            config.codex_auto_refresh_minutes > 0,
            async () => {
              if (codexRefreshingRef.current) {
                return;
              }
              if (!useCodexAccountStore.getState().currentAccount?.id) {
                await fetchCurrentCodexAccount();
              }
              if (!useCodexAccountStore.getState().currentAccount?.id) {
                return;
              }
              await invoke('refresh_current_codex_quota');
              await fetchCodexAccounts();
              await fetchCurrentCodexAccount();
            },
          );

          if (config.ghcp_auto_refresh_minutes > 0) {
            console.log(`[AutoRefresh] GitHub Copilot 已启用: 每 ${config.ghcp_auto_refresh_minutes} 分钟`);
            const ghcpMs = config.ghcp_auto_refresh_minutes * 60 * 1000;

            ghcpIntervalRef.current = window.setInterval(async () => {
              if (ghcpRefreshingRef.current) {
                return;
              }
              ghcpRefreshingRef.current = true;

              try {
                console.log('[AutoRefresh] 触发 GitHub Copilot Token 刷新...');
                await refreshAllGhcpTokens();
              } catch (e) {
                console.error('[AutoRefresh] GitHub Copilot 刷新失败:', e);
              } finally {
                ghcpRefreshingRef.current = false;
              }
            }, ghcpMs);
          } else {
            console.log('[AutoRefresh] GitHub Copilot 已禁用');
          }

          scheduleCurrentRefresh(
            'GitHub Copilot',
            ghcpCurrentIntervalRef,
            ghcpCurrentRefreshingRef,
            currentRefreshMinutesMap.ghcp,
            config.ghcp_auto_refresh_minutes > 0,
            async () => {
              if (ghcpRefreshingRef.current) {
                return;
              }
              await runProviderCurrentRefresh(fetchCurrentGhcpAccountId, refreshGhcpToken);
            },
          );

          if (config.windsurf_auto_refresh_minutes > 0) {
            console.log(`[AutoRefresh] Windsurf 已启用: 每 ${config.windsurf_auto_refresh_minutes} 分钟`);
            const windsurfMs = config.windsurf_auto_refresh_minutes * 60 * 1000;

            windsurfIntervalRef.current = window.setInterval(async () => {
              if (windsurfRefreshingRef.current) {
                return;
              }
              windsurfRefreshingRef.current = true;

              try {
                console.log('[AutoRefresh] 触发 Windsurf 配额刷新...');
                await refreshAllWindsurfTokens();
              } catch (e) {
                console.error('[AutoRefresh] Windsurf 刷新失败:', e);
              } finally {
                windsurfRefreshingRef.current = false;
              }
            }, windsurfMs);
          } else {
            console.log('[AutoRefresh] Windsurf 已禁用');
          }

          scheduleCurrentRefresh(
            'Windsurf',
            windsurfCurrentIntervalRef,
            windsurfCurrentRefreshingRef,
            currentRefreshMinutesMap.windsurf,
            config.windsurf_auto_refresh_minutes > 0,
            async () => {
              if (windsurfRefreshingRef.current) {
                return;
              }
              await runProviderCurrentRefresh(
                fetchCurrentWindsurfAccountId,
                refreshWindsurfToken,
              );
            },
          );

          if (config.kiro_auto_refresh_minutes > 0) {
            console.log(`[AutoRefresh] Kiro 已启用: 每 ${config.kiro_auto_refresh_minutes} 分钟`);
            const kiroMs = config.kiro_auto_refresh_minutes * 60 * 1000;

            kiroIntervalRef.current = window.setInterval(async () => {
              if (kiroRefreshingRef.current) {
                return;
              }
              kiroRefreshingRef.current = true;

              try {
                console.log('[AutoRefresh] 触发 Kiro 配额刷新...');
                await refreshAllKiroTokens();
              } catch (e) {
                console.error('[AutoRefresh] Kiro 刷新失败:', e);
              } finally {
                kiroRefreshingRef.current = false;
              }
            }, kiroMs);
          } else {
            console.log('[AutoRefresh] Kiro 已禁用');
          }

          scheduleCurrentRefresh(
            'Kiro',
            kiroCurrentIntervalRef,
            kiroCurrentRefreshingRef,
            currentRefreshMinutesMap.kiro,
            config.kiro_auto_refresh_minutes > 0,
            async () => {
              if (kiroRefreshingRef.current) {
                return;
              }
              await runProviderCurrentRefresh(fetchCurrentKiroAccountId, refreshKiroToken);
            },
          );

          if (config.cursor_auto_refresh_minutes > 0) {
            console.log(`[AutoRefresh] Cursor 已启用: 每 ${config.cursor_auto_refresh_minutes} 分钟`);
            const cursorMs = config.cursor_auto_refresh_minutes * 60 * 1000;

            cursorIntervalRef.current = window.setInterval(async () => {
              if (cursorRefreshingRef.current) {
                return;
              }
              cursorRefreshingRef.current = true;

              try {
                console.log('[AutoRefresh] 触发 Cursor 配额刷新...');
                await refreshAllCursorTokens();
              } catch (e) {
                console.error('[AutoRefresh] Cursor 刷新失败:', e);
              } finally {
                cursorRefreshingRef.current = false;
              }
            }, cursorMs);
          } else {
            console.log('[AutoRefresh] Cursor 已禁用');
          }

          scheduleCurrentRefresh(
            'Cursor',
            cursorCurrentIntervalRef,
            cursorCurrentRefreshingRef,
            currentRefreshMinutesMap.cursor,
            config.cursor_auto_refresh_minutes > 0,
            async () => {
              if (cursorRefreshingRef.current) {
                return;
              }
              await runProviderCurrentRefresh(fetchCurrentCursorAccountId, refreshCursorToken);
            },
          );

          if (config.gemini_auto_refresh_minutes > 0) {
            console.log(`[AutoRefresh] Gemini 已启用: 每 ${config.gemini_auto_refresh_minutes} 分钟`);
            const geminiMs = config.gemini_auto_refresh_minutes * 60 * 1000;

            geminiIntervalRef.current = window.setInterval(async () => {
              if (geminiRefreshingRef.current) {
                return;
              }
              geminiRefreshingRef.current = true;

              try {
                console.log('[AutoRefresh] 触发 Gemini 配额刷新...');
                await refreshAllGeminiTokens();
              } catch (e) {
                console.error('[AutoRefresh] Gemini 刷新失败:', e);
              } finally {
                geminiRefreshingRef.current = false;
              }
            }, geminiMs);
          } else {
            console.log('[AutoRefresh] Gemini 已禁用');
          }

          scheduleCurrentRefresh(
            'Gemini',
            geminiCurrentIntervalRef,
            geminiCurrentRefreshingRef,
            currentRefreshMinutesMap.gemini,
            config.gemini_auto_refresh_minutes > 0,
            async () => {
              if (geminiRefreshingRef.current) {
                return;
              }
              await runProviderCurrentRefresh(fetchCurrentGeminiAccountId, refreshGeminiToken);
            },
          );

          if (config.codebuddy_auto_refresh_minutes > 0) {
            console.log(`[AutoRefresh] CodeBuddy 已启用: 每 ${config.codebuddy_auto_refresh_minutes} 分钟`);
            const codebuddyMs = config.codebuddy_auto_refresh_minutes * 60 * 1000;

            codebuddyIntervalRef.current = window.setInterval(async () => {
              if (codebuddyRefreshingRef.current) {
                return;
              }
              codebuddyRefreshingRef.current = true;

              try {
                console.log('[AutoRefresh] 触发 CodeBuddy 配额刷新...');
                await refreshAllCodebuddyTokens();
              } catch (e) {
                console.error('[AutoRefresh] CodeBuddy 刷新失败:', e);
              } finally {
                codebuddyRefreshingRef.current = false;
              }
            }, codebuddyMs);
          } else {
            console.log('[AutoRefresh] CodeBuddy 已禁用');
          }

          scheduleCurrentRefresh(
            'CodeBuddy',
            codebuddyCurrentIntervalRef,
            codebuddyCurrentRefreshingRef,
            currentRefreshMinutesMap.codebuddy,
            config.codebuddy_auto_refresh_minutes > 0,
            async () => {
              if (codebuddyRefreshingRef.current) {
                return;
              }
              await runProviderCurrentRefresh(
                fetchCurrentCodebuddyAccountId,
                refreshCodebuddyToken,
              );
            },
          );

          if (config.codebuddy_cn_auto_refresh_minutes > 0) {
            console.log(
              `[AutoRefresh] CodeBuddy CN 已启用: 每 ${config.codebuddy_cn_auto_refresh_minutes} 分钟`,
            );
            const codebuddyCnMs = config.codebuddy_cn_auto_refresh_minutes * 60 * 1000;

            codebuddyCnIntervalRef.current = window.setInterval(async () => {
              if (codebuddyCnRefreshingRef.current) {
                return;
              }
              codebuddyCnRefreshingRef.current = true;

              try {
                console.log('[AutoRefresh] 触发 CodeBuddy CN 配额刷新...');
                await refreshAllCodebuddyCnTokens();
              } catch (e) {
                console.error('[AutoRefresh] CodeBuddy CN 刷新失败:', e);
              } finally {
                codebuddyCnRefreshingRef.current = false;
              }
            }, codebuddyCnMs);
          } else {
            console.log('[AutoRefresh] CodeBuddy CN 已禁用');
          }

          scheduleCurrentRefresh(
            'CodeBuddy CN',
            codebuddyCnCurrentIntervalRef,
            codebuddyCnCurrentRefreshingRef,
            currentRefreshMinutesMap.codebuddy_cn,
            config.codebuddy_cn_auto_refresh_minutes > 0,
            async () => {
              if (codebuddyCnRefreshingRef.current) {
                return;
              }
              await runProviderCurrentRefresh(
                fetchCurrentCodebuddyCnAccountId,
                refreshCodebuddyCnToken,
              );
            },
          );

          if (config.workbuddy_auto_refresh_minutes > 0) {
            console.log(`[AutoRefresh] WorkBuddy 已启用: 每 ${config.workbuddy_auto_refresh_minutes} 分钟`);
            const workbuddyMs = config.workbuddy_auto_refresh_minutes * 60 * 1000;

            workbuddyIntervalRef.current = window.setInterval(async () => {
              if (workbuddyRefreshingRef.current) {
                return;
              }
              workbuddyRefreshingRef.current = true;

              try {
                console.log('[AutoRefresh] 触发 WorkBuddy 配额刷新...');
                await refreshAllWorkbuddyTokens();
              } catch (e) {
                console.error('[AutoRefresh] WorkBuddy 刷新失败:', e);
              } finally {
                workbuddyRefreshingRef.current = false;
              }
            }, workbuddyMs);
          } else {
            console.log('[AutoRefresh] WorkBuddy 已禁用');
          }

          scheduleCurrentRefresh(
            'WorkBuddy',
            workbuddyCurrentIntervalRef,
            workbuddyCurrentRefreshingRef,
            currentRefreshMinutesMap.workbuddy,
            config.workbuddy_auto_refresh_minutes > 0,
            async () => {
              if (workbuddyRefreshingRef.current) {
                return;
              }
              await runProviderCurrentRefresh(
                fetchCurrentWorkbuddyAccountId,
                refreshWorkbuddyToken,
              );
            },
          );

          if (config.qoder_auto_refresh_minutes > 0) {
            console.log(`[AutoRefresh] Qoder 已启用: 每 ${config.qoder_auto_refresh_minutes} 分钟`);
            const qoderMs = config.qoder_auto_refresh_minutes * 60 * 1000;

            qoderIntervalRef.current = window.setInterval(async () => {
              if (qoderRefreshingRef.current) {
                return;
              }
              qoderRefreshingRef.current = true;

              try {
                console.log('[AutoRefresh] 触发 Qoder 配额刷新...');
                await refreshAllQoderTokens();
              } catch (e) {
                console.error('[AutoRefresh] Qoder 刷新失败:', e);
              } finally {
                qoderRefreshingRef.current = false;
              }
            }, qoderMs);
          } else {
            console.log('[AutoRefresh] Qoder 已禁用');
          }

          scheduleCurrentRefresh(
            'Qoder',
            qoderCurrentIntervalRef,
            qoderCurrentRefreshingRef,
            currentRefreshMinutesMap.qoder,
            config.qoder_auto_refresh_minutes > 0,
            async () => {
              if (qoderRefreshingRef.current) {
                return;
              }
              await runProviderCurrentRefresh(fetchCurrentQoderAccountId, refreshQoderToken);
            },
          );

          if (config.trae_auto_refresh_minutes > 0) {
            console.log(`[AutoRefresh] Trae 已启用: 每 ${config.trae_auto_refresh_minutes} 分钟`);
            const traeMs = config.trae_auto_refresh_minutes * 60 * 1000;

            traeIntervalRef.current = window.setInterval(async () => {
              if (traeRefreshingRef.current) {
                return;
              }
              traeRefreshingRef.current = true;

              try {
                console.log('[AutoRefresh] 触发 Trae 配额刷新...');
                await refreshAllTraeTokens();
              } catch (e) {
                console.error('[AutoRefresh] Trae 刷新失败:', e);
              } finally {
                traeRefreshingRef.current = false;
              }
            }, traeMs);
          } else {
            console.log('[AutoRefresh] Trae 已禁用');
          }

          scheduleCurrentRefresh(
            'Trae',
            traeCurrentIntervalRef,
            traeCurrentRefreshingRef,
            currentRefreshMinutesMap.trae,
            config.trae_auto_refresh_minutes > 0,
            async () => {
              if (traeRefreshingRef.current) {
                return;
              }
              await runProviderCurrentRefresh(fetchCurrentTraeAccountId, refreshTraeToken);
            },
          );

          if (config.zed_auto_refresh_minutes > 0) {
            console.log(`[AutoRefresh] Zed 已启用: 每 ${config.zed_auto_refresh_minutes} 分钟`);
            const zedMs = config.zed_auto_refresh_minutes * 60 * 1000;

            zedIntervalRef.current = window.setInterval(async () => {
              if (zedRefreshingRef.current) {
                return;
              }
              zedRefreshingRef.current = true;

              try {
                console.log('[AutoRefresh] 触发 Zed 配额刷新...');
                await refreshAllZedTokens();
              } catch (e) {
                console.error('[AutoRefresh] Zed 刷新失败:', e);
              } finally {
                zedRefreshingRef.current = false;
              }
            }, zedMs);
          } else {
            console.log('[AutoRefresh] Zed 已禁用');
          }

          scheduleCurrentRefresh(
            'Zed',
            zedCurrentIntervalRef,
            zedCurrentRefreshingRef,
            currentRefreshMinutesMap.zed,
            config.zed_auto_refresh_minutes > 0,
            async () => {
              if (zedRefreshingRef.current) {
                return;
              }
              await runProviderCurrentRefresh(fetchCurrentZedAccountId, refreshZedToken);
            },
          );

          const enabledPlatforms = [
            config.auto_refresh_minutes > 0 ? `antigravity=${config.auto_refresh_minutes}` : null,
            config.codex_auto_refresh_minutes > 0 ? `codex=${config.codex_auto_refresh_minutes}` : null,
            config.ghcp_auto_refresh_minutes > 0 ? `ghcp=${config.ghcp_auto_refresh_minutes}` : null,
            config.windsurf_auto_refresh_minutes > 0 ? `windsurf=${config.windsurf_auto_refresh_minutes}` : null,
            config.kiro_auto_refresh_minutes > 0 ? `kiro=${config.kiro_auto_refresh_minutes}` : null,
            config.cursor_auto_refresh_minutes > 0 ? `cursor=${config.cursor_auto_refresh_minutes}` : null,
            config.gemini_auto_refresh_minutes > 0 ? `gemini=${config.gemini_auto_refresh_minutes}` : null,
            config.codebuddy_auto_refresh_minutes > 0 ? `codebuddy=${config.codebuddy_auto_refresh_minutes}` : null,
            config.codebuddy_cn_auto_refresh_minutes > 0
              ? `codebuddy_cn=${config.codebuddy_cn_auto_refresh_minutes}`
              : null,
            config.workbuddy_auto_refresh_minutes > 0
              ? `workbuddy=${config.workbuddy_auto_refresh_minutes}`
              : null,
            config.qoder_auto_refresh_minutes > 0 ? `qoder=${config.qoder_auto_refresh_minutes}` : null,
            config.trae_auto_refresh_minutes > 0 ? `trae=${config.trae_auto_refresh_minutes}` : null,
            config.zed_auto_refresh_minutes > 0 ? `zed=${config.zed_auto_refresh_minutes}` : null,
            `current=${CURRENT_ACCOUNT_REFRESH_PLATFORMS.map(
              (platform) => `${platform}:${currentRefreshMinutesMap[platform]}`,
            ).join('|')}`,
          ].filter(Boolean).join(', ');
          console.log(
            `[StartupPerf][AutoRefresh] setupAutoRefresh completed in ${(performance.now() - setupStartedAt).toFixed(2)}ms; enabled=${enabledPlatforms || 'none'}`,
          );
        } catch (err) {
          console.error('[AutoRefresh] 加载配置失败:', err);
          console.error(
            `[StartupPerf][AutoRefresh] setupAutoRefresh failed after ${(performance.now() - setupStartedAt).toFixed(2)}ms:`,
            err,
          );
        }
      } while (setupPendingRef.current && !destroyedRef.current);
    } finally {
      setupRunningRef.current = false;
      console.log(
        `[StartupPerf][AutoRefresh] setupAutoRefresh exit after ${(performance.now() - setupStartedAt).toFixed(2)}ms`,
      );
    }
  }, [
    clearAllIntervals,
    fetchCodexAccounts,
    fetchCurrentCodebuddyAccountId,
    fetchCurrentCodebuddyCnAccountId,
    fetchCurrentCodexAccount,
    fetchCurrentCursorAccountId,
    fetchCurrentGeminiAccountId,
    fetchCurrentGhcpAccountId,
    fetchCurrentKiroAccountId,
    fetchCurrentQoderAccountId,
    fetchCurrentTraeAccountId,
    fetchCurrentWindsurfAccountId,
    fetchCurrentWorkbuddyAccountId,
    fetchCurrentZedAccountId,
    fetchAccounts,
    fetchCurrentAccount,
    refreshAllCodexQuotas,
    refreshAllCursorTokens,
    refreshAllGeminiTokens,
    refreshAllGhcpTokens,
    refreshAllKiroTokens,
    refreshAllCodebuddyTokens,
    refreshAllCodebuddyCnTokens,
    refreshAllWorkbuddyTokens,
    refreshAllQoderTokens,
    refreshAllTraeTokens,
    refreshAllZedTokens,
    refreshAllQuotas,
    refreshAllWindsurfTokens,
    refreshCodebuddyCnToken,
    refreshCodebuddyToken,
    refreshCursorToken,
    refreshGeminiToken,
    refreshGhcpToken,
    refreshKiroToken,
    refreshQoderToken,
    refreshTraeToken,
    refreshWindsurfToken,
    refreshWorkbuddyToken,
    refreshZedToken,
    syncCurrentFromClient,
  ]);

  useEffect(() => {
    destroyedRef.current = false;
    let startupTimer = window.setTimeout(() => {
      startupTimer = 0;
      console.log(
        `[StartupPerf][AutoRefresh] deferred startup setup triggered after ${STARTUP_AUTO_REFRESH_SETUP_DELAY_MS}ms`,
      );
      void setupAutoRefresh();
    }, STARTUP_AUTO_REFRESH_SETUP_DELAY_MS);

    const handleConfigUpdate = () => {
      if (startupTimer) {
        window.clearTimeout(startupTimer);
        startupTimer = 0;
      }
      console.log('[AutoRefresh] 检测到配置变更，重新设置定时器');
      void setupAutoRefresh();
    };

    window.addEventListener('config-updated', handleConfigUpdate);

    return () => {
      destroyedRef.current = true;
      setupPendingRef.current = false;
      if (startupTimer) {
        window.clearTimeout(startupTimer);
      }
      clearAllIntervals();
      window.removeEventListener('config-updated', handleConfigUpdate);
    };
  }, [clearAllIntervals, setupAutoRefresh]);
}
