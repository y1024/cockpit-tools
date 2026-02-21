import { useState, useEffect, useMemo, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { openUrl } from '@tauri-apps/plugin-opener';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { changeLanguage, getCurrentLanguage, normalizeLanguage } from '../i18n';
import * as accountService from '../services/accountService';
import { usePlatformLayoutStore } from '../stores/usePlatformLayoutStore';
import { ALL_PLATFORM_IDS, PlatformId } from '../types/platform';
import './settings/Settings.css';
import { 
  Github, User, Rocket, Save, FolderOpen,
  AlertCircle, RefreshCw, Heart, MessageSquare
} from 'lucide-react';



/** 网络配置类型 */
interface NetworkConfig {
  ws_enabled: boolean;
  ws_port: number;
  actual_port: number | null;
  default_port: number;
}

/** 通用配置类型 */
interface GeneralConfig {
  language: string;
  theme: string;
  auto_refresh_minutes: number;
  codex_auto_refresh_minutes: number;
  ghcp_auto_refresh_minutes: number;
  windsurf_auto_refresh_minutes: number;
  kiro_auto_refresh_minutes: number;
  close_behavior: 'ask' | 'minimize' | 'quit';
  opencode_app_path: string;
  antigravity_app_path: string;
  codex_app_path: string;
  vscode_app_path: string;
  windsurf_app_path: string;
  kiro_app_path: string;
  opencode_sync_on_switch: boolean;
  codex_launch_on_switch: boolean;
  auto_switch_enabled: boolean;
  auto_switch_threshold: number;
  quota_alert_enabled: boolean;
  quota_alert_threshold: number;
  codex_quota_alert_enabled: boolean;
  codex_quota_alert_threshold: number;
  ghcp_quota_alert_enabled: boolean;
  ghcp_quota_alert_threshold: number;
  windsurf_quota_alert_enabled: boolean;
  windsurf_quota_alert_threshold: number;
  kiro_quota_alert_enabled: boolean;
  kiro_quota_alert_threshold: number;
}

type AppPathTarget = 'antigravity' | 'codex' | 'vscode' | 'opencode' | 'windsurf' | 'kiro';
const REFRESH_PRESET_VALUES = ['-1', '2', '5', '10', '15'];
const THRESHOLD_PRESET_VALUES = ['0', '20', '40', '60'];
const FALLBACK_PLATFORM_SETTINGS_ORDER: Record<PlatformId, number> = {
  antigravity: 0,
  codex: 1,
  'github-copilot': 2,
  windsurf: 3,
  kiro: 4,
};
type UpdateCheckSource = 'auto' | 'manual';
type UpdateCheckFinishedDetail = {
  source: UpdateCheckSource;
  status: 'has_update' | 'up_to_date' | 'failed';
  currentVersion?: string;
  latestVersion?: string;
  error?: string;
};

export function SettingsPage() {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<'general' | 'network' | 'about'>('general');
  const orderedPlatformIds = usePlatformLayoutStore((state) => state.orderedPlatformIds);
  const platformSettingsOrder = useMemo<Record<PlatformId, number>>(() => {
    const next: Record<PlatformId, number> = { ...FALLBACK_PLATFORM_SETTINGS_ORDER };
    let order = 0;
    for (const id of orderedPlatformIds) {
      if (!ALL_PLATFORM_IDS.includes(id)) continue;
      next[id] = order;
      order += 1;
    }
    return next;
  }, [orderedPlatformIds]);

  const languageOptions = [
    { value: 'zh-cn', label: '简体中文' },
    { value: 'zh-tw', label: '繁體中文' },
    { value: 'en', label: 'English' },
    { value: 'ja', label: '日本語' },
    { value: 'ko', label: '한국어' },
    { value: 'de', label: 'Deutsch' },
    { value: 'fr', label: 'Français' },
    { value: 'es', label: 'Español' },
    { value: 'pt-br', label: 'Português (Brasil)' },
    { value: 'ru', label: 'Русский' },
    { value: 'it', label: 'Italiano' },
    { value: 'tr', label: 'Türkçe' },
    { value: 'pl', label: 'Polski' },
    { value: 'cs', label: 'Čeština' },
    { value: 'vi', label: 'Tiếng Việt' },
    { value: 'ar', label: 'العربية' },
  ];
  
  // General Settings States
  const [language, setLanguage] = useState(getCurrentLanguage());
  const [theme, setTheme] = useState('system');
  const [autoRefresh, setAutoRefresh] = useState('5');
  const [codexAutoRefresh, setCodexAutoRefresh] = useState('10');
  const [ghcpAutoRefresh, setGhcpAutoRefresh] = useState('10');
  const [windsurfAutoRefresh, setWindsurfAutoRefresh] = useState('10');
  const [kiroAutoRefresh, setKiroAutoRefresh] = useState('10');
  const [closeBehavior, setCloseBehavior] = useState<'ask' | 'minimize' | 'quit'>('ask');
  const [opencodeAppPath, setOpencodeAppPath] = useState('');
  const [antigravityAppPath, setAntigravityAppPath] = useState('');
  const [codexAppPath, setCodexAppPath] = useState('');
  const [vscodeAppPath, setVscodeAppPath] = useState('');
  const [windsurfAppPath, setWindsurfAppPath] = useState('');
  const [kiroAppPath, setKiroAppPath] = useState('');
  const [appPathResetDetectingTargets, setAppPathResetDetectingTargets] = useState<Set<AppPathTarget>>(new Set());
  const [opencodeSyncOnSwitch, setOpencodeSyncOnSwitch] = useState(true);
  const [codexLaunchOnSwitch, setCodexLaunchOnSwitch] = useState(true);
  const [autoSwitchEnabled, setAutoSwitchEnabled] = useState(false);
  const [autoSwitchThreshold, setAutoSwitchThreshold] = useState('20');
  const [quotaAlertEnabled, setQuotaAlertEnabled] = useState(false);
  const [quotaAlertThreshold, setQuotaAlertThreshold] = useState('20');
  const [codexQuotaAlertEnabled, setCodexQuotaAlertEnabled] = useState(false);
  const [codexQuotaAlertThreshold, setCodexQuotaAlertThreshold] = useState('20');
  const [ghcpQuotaAlertEnabled, setGhcpQuotaAlertEnabled] = useState(false);
  const [ghcpQuotaAlertThreshold, setGhcpQuotaAlertThreshold] = useState('20');
  const [windsurfQuotaAlertEnabled, setWindsurfQuotaAlertEnabled] = useState(false);
  const [windsurfQuotaAlertThreshold, setWindsurfQuotaAlertThreshold] = useState('20');
  const [kiroQuotaAlertEnabled, setKiroQuotaAlertEnabled] = useState(false);
  const [kiroQuotaAlertThreshold, setKiroQuotaAlertThreshold] = useState('20');
  const [autoRefreshCustomMode, setAutoRefreshCustomMode] = useState(false);
  const [codexAutoRefreshCustomMode, setCodexAutoRefreshCustomMode] = useState(false);
  const [ghcpAutoRefreshCustomMode, setGhcpAutoRefreshCustomMode] = useState(false);
  const [windsurfAutoRefreshCustomMode, setWindsurfAutoRefreshCustomMode] = useState(false);
  const [kiroAutoRefreshCustomMode, setKiroAutoRefreshCustomMode] = useState(false);
  const [autoSwitchThresholdCustomMode, setAutoSwitchThresholdCustomMode] = useState(false);
  const [quotaAlertThresholdCustomMode, setQuotaAlertThresholdCustomMode] = useState(false);
  const [codexQuotaAlertThresholdCustomMode, setCodexQuotaAlertThresholdCustomMode] = useState(false);
  const [ghcpQuotaAlertThresholdCustomMode, setGhcpQuotaAlertThresholdCustomMode] = useState(false);
  const [windsurfQuotaAlertThresholdCustomMode, setWindsurfQuotaAlertThresholdCustomMode] = useState(false);
  const [kiroQuotaAlertThresholdCustomMode, setKiroQuotaAlertThresholdCustomMode] = useState(false);
  const [generalLoaded, setGeneralLoaded] = useState(false);
  const generalSaveTimerRef = useRef<number | null>(null);
  const suppressGeneralSaveRef = useRef(false);
  
  const [appVersion, setAppVersion] = useState('');
  const [updateChecking, setUpdateChecking] = useState(false);
  const [updateCheckMessage, setUpdateCheckMessage] = useState<{
    text: string;
    tone?: 'error' | 'success';
  } | null>(null);

  useEffect(() => {
    getVersion().then(ver => setAppVersion(`v${ver}`));
  }, []);

  useEffect(() => {
    const handleStarted = (event: Event) => {
      const detail = (event as CustomEvent<{ source?: UpdateCheckSource }>).detail;
      if (detail?.source !== 'manual') {
        return;
      }
      setUpdateChecking(true);
      setUpdateCheckMessage(null);
    };

    const handleFinished = (event: Event) => {
      const detail = (event as CustomEvent<UpdateCheckFinishedDetail>).detail;
      if (!detail || detail.source !== 'manual') {
        return;
      }

      setUpdateChecking(false);

      if (detail.status === 'up_to_date') {
        const version = detail.latestVersion || detail.currentVersion;
        const upToDateText = t('settings.about.upToDate');
        setUpdateCheckMessage({
          text: version ? `${upToDateText} v${version}` : upToDateText,
          tone: 'success',
        });
        return;
      }

      if (detail.status === 'failed') {
        setUpdateCheckMessage({
          text: t('settings.about.checkFailed'),
          tone: 'error',
        });
        return;
      }

      setUpdateCheckMessage(null);
    };

    window.addEventListener('update-check-started', handleStarted as EventListener);
    window.addEventListener('update-check-finished', handleFinished as EventListener);
    return () => {
      window.removeEventListener('update-check-started', handleStarted as EventListener);
      window.removeEventListener('update-check-finished', handleFinished as EventListener);
    };
  }, [t]);
  
  // Network States
  const [wsEnabled, setWsEnabled] = useState(true);
  const [wsPort, setWsPort] = useState('19528');
  const [actualPort, setActualPort] = useState<number | null>(null);
  const [defaultPort, setDefaultPort] = useState(19528);
  const [needsRestart, setNeedsRestart] = useState(false);
  const [networkSaving, setNetworkSaving] = useState(false);
  
  // 检测配额重置任务状态
  const [hasActiveResetTasks, setHasActiveResetTasks] = useState(false);
  
  // 加载配置
  useEffect(() => {
    loadGeneralConfig();
    loadNetworkConfig();
  }, []);
  
  useEffect(() => {
    if (!generalLoaded) {
      return;
    }
    changeLanguage(language);
    applyTheme(theme);
  }, [generalLoaded, language, theme]);

  useEffect(() => {
    if (!generalLoaded) {
      return;
    }

    if (generalSaveTimerRef.current) {
      window.clearTimeout(generalSaveTimerRef.current);
    }

    if (
      !autoRefresh.trim() ||
      !codexAutoRefresh.trim() ||
      !ghcpAutoRefresh.trim() ||
      !windsurfAutoRefresh.trim() ||
      !kiroAutoRefresh.trim()
    ) {
      return;
    }

    const autoRefreshNum = parseInt(autoRefresh, 10) || -1;
    const codexAutoRefreshNum = parseInt(codexAutoRefresh, 10) || -1;
    const ghcpAutoRefreshNum = parseInt(ghcpAutoRefresh, 10) || -1;
    const windsurfAutoRefreshNum = parseInt(windsurfAutoRefresh, 10) || -1;
    const kiroAutoRefreshNum = parseInt(kiroAutoRefresh, 10) || -1;
    const parsedAutoSwitchThreshold = Number.parseInt(autoSwitchThreshold, 10);
    const parsedQuotaAlertThreshold = Number.parseInt(quotaAlertThreshold, 10);
    const parsedCodexQuotaAlertThreshold = Number.parseInt(codexQuotaAlertThreshold, 10);
    const parsedGhcpQuotaAlertThreshold = Number.parseInt(ghcpQuotaAlertThreshold, 10);
    const parsedWindsurfQuotaAlertThreshold = Number.parseInt(windsurfQuotaAlertThreshold, 10);
    const parsedKiroQuotaAlertThreshold = Number.parseInt(kiroQuotaAlertThreshold, 10);

    if (suppressGeneralSaveRef.current) {
      suppressGeneralSaveRef.current = false;
      return;
    }

    generalSaveTimerRef.current = window.setTimeout(async () => {
      try {
        await invoke('save_general_config', {
          language,
          theme,
          autoRefreshMinutes: autoRefreshNum,
          codexAutoRefreshMinutes: codexAutoRefreshNum,
          ghcpAutoRefreshMinutes: ghcpAutoRefreshNum,
          windsurfAutoRefreshMinutes: windsurfAutoRefreshNum,
          kiroAutoRefreshMinutes: kiroAutoRefreshNum,
          closeBehavior,
          opencodeAppPath,
          antigravityAppPath,
          codexAppPath,
          vscodeAppPath,
          windsurfAppPath,
          kiroAppPath,
          opencodeSyncOnSwitch,
          codexLaunchOnSwitch,
          autoSwitchEnabled,
          autoSwitchThreshold: Number.isNaN(parsedAutoSwitchThreshold) ? 20 : parsedAutoSwitchThreshold,
          quotaAlertEnabled,
          quotaAlertThreshold: Number.isNaN(parsedQuotaAlertThreshold) ? 20 : parsedQuotaAlertThreshold,
          codexQuotaAlertEnabled,
          codexQuotaAlertThreshold: Number.isNaN(parsedCodexQuotaAlertThreshold)
            ? 20
            : parsedCodexQuotaAlertThreshold,
          ghcpQuotaAlertEnabled,
          ghcpQuotaAlertThreshold: Number.isNaN(parsedGhcpQuotaAlertThreshold)
            ? 20
            : parsedGhcpQuotaAlertThreshold,
          windsurfQuotaAlertEnabled,
          windsurfQuotaAlertThreshold: Number.isNaN(parsedWindsurfQuotaAlertThreshold)
            ? 20
            : parsedWindsurfQuotaAlertThreshold,
          kiroQuotaAlertEnabled,
          kiroQuotaAlertThreshold: Number.isNaN(parsedKiroQuotaAlertThreshold)
            ? 20
            : parsedKiroQuotaAlertThreshold,
        });
        window.dispatchEvent(new Event('config-updated'));
      } catch (err) {
        console.error('保存通用配置失败:', err);
        alert(`${t('settings.network.saveFailed').replace('{error}', String(err))}`);
      }
    }, 300);

    return () => {
      if (generalSaveTimerRef.current) {
        window.clearTimeout(generalSaveTimerRef.current);
      }
    };
  }, [
    autoRefresh,
    codexAutoRefresh,
    ghcpAutoRefresh,
    windsurfAutoRefresh,
    kiroAutoRefresh,
    closeBehavior,
    generalLoaded,
    language,
    theme,
    opencodeAppPath,
    antigravityAppPath,
    codexAppPath,
    vscodeAppPath,
    windsurfAppPath,
    kiroAppPath,
    opencodeSyncOnSwitch,
    codexLaunchOnSwitch,
    autoSwitchEnabled,
    autoSwitchThreshold,
    quotaAlertEnabled,
    quotaAlertThreshold,
    codexQuotaAlertEnabled,
    codexQuotaAlertThreshold,
    ghcpQuotaAlertEnabled,
    ghcpQuotaAlertThreshold,
    windsurfQuotaAlertEnabled,
    windsurfQuotaAlertThreshold,
    kiroQuotaAlertEnabled,
    kiroQuotaAlertThreshold,
    t,
  ]);

  useEffect(() => {
    const handleLanguageUpdated = (event: Event) => {
      const detail = (event as CustomEvent<{ language?: string }>).detail;
      if (!detail?.language) {
        return;
      }
      suppressGeneralSaveRef.current = true;
      setLanguage(detail.language);
    };

    window.addEventListener('general-language-updated', handleLanguageUpdated);
    return () => {
      window.removeEventListener('general-language-updated', handleLanguageUpdated);
    };
  }, []);

  // 监听外部配置更新（如 QuickSettingsPopover 保存后同步）
  useEffect(() => {
    const handleConfigUpdated = () => {
      suppressGeneralSaveRef.current = true;
      loadGeneralConfig();
    };
    window.addEventListener('config-updated', handleConfigUpdated);
    return () => {
      window.removeEventListener('config-updated', handleConfigUpdated);
    };
  }, []);
  
  // 检测配额重置任务状态
  useEffect(() => {
    const checkResetTasks = () => {
      try {
        // 检查唤醒总开关
        const wakeupEnabledRaw = localStorage.getItem('agtools.wakeup.enabled');
        const wakeupEnabled = wakeupEnabledRaw === 'true';
        
        // 如果总开关关闭，不需要限制
        if (!wakeupEnabled) {
          setHasActiveResetTasks(false);
          return;
        }
        
        // 检查是否有启用的配额重置任务
        const tasksJson = localStorage.getItem('agtools.wakeup.tasks');
        if (!tasksJson) {
          setHasActiveResetTasks(false);
          return;
        }
        
        const tasks = JSON.parse(tasksJson);
        const hasReset = Array.isArray(tasks) && tasks.some(
          (task: any) => task.enabled && task.schedule?.wakeOnReset
        );
        setHasActiveResetTasks(hasReset);
      } catch (error) {
        console.error('检测配额重置任务失败:', error);
        setHasActiveResetTasks(false);
      }
    };
    
    // 初始检测
    checkResetTasks();
    
    // 监听存储变化
    const handleStorageChange = (e: StorageEvent) => {
      if (e.key === 'agtools.wakeup.tasks' || e.key === 'agtools.wakeup.enabled') {
        checkResetTasks();
      }
    };
    
    window.addEventListener('storage', handleStorageChange);
    
    // 监听自定义事件（同一窗口内的任务变更）
    const handleTasksUpdated = () => checkResetTasks();
    window.addEventListener('wakeup-tasks-updated', handleTasksUpdated);
    
    return () => {
      window.removeEventListener('storage', handleStorageChange);
      window.removeEventListener('wakeup-tasks-updated', handleTasksUpdated);
    };
  }, []);
  
  const applyTheme = (newTheme: string) => {
    if (newTheme === 'system') {
      const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
      document.documentElement.setAttribute('data-theme', isDark ? 'dark' : 'light');
    } else {
      document.documentElement.setAttribute('data-theme', newTheme);
    }
  };

  useEffect(() => {
    if (theme !== 'system') {
      return;
    }

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
  }, [theme]);
  
  const loadGeneralConfig = async () => {
    try {
      const config = await invoke<GeneralConfig>('get_general_config');
      setLanguage(normalizeLanguage(config.language));
      setTheme(config.theme);
      setAutoRefresh(String(config.auto_refresh_minutes));
      setCodexAutoRefresh(String(config.codex_auto_refresh_minutes ?? 10));
      setGhcpAutoRefresh(String(config.ghcp_auto_refresh_minutes ?? 10));
      setWindsurfAutoRefresh(String(config.windsurf_auto_refresh_minutes ?? 10));
      setKiroAutoRefresh(String(config.kiro_auto_refresh_minutes ?? 10));
      setCloseBehavior(config.close_behavior || 'ask');
      setOpencodeAppPath(config.opencode_app_path || '');
      setAntigravityAppPath(config.antigravity_app_path || '');
      setCodexAppPath(config.codex_app_path || '');
      setVscodeAppPath(config.vscode_app_path || '');
      setWindsurfAppPath(config.windsurf_app_path || '');
      setKiroAppPath(config.kiro_app_path || '');
      setOpencodeSyncOnSwitch(config.opencode_sync_on_switch ?? true);
      setCodexLaunchOnSwitch(config.codex_launch_on_switch ?? true);
      setAutoSwitchEnabled(config.auto_switch_enabled ?? false);
      setAutoSwitchThreshold(String(config.auto_switch_threshold ?? 20));
      setQuotaAlertEnabled(config.quota_alert_enabled ?? false);
      setQuotaAlertThreshold(String(config.quota_alert_threshold ?? 20));
      setCodexQuotaAlertEnabled(config.codex_quota_alert_enabled ?? false);
      setCodexQuotaAlertThreshold(String(config.codex_quota_alert_threshold ?? 20));
      setGhcpQuotaAlertEnabled(config.ghcp_quota_alert_enabled ?? false);
      setGhcpQuotaAlertThreshold(String(config.ghcp_quota_alert_threshold ?? 20));
      setWindsurfQuotaAlertEnabled(config.windsurf_quota_alert_enabled ?? false);
      setWindsurfQuotaAlertThreshold(String(config.windsurf_quota_alert_threshold ?? 20));
      setKiroQuotaAlertEnabled(config.kiro_quota_alert_enabled ?? false);
      setKiroQuotaAlertThreshold(String(config.kiro_quota_alert_threshold ?? 20));
      setAutoRefreshCustomMode(false);
      setCodexAutoRefreshCustomMode(false);
      setGhcpAutoRefreshCustomMode(false);
      setWindsurfAutoRefreshCustomMode(false);
      setKiroAutoRefreshCustomMode(false);
      setAutoSwitchThresholdCustomMode(false);
      setQuotaAlertThresholdCustomMode(false);
      setCodexQuotaAlertThresholdCustomMode(false);
      setGhcpQuotaAlertThresholdCustomMode(false);
      setWindsurfQuotaAlertThresholdCustomMode(false);
      setKiroQuotaAlertThresholdCustomMode(false);
      // 同步语言
      changeLanguage(config.language);
      applyTheme(config.theme);
      setGeneralLoaded(true);
    } catch (err) {
      console.error('加载通用配置失败:', err);
    }
  };
  
  const loadNetworkConfig = async () => {
    try {
      const config = await invoke<NetworkConfig>('get_network_config');
      setWsEnabled(config.ws_enabled);
      setWsPort(String(config.ws_port));
      setActualPort(config.actual_port);
      setDefaultPort(config.default_port);
      setNeedsRestart(false);
    } catch (err) {
      console.error('加载网络配置失败:', err);
    }
  };
  
  // 保存网络配置
  const handleSaveNetworkConfig = async () => {
    setNetworkSaving(true);
    try {
      const portNum = parseInt(wsPort, 10) || defaultPort;
      const result = await invoke<boolean>('save_network_config', {
        wsEnabled,
        wsPort: portNum,
      });
      
      if (result) {
        setNeedsRestart(true);
        alert(t('settings.network.saveSuccessRestart'));
      } else {
        alert(t('settings.network.saveSuccess'));
      }
    } catch (err) {
      alert(t('settings.network.saveFailed').replace('{error}', String(err)));
    } finally {
      setNetworkSaving(false);
    }
  };

  const openLink = (url: string) => {
    openUrl(url);
  };

  const isAppPathResetDetecting = (target: AppPathTarget) => appPathResetDetectingTargets.has(target);

  const setAppPathForTarget = (target: AppPathTarget, path: string) => {
    if (target === 'antigravity') {
      setAntigravityAppPath(path);
    } else if (target === 'codex') {
      setCodexAppPath(path);
    } else if (target === 'vscode') {
      setVscodeAppPath(path);
    } else if (target === 'windsurf') {
      setWindsurfAppPath(path);
    } else if (target === 'kiro') {
      setKiroAppPath(path);
    } else {
      setOpencodeAppPath(path);
    }
  };

  const getResetLabelByTarget = (target: AppPathTarget) => {
    if (target === 'vscode') {
      return t('settings.general.vscodePathReset', '重置默认');
    }
    if (target === 'windsurf') {
      return t('settings.general.windsurfPathReset', '重置默认');
    }
    if (target === 'kiro') {
      return t('settings.general.kiroPathReset', '重置默认');
    }
    if (target === 'opencode') {
      return t('settings.general.opencodePathReset', '重置默认');
    }
    return t('settings.general.codexPathReset', '重置默认');
  };

  const handlePickAppPath = async (target: AppPathTarget) => {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
      });

      const path = Array.isArray(selected) ? selected[0] : selected;
      if (!path) return;

      setAppPathForTarget(target, path);
    } catch (err) {
      console.error('选择启动路径失败:', err);
    }
  };

  const handleResetAppPath = async (target: AppPathTarget) => {
    if (isAppPathResetDetecting(target)) return;
    setAppPathResetDetectingTargets((prev) => {
      const next = new Set(prev);
      next.add(target);
      return next;
    });
    try {
      const detected = await invoke<string | null>('detect_app_path', { app: target, force: true });
      setAppPathForTarget(target, detected || '');
    } catch (err) {
      console.error('重置启动路径失败:', err);
      setAppPathForTarget(target, '');
    } finally {
      setAppPathResetDetectingTargets((prev) => {
        const next = new Set(prev);
        next.delete(target);
        return next;
      });
    }
  };

  const sanitizeNumberInput = (value: string) => value.replace(/[^\d]/g, '');

  const normalizeNumberInput = (value: string, min: number, max?: number): string => {
    const parsed = Number.parseInt(value, 10);
    if (Number.isNaN(parsed)) {
      return String(min);
    }
    const bounded = Math.max(min, max ? Math.min(parsed, max) : parsed);
    return String(bounded);
  };

  const autoRefreshIsPreset = REFRESH_PRESET_VALUES.includes(autoRefresh);
  const codexAutoRefreshIsPreset = REFRESH_PRESET_VALUES.includes(codexAutoRefresh);
  const ghcpAutoRefreshIsPreset = REFRESH_PRESET_VALUES.includes(ghcpAutoRefresh);
  const windsurfAutoRefreshIsPreset = REFRESH_PRESET_VALUES.includes(windsurfAutoRefresh);
  const kiroAutoRefreshIsPreset = REFRESH_PRESET_VALUES.includes(kiroAutoRefresh);
  const autoSwitchThresholdIsPreset = THRESHOLD_PRESET_VALUES.includes(autoSwitchThreshold);
  const quotaAlertThresholdIsPreset = THRESHOLD_PRESET_VALUES.includes(quotaAlertThreshold);
  const codexQuotaAlertThresholdIsPreset = THRESHOLD_PRESET_VALUES.includes(codexQuotaAlertThreshold);
  const ghcpQuotaAlertThresholdIsPreset = THRESHOLD_PRESET_VALUES.includes(ghcpQuotaAlertThreshold);
  const windsurfQuotaAlertThresholdIsPreset = THRESHOLD_PRESET_VALUES.includes(windsurfQuotaAlertThreshold);
  const kiroQuotaAlertThresholdIsPreset = THRESHOLD_PRESET_VALUES.includes(kiroQuotaAlertThreshold);

  // 检查更新
  const handleCheckUpdate = () => {
    if (updateChecking) {
      return;
    }
    window.dispatchEvent(
      new CustomEvent('update-check-requested', {
        detail: { source: 'manual' as UpdateCheckSource },
      }),
    );
  };

  return (
    <main className="main-content">
      <div className="page-tabs-row">
        <div className="page-tabs-label">{t('settings.title')}</div>
        <div className="page-tabs filter-tabs">
          <button 
            className={`filter-tab ${activeTab === 'general' ? 'active' : ''}`}
            onClick={() => setActiveTab('general')}
          >
            {t('settings.tabs.general')}
          </button>
          <button 
            className={`filter-tab ${activeTab === 'network' ? 'active' : ''}`}
            onClick={() => setActiveTab('network')}
          >
            {t('settings.tabs.network')}
          </button>
          <button 
            className={`filter-tab ${activeTab === 'about' ? 'active' : ''}`}
            onClick={() => setActiveTab('about')}
          >
            {t('settings.tabs.about')}
          </button>
        </div>
      </div>

      {/* 2. Content Area */}
      <div className="settings-container">
        <div className="settings-content">
        {/* === General Tab === */}
        {activeTab === 'general' && (
          <>
            <div className="group-title">{t('settings.general.commonTitle', '通用')}</div>
            <div className="settings-group">
              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.language')}</div>
                  <div className="row-desc">{t('settings.general.languageDesc')}</div>
                </div>
                <div className="row-control">
                  <select 
                    className="settings-select" 
                    value={language} 
                    onChange={(e) => setLanguage(normalizeLanguage(e.target.value))}
                  >
                    {languageOptions.map((option) => (
                      <option key={option.value} value={option.value}>{option.label}</option>
                    ))}
                  </select>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.theme')}</div>
                  <div className="row-desc">{t('settings.general.themeDesc')}</div>
                </div>
                <div className="row-control">
                  <select 
                    className="settings-select" 
                    value={theme} 
                    onChange={(e) => setTheme(e.target.value)}
                  >
                    <option value="light">{t('settings.general.themeLight')}</option>
                    <option value="dark">{t('settings.general.themeDark')}</option>
                    <option value="system">{t('settings.general.themeSystem')}</option>
                  </select>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.closeBehavior')}</div>
                  <div className="row-desc">{t('settings.general.closeBehaviorDesc')}</div>
                </div>
                <div className="row-control">
                  <select 
                    className="settings-select" 
                    value={closeBehavior} 
                    onChange={(e) => setCloseBehavior(e.target.value as 'ask' | 'minimize' | 'quit')}
                  >
                    <option value="ask">{t('settings.general.closeBehaviorAsk')}</option>
                    <option value="minimize">{t('settings.general.closeBehaviorMinimize')}</option>
                    <option value="quit">{t('settings.general.closeBehaviorQuit')}</option>
                  </select>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.dataDir')}</div>
                  <div className="row-desc">{t('settings.general.dataDirDesc')}</div>
                </div>
                <div className="row-control">
                  <button className="btn btn-secondary" onClick={() => accountService.openDataFolder()}>
                    <FolderOpen size={16} />{t('common.open')}
                  </button>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.fpDir')}</div>
                  <div className="row-desc">{t('settings.general.fpDirDesc')}</div>
                </div>
                <div className="row-control">
                  <button className="btn btn-secondary" onClick={() => accountService.openDeviceFolder()}>
                    <FolderOpen size={16} />{t('common.open')}
                  </button>
                </div>
              </div>
            </div>

            <div style={{ display: 'flex', flexDirection: 'column' }}>
              <div style={{ order: platformSettingsOrder.antigravity }}>
                <div className="group-title">{t('settings.general.antigravitySettingsTitle', 'Antigravity 设置')}</div>
                <div className="settings-group">
              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.autoRefresh')}</div>
                  <div className="row-desc">{t('settings.general.autoRefreshDesc')}</div>
                </div>
                <div className="row-control">
                  <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                    {autoRefreshCustomMode ? (
                      <div className="settings-inline-input" style={{ minWidth: '120px', width: 'auto' }}>
                        <input
                          type="number"
                          min={1}
                          max={999}
                          className="settings-select settings-select--input-mode settings-select--with-unit"
                          value={autoRefresh}
                          placeholder={t('quickSettings.inputMinutes', '输入分钟数')}
                          onChange={(e) => setAutoRefresh(sanitizeNumberInput(e.target.value))}
                        onBlur={() => {
                          const normalized = normalizeNumberInput(autoRefresh, 1, 999);
                          setAutoRefresh(normalized);
                          setAutoRefreshCustomMode(false);
                        }}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            e.preventDefault();
                            const normalized = normalizeNumberInput(autoRefresh, 1, 999);
                            setAutoRefresh(normalized);
                            setAutoRefreshCustomMode(false);
                          }
                        }}
                      />
                        <span className="settings-input-unit">{t('settings.general.minutes')}</span>
                      </div>
                    ) : (
                      <select
                        className="settings-select"
                        style={{ minWidth: '120px', width: 'auto' }}
                        value={autoRefresh}
                        onChange={(e) => {
                          const val = e.target.value;
                          if (val === 'custom') {
                            setAutoRefreshCustomMode(true);
                            setAutoRefresh(autoRefresh !== '-1' ? autoRefresh : '1');
                            return;
                          }
                          setAutoRefreshCustomMode(false);
                          setAutoRefresh(val);
                        }}
                      >
                        {!autoRefreshIsPreset && (
                          <option value={autoRefresh}>
                            {autoRefresh} {t('settings.general.minutes')}
                          </option>
                        )}
                        <option value="-1" disabled={hasActiveResetTasks}>{t('settings.general.autoRefreshDisabled')}</option>
                        <option value="2">2 {t('settings.general.minutes')}</option>
                        <option value="5" disabled={hasActiveResetTasks}>5 {t('settings.general.minutes')}</option>
                        <option value="10" disabled={hasActiveResetTasks}>10 {t('settings.general.minutes')}</option>
                        <option value="15" disabled={hasActiveResetTasks}>15 {t('settings.general.minutes')}</option>
                        <option value="custom" disabled={hasActiveResetTasks}>{t('settings.general.autoRefreshCustom')}</option>
                      </select>
                    )}
                  </div>
                  
                  {hasActiveResetTasks && (
                    <div style={{
                      display: 'flex',
                      alignItems: 'flex-start',
                      gap: '8px',
                      padding: '12px',
                      marginTop: '8px',
                      background: 'rgba(59, 130, 246, 0.1)',
                      borderRadius: '8px',
                      fontSize: '13px',
                      color: 'var(--accent)',
                      lineHeight: '1.5'
                    }}>
                      <AlertCircle size={16} style={{ marginTop: '2px', flexShrink: 0 }} />
                      <span>{t('settings.general.refreshIntervalLimited')}</span>
                    </div>
                  )}
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.antigravityAppPath', 'Antigravity 启动路径')}</div>
                  <div className="row-desc">{t('settings.general.codexAppPathDesc', '留空则使用默认路径')}</div>
                </div>
                <div className="row-control row-control--grow">
                  <div style={{ display: 'flex', gap: '8px', alignItems: 'center', flex: 1 }}>
                    <input
                      type="text"
                      className="settings-input settings-input--path"
                      value={antigravityAppPath}
                      placeholder={t('settings.general.codexAppPathPlaceholder', '默认路径')}
                      onChange={(e) => setAntigravityAppPath(e.target.value)}
                    />
                    <button
                      className="btn btn-secondary"
                      onClick={() => handlePickAppPath('antigravity')}
                      disabled={isAppPathResetDetecting('antigravity')}
                    >
                      {t('settings.general.codexPathSelect', '选择')}
                    </button>
                    <button
                      className="btn btn-secondary"
                      onClick={() => handleResetAppPath('antigravity')}
                      disabled={isAppPathResetDetecting('antigravity')}
                    >
                      <RefreshCw size={16} className={isAppPathResetDetecting('antigravity') ? 'spin' : undefined} />
                      {isAppPathResetDetecting('antigravity')
                        ? t('common.loading', '加载中...')
                        : getResetLabelByTarget('antigravity')}
                    </button>
                  </div>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('quickSettings.autoSwitch.enable', '自动切号')}</div>
                  <div className="row-desc">{t('quickSettings.autoSwitch.hint', '当任意模型配额低于阈值时，自动切换到配额最高的账号。')}</div>
                </div>
                <div className="row-control">
                  <label className="switch">
                    <input
                      type="checkbox"
                      checked={autoSwitchEnabled}
                      onChange={(e) => setAutoSwitchEnabled(e.target.checked)}
                    />
                    <span className="slider"></span>
                  </label>
                </div>
              </div>
              {autoSwitchEnabled && (
                <div className="settings-row" style={{ animation: 'fadeUp 0.3s ease both' }}>
                  <div className="row-label">
                    <div className="row-title">{t('quickSettings.autoSwitch.threshold', '切号阈值')}</div>
                    <div className="row-desc">{t('quickSettings.autoSwitch.thresholdDesc', '任意模型配额低于此百分比时触发自动切号')}</div>
                  </div>
                  <div className="row-control">
                    {autoSwitchThresholdCustomMode ? (
                      <div className="settings-inline-input">
                        <input
                          type="number"
                          min={0}
                          max={100}
                          className="settings-select settings-select--input-mode settings-select--with-unit"
                          value={autoSwitchThreshold}
                          placeholder={t('quickSettings.inputPercent', '输入百分比')}
                          onChange={(e) => setAutoSwitchThreshold(sanitizeNumberInput(e.target.value))}
                        onBlur={() => {
                          const normalized = normalizeNumberInput(autoSwitchThreshold, 0, 100);
                          setAutoSwitchThreshold(normalized);
                          setAutoSwitchThresholdCustomMode(false);
                        }}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            e.preventDefault();
                            const normalized = normalizeNumberInput(autoSwitchThreshold, 0, 100);
                            setAutoSwitchThreshold(normalized);
                            setAutoSwitchThresholdCustomMode(false);
                          }
                        }}
                      />
                        <span className="settings-input-unit">%</span>
                      </div>
                    ) : (
                      <select
                        className="settings-select"
                        value={autoSwitchThreshold}
                        onChange={(e) => {
                          const val = e.target.value;
                          if (val === 'custom') {
                            setAutoSwitchThresholdCustomMode(true);
                            setAutoSwitchThreshold(autoSwitchThreshold || '20');
                            return;
                          }
                          setAutoSwitchThresholdCustomMode(false);
                          setAutoSwitchThreshold(val);
                        }}
                      >
                        {!autoSwitchThresholdIsPreset && (
                          <option value={autoSwitchThreshold}>{autoSwitchThreshold}%</option>
                        )}
                        <option value="0">0%</option>
                        <option value="20">20%</option>
                        <option value="40">40%</option>
                        <option value="60">60%</option>
                        <option value="custom">{t('settings.general.autoRefreshCustom')}</option>
                      </select>
                    )}
                  </div>
                </div>
              )}

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('quickSettings.quotaAlert.enable', '超额预警')}</div>
                  <div className="row-desc">{t('quickSettings.quotaAlert.hint', '当当前账号任意模型配额低于阈值时，发送原生通知并在页面提示快捷切号。')}</div>
                </div>
                <div className="row-control">
                  <label className="switch">
                    <input
                      type="checkbox"
                      checked={quotaAlertEnabled}
                      onChange={(e) => setQuotaAlertEnabled(e.target.checked)}
                    />
                    <span className="slider"></span>
                  </label>
                </div>
              </div>
              {quotaAlertEnabled && (
                <div className="settings-row" style={{ animation: 'fadeUp 0.3s ease both' }}>
                  <div className="row-label">
                    <div className="row-title">{t('quickSettings.quotaAlert.threshold', '预警阈值')}</div>
                    <div className="row-desc">{t('quickSettings.quotaAlert.thresholdDesc', '任意模型配额低于此百分比时触发预警')}</div>
                  </div>
                  <div className="row-control">
                    {quotaAlertThresholdCustomMode ? (
                      <div className="settings-inline-input">
                        <input
                          type="number"
                          min={0}
                          max={100}
                          className="settings-select settings-select--input-mode settings-select--with-unit"
                          value={quotaAlertThreshold}
                          placeholder={t('quickSettings.inputPercent', '输入百分比')}
                          onChange={(e) => setQuotaAlertThreshold(sanitizeNumberInput(e.target.value))}
                          onBlur={() => {
                            const normalized = normalizeNumberInput(quotaAlertThreshold, 0, 100);
                            setQuotaAlertThreshold(normalized);
                            setQuotaAlertThresholdCustomMode(false);
                          }}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') {
                              e.preventDefault();
                              const normalized = normalizeNumberInput(quotaAlertThreshold, 0, 100);
                              setQuotaAlertThreshold(normalized);
                              setQuotaAlertThresholdCustomMode(false);
                            }
                          }}
                        />
                        <span className="settings-input-unit">%</span>
                      </div>
                    ) : (
                      <select
                        className="settings-select"
                        value={quotaAlertThreshold}
                        onChange={(e) => {
                          const val = e.target.value;
                          if (val === 'custom') {
                            setQuotaAlertThresholdCustomMode(true);
                            setQuotaAlertThreshold(quotaAlertThreshold || '20');
                            return;
                          }
                          setQuotaAlertThresholdCustomMode(false);
                          setQuotaAlertThreshold(val);
                        }}
                      >
                        {!quotaAlertThresholdIsPreset && (
                          <option value={quotaAlertThreshold}>{quotaAlertThreshold}%</option>
                        )}
                        <option value="0">0%</option>
                        <option value="20">20%</option>
                        <option value="40">40%</option>
                        <option value="60">60%</option>
                        <option value="custom">{t('settings.general.autoRefreshCustom')}</option>
                      </select>
                    )}
                  </div>
                </div>
              )}
            </div>

              </div>

              <div style={{ order: platformSettingsOrder.codex }}>
                <div className="group-title">{t('settings.general.codexSettingsTitle', 'Codex 设置')}</div>
                <div className="settings-group">
              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.codexAutoRefresh')}</div>
                  <div className="row-desc">{t('settings.general.codexAutoRefreshDesc')}</div>
                </div>
                <div className="row-control">
                  <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                    {codexAutoRefreshCustomMode ? (
                      <div className="settings-inline-input" style={{ minWidth: '120px', width: 'auto' }}>
                        <input
                          type="number"
                          min={1}
                          max={999}
                          className="settings-select settings-select--input-mode settings-select--with-unit"
                          value={codexAutoRefresh}
                          placeholder={t('quickSettings.inputMinutes', '输入分钟数')}
                          onChange={(e) => setCodexAutoRefresh(sanitizeNumberInput(e.target.value))}
                        onBlur={() => {
                          const normalized = normalizeNumberInput(codexAutoRefresh, 1, 999);
                          setCodexAutoRefresh(normalized);
                          setCodexAutoRefreshCustomMode(false);
                        }}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            e.preventDefault();
                            const normalized = normalizeNumberInput(codexAutoRefresh, 1, 999);
                            setCodexAutoRefresh(normalized);
                            setCodexAutoRefreshCustomMode(false);
                          }
                        }}
                      />
                        <span className="settings-input-unit">{t('settings.general.minutes')}</span>
                      </div>
                    ) : (
                      <select
                        className="settings-select"
                        style={{ minWidth: '120px', width: 'auto' }}
                        value={codexAutoRefresh}
                        onChange={(e) => {
                          const val = e.target.value;
                          if (val === 'custom') {
                            setCodexAutoRefreshCustomMode(true);
                            setCodexAutoRefresh(codexAutoRefresh !== '-1' ? codexAutoRefresh : '1');
                            return;
                          }
                          setCodexAutoRefreshCustomMode(false);
                          setCodexAutoRefresh(val);
                        }}
                      >
                        {!codexAutoRefreshIsPreset && (
                          <option value={codexAutoRefresh}>
                            {codexAutoRefresh} {t('settings.general.minutes')}
                          </option>
                        )}
                        <option value="-1">{t('settings.general.autoRefreshDisabled')}</option>
                        <option value="2">2 {t('settings.general.minutes')}</option>
                        <option value="5">5 {t('settings.general.minutes')}</option>
                        <option value="10">10 {t('settings.general.minutes')}</option>
                        <option value="15">15 {t('settings.general.minutes')}</option>
                        <option value="custom">{t('settings.general.autoRefreshCustom')}</option>
                      </select>
                    )}
                  </div>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.codexAppPath', 'Codex 启动路径')}</div>
                  <div className="row-desc">{t('settings.general.codexAppPathDesc', '留空则使用默认路径')}</div>
                </div>
                <div className="row-control row-control--grow">
                  <div style={{ display: 'flex', gap: '8px', alignItems: 'center', flex: 1 }}>
                    <input
                      type="text"
                      className="settings-input settings-input--path"
                      value={codexAppPath}
                      placeholder={t('settings.general.codexAppPathPlaceholder', '默认路径')}
                      onChange={(e) => setCodexAppPath(e.target.value)}
                    />
                    <button
                      className="btn btn-secondary"
                      onClick={() => handlePickAppPath('codex')}
                      disabled={isAppPathResetDetecting('codex')}
                    >
                      {t('settings.general.codexPathSelect', '选择')}
                    </button>
                    <button
                      className="btn btn-secondary"
                      onClick={() => handleResetAppPath('codex')}
                      disabled={isAppPathResetDetecting('codex')}
                    >
                      <RefreshCw size={16} className={isAppPathResetDetecting('codex') ? 'spin' : undefined} />
                      {isAppPathResetDetecting('codex')
                        ? t('common.loading', '加载中...')
                        : getResetLabelByTarget('codex')}
                    </button>
                  </div>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.codexLaunchOnSwitch', '切换 Codex 时自动启动 Codex App')}</div>
                  <div className="row-desc">{t('settings.general.codexLaunchOnSwitchDesc', '切换账号后自动启动或重启 Codex App')}</div>
                </div>
                <div className="row-control">
                  <label className="switch">
                    <input
                      type="checkbox"
                      checked={codexLaunchOnSwitch}
                      onChange={(e) => setCodexLaunchOnSwitch(e.target.checked)}
                    />
                    <span className="slider"></span>
                  </label>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.opencodeRestart')}</div>
                  <div className="row-desc">{t('settings.general.opencodeRestartDesc')}</div>
                </div>
                <div className="row-control">
                  <label className="switch">
                    <input
                      type="checkbox"
                      checked={opencodeSyncOnSwitch}
                      onChange={(e) => setOpencodeSyncOnSwitch(e.target.checked)}
                    />
                    <span className="slider"></span>
                  </label>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.opencodeAppPath')}</div>
                  <div className="row-desc">
                    {t('settings.general.opencodeAppPathDesc')}
                  </div>
                </div>
                <div className="row-control row-control--grow">
                  <div style={{ display: 'flex', gap: '8px', alignItems: 'center', flex: 1 }}>
                    <input
                      type="text"
                      className="settings-input settings-input--path"
                      value={opencodeAppPath}
                      placeholder={t('settings.general.opencodeAppPathPlaceholder')}
                      onChange={(e) => setOpencodeAppPath(e.target.value)}
                    />
                    <button
                      className="btn btn-secondary"
                      onClick={() => handlePickAppPath('opencode')}
                      disabled={isAppPathResetDetecting('opencode')}
                    >
                      {t('settings.general.opencodePathSelect', '选择')}
                    </button>
                    <button
                      className="btn btn-secondary"
                      onClick={() => handleResetAppPath('opencode')}
                      disabled={isAppPathResetDetecting('opencode')}
                    >
                      <RefreshCw size={16} className={isAppPathResetDetecting('opencode') ? 'spin' : undefined} />
                      {isAppPathResetDetecting('opencode')
                        ? t('common.loading', '加载中...')
                        : getResetLabelByTarget('opencode')}
                    </button>
                  </div>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('quickSettings.quotaAlert.enable', '超额预警')}</div>
                  <div className="row-desc">{t('quickSettings.quotaAlert.hint', '当当前账号任意模型配额低于阈值时，发送原生通知并在页面提示快捷切号。')}</div>
                </div>
                <div className="row-control">
                  <label className="switch">
                    <input
                      type="checkbox"
                      checked={codexQuotaAlertEnabled}
                      onChange={(e) => setCodexQuotaAlertEnabled(e.target.checked)}
                    />
                    <span className="slider"></span>
                  </label>
                </div>
              </div>
              {codexQuotaAlertEnabled && (
                <div className="settings-row" style={{ animation: 'fadeUp 0.3s ease both' }}>
                  <div className="row-label">
                    <div className="row-title">{t('quickSettings.quotaAlert.threshold', '预警阈值')}</div>
                    <div className="row-desc">{t('quickSettings.quotaAlert.thresholdDesc', '任意模型配额低于此百分比时触发预警')}</div>
                  </div>
                  <div className="row-control">
                    {codexQuotaAlertThresholdCustomMode ? (
                      <div className="settings-inline-input">
                        <input
                          type="number"
                          min={0}
                          max={100}
                          className="settings-select settings-select--input-mode settings-select--with-unit"
                          value={codexQuotaAlertThreshold}
                          placeholder={t('quickSettings.inputPercent', '输入百分比')}
                          onChange={(e) => setCodexQuotaAlertThreshold(sanitizeNumberInput(e.target.value))}
                          onBlur={() => {
                            const normalized = normalizeNumberInput(codexQuotaAlertThreshold, 0, 100);
                            setCodexQuotaAlertThreshold(normalized);
                            setCodexQuotaAlertThresholdCustomMode(false);
                          }}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') {
                              e.preventDefault();
                              const normalized = normalizeNumberInput(codexQuotaAlertThreshold, 0, 100);
                              setCodexQuotaAlertThreshold(normalized);
                              setCodexQuotaAlertThresholdCustomMode(false);
                            }
                          }}
                        />
                        <span className="settings-input-unit">%</span>
                      </div>
                    ) : (
                      <select
                        className="settings-select"
                        value={codexQuotaAlertThreshold}
                        onChange={(e) => {
                          const val = e.target.value;
                          if (val === 'custom') {
                            setCodexQuotaAlertThresholdCustomMode(true);
                            setCodexQuotaAlertThreshold(codexQuotaAlertThreshold || '20');
                            return;
                          }
                          setCodexQuotaAlertThresholdCustomMode(false);
                          setCodexQuotaAlertThreshold(val);
                        }}
                      >
                        {!codexQuotaAlertThresholdIsPreset && (
                          <option value={codexQuotaAlertThreshold}>{codexQuotaAlertThreshold}%</option>
                        )}
                        <option value="0">0%</option>
                        <option value="20">20%</option>
                        <option value="40">40%</option>
                        <option value="60">60%</option>
                        <option value="custom">{t('settings.general.autoRefreshCustom')}</option>
                      </select>
                    )}
                  </div>
                </div>
              )}
            </div>

              </div>

              <div style={{ order: platformSettingsOrder['github-copilot'] }}>
                <div className="group-title">{t('settings.general.githubCopilotSettingsTitle', 'GitHub Copilot 设置')}</div>
                <div className="settings-group">
              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.ghcpAutoRefresh', 'GitHub Copilot 自动刷新配额')}</div>
                  <div className="row-desc">{t('settings.general.ghcpAutoRefreshDesc', '后台自动更新频率')}</div>
                </div>
                <div className="row-control">
                  <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                    {ghcpAutoRefreshCustomMode ? (
                      <div className="settings-inline-input" style={{ minWidth: '120px', width: 'auto' }}>
                        <input
                          type="number"
                          min={1}
                          max={999}
                          className="settings-select settings-select--input-mode settings-select--with-unit"
                          value={ghcpAutoRefresh}
                          placeholder={t('quickSettings.inputMinutes', '输入分钟数')}
                          onChange={(e) => setGhcpAutoRefresh(sanitizeNumberInput(e.target.value))}
                        onBlur={() => {
                          const normalized = normalizeNumberInput(ghcpAutoRefresh, 1, 999);
                          setGhcpAutoRefresh(normalized);
                          setGhcpAutoRefreshCustomMode(false);
                        }}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            e.preventDefault();
                            const normalized = normalizeNumberInput(ghcpAutoRefresh, 1, 999);
                            setGhcpAutoRefresh(normalized);
                            setGhcpAutoRefreshCustomMode(false);
                          }
                        }}
                      />
                        <span className="settings-input-unit">{t('settings.general.minutes')}</span>
                      </div>
                    ) : (
                      <select
                        className="settings-select"
                        style={{ minWidth: '120px', width: 'auto' }}
                        value={ghcpAutoRefresh}
                        onChange={(e) => {
                          const val = e.target.value;
                          if (val === 'custom') {
                            setGhcpAutoRefreshCustomMode(true);
                            setGhcpAutoRefresh(ghcpAutoRefresh !== '-1' ? ghcpAutoRefresh : '1');
                            return;
                          }
                          setGhcpAutoRefreshCustomMode(false);
                          setGhcpAutoRefresh(val);
                        }}
                      >
                        {!ghcpAutoRefreshIsPreset && (
                          <option value={ghcpAutoRefresh}>
                            {ghcpAutoRefresh} {t('settings.general.minutes')}
                          </option>
                        )}
                        <option value="-1">{t('settings.general.autoRefreshDisabled')}</option>
                        <option value="2">2 {t('settings.general.minutes')}</option>
                        <option value="5">5 {t('settings.general.minutes')}</option>
                        <option value="10">10 {t('settings.general.minutes')}</option>
                        <option value="15">15 {t('settings.general.minutes')}</option>
                        <option value="custom">{t('settings.general.autoRefreshCustom')}</option>
                      </select>
                    )}
                  </div>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.vscodeAppPath', 'VS Code 启动路径')}</div>
                  <div className="row-desc">{t('settings.general.vscodeAppPathDesc', '留空则使用默认路径')}</div>
                </div>
                <div className="row-control row-control--grow">
                  <div style={{ display: 'flex', gap: '8px', alignItems: 'center', flex: 1 }}>
                    <input
                      type="text"
                      className="settings-input settings-input--path"
                      value={vscodeAppPath}
                      placeholder={t('settings.general.vscodeAppPathPlaceholder', '默认路径')}
                      onChange={(e) => setVscodeAppPath(e.target.value)}
                    />
                    <button
                      className="btn btn-secondary"
                      onClick={() => handlePickAppPath('vscode')}
                      disabled={isAppPathResetDetecting('vscode')}
                    >
                      {t('settings.general.vscodePathSelect', '选择')}
                    </button>
                    <button
                      className="btn btn-secondary"
                      onClick={() => handleResetAppPath('vscode')}
                      disabled={isAppPathResetDetecting('vscode')}
                    >
                      <RefreshCw size={16} className={isAppPathResetDetecting('vscode') ? 'spin' : undefined} />
                      {isAppPathResetDetecting('vscode')
                        ? t('common.loading', '加载中...')
                        : getResetLabelByTarget('vscode')}
                    </button>
                  </div>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('quickSettings.quotaAlert.enable', '超额预警')}</div>
                  <div className="row-desc">{t('quickSettings.quotaAlert.hint', '当当前账号任意模型配额低于阈值时，发送原生通知并在页面提示快捷切号。')}</div>
                </div>
                <div className="row-control">
                  <label className="switch">
                    <input
                      type="checkbox"
                      checked={ghcpQuotaAlertEnabled}
                      onChange={(e) => setGhcpQuotaAlertEnabled(e.target.checked)}
                    />
                    <span className="slider"></span>
                  </label>
                </div>
              </div>
              {ghcpQuotaAlertEnabled && (
                <div className="settings-row" style={{ animation: 'fadeUp 0.3s ease both' }}>
                  <div className="row-label">
                    <div className="row-title">{t('quickSettings.quotaAlert.threshold', '预警阈值')}</div>
                    <div className="row-desc">{t('quickSettings.quotaAlert.thresholdDesc', '任意模型配额低于此百分比时触发预警')}</div>
                  </div>
                  <div className="row-control">
                    {ghcpQuotaAlertThresholdCustomMode ? (
                      <div className="settings-inline-input">
                        <input
                          type="number"
                          min={0}
                          max={100}
                          className="settings-select settings-select--input-mode settings-select--with-unit"
                          value={ghcpQuotaAlertThreshold}
                          placeholder={t('quickSettings.inputPercent', '输入百分比')}
                          onChange={(e) => setGhcpQuotaAlertThreshold(sanitizeNumberInput(e.target.value))}
                          onBlur={() => {
                            const normalized = normalizeNumberInput(ghcpQuotaAlertThreshold, 0, 100);
                            setGhcpQuotaAlertThreshold(normalized);
                            setGhcpQuotaAlertThresholdCustomMode(false);
                          }}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') {
                              e.preventDefault();
                              const normalized = normalizeNumberInput(ghcpQuotaAlertThreshold, 0, 100);
                              setGhcpQuotaAlertThreshold(normalized);
                              setGhcpQuotaAlertThresholdCustomMode(false);
                            }
                          }}
                        />
                        <span className="settings-input-unit">%</span>
                      </div>
                    ) : (
                      <select
                        className="settings-select"
                        value={ghcpQuotaAlertThreshold}
                        onChange={(e) => {
                          const val = e.target.value;
                          if (val === 'custom') {
                            setGhcpQuotaAlertThresholdCustomMode(true);
                            setGhcpQuotaAlertThreshold(ghcpQuotaAlertThreshold || '20');
                            return;
                          }
                          setGhcpQuotaAlertThresholdCustomMode(false);
                          setGhcpQuotaAlertThreshold(val);
                        }}
                      >
                        {!ghcpQuotaAlertThresholdIsPreset && (
                          <option value={ghcpQuotaAlertThreshold}>{ghcpQuotaAlertThreshold}%</option>
                        )}
                        <option value="0">0%</option>
                        <option value="20">20%</option>
                        <option value="40">40%</option>
                        <option value="60">60%</option>
                        <option value="custom">{t('settings.general.autoRefreshCustom')}</option>
                      </select>
                    )}
                  </div>
                </div>
              )}
            </div>

              </div>

              <div style={{ order: platformSettingsOrder.windsurf }}>
                <div className="group-title">{t('settings.general.windsurfSettingsTitle', 'Windsurf 设置')}</div>
                <div className="settings-group">
              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.windsurfAutoRefresh', 'Windsurf 自动刷新配额')}</div>
                  <div className="row-desc">{t('settings.general.windsurfAutoRefreshDesc', '后台自动更新频率')}</div>
                </div>
                <div className="row-control">
                  <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                    {windsurfAutoRefreshCustomMode ? (
                      <div className="settings-inline-input" style={{ minWidth: '120px', width: 'auto' }}>
                        <input
                          type="number"
                          min={1}
                          max={999}
                          className="settings-select settings-select--input-mode settings-select--with-unit"
                          value={windsurfAutoRefresh}
                          placeholder={t('quickSettings.inputMinutes', '输入分钟数')}
                          onChange={(e) => setWindsurfAutoRefresh(sanitizeNumberInput(e.target.value))}
                        onBlur={() => {
                          const normalized = normalizeNumberInput(windsurfAutoRefresh, 1, 999);
                          setWindsurfAutoRefresh(normalized);
                          setWindsurfAutoRefreshCustomMode(false);
                        }}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            e.preventDefault();
                            const normalized = normalizeNumberInput(windsurfAutoRefresh, 1, 999);
                            setWindsurfAutoRefresh(normalized);
                            setWindsurfAutoRefreshCustomMode(false);
                          }
                        }}
                      />
                        <span className="settings-input-unit">{t('settings.general.minutes')}</span>
                      </div>
                    ) : (
                      <select
                        className="settings-select"
                        style={{ minWidth: '120px', width: 'auto' }}
                        value={windsurfAutoRefresh}
                        onChange={(e) => {
                          const val = e.target.value;
                          if (val === 'custom') {
                            setWindsurfAutoRefreshCustomMode(true);
                            setWindsurfAutoRefresh(windsurfAutoRefresh !== '-1' ? windsurfAutoRefresh : '1');
                            return;
                          }
                          setWindsurfAutoRefreshCustomMode(false);
                          setWindsurfAutoRefresh(val);
                        }}
                      >
                        {!windsurfAutoRefreshIsPreset && (
                          <option value={windsurfAutoRefresh}>
                            {windsurfAutoRefresh} {t('settings.general.minutes')}
                          </option>
                        )}
                        <option value="-1">{t('settings.general.autoRefreshDisabled')}</option>
                        <option value="2">2 {t('settings.general.minutes')}</option>
                        <option value="5">5 {t('settings.general.minutes')}</option>
                        <option value="10">10 {t('settings.general.minutes')}</option>
                        <option value="15">15 {t('settings.general.minutes')}</option>
                        <option value="custom">{t('settings.general.autoRefreshCustom')}</option>
                      </select>
                    )}
                  </div>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.windsurfAppPath', 'Windsurf 启动路径')}</div>
                  <div className="row-desc">{t('settings.general.windsurfAppPathDesc', '留空则使用默认路径')}</div>
                </div>
                <div className="row-control row-control--grow">
                  <div style={{ display: 'flex', gap: '8px', alignItems: 'center', flex: 1 }}>
                    <input
                      type="text"
                      className="settings-input settings-input--path"
                      value={windsurfAppPath}
                      placeholder={t('settings.general.windsurfAppPathPlaceholder', '默认路径')}
                      onChange={(e) => setWindsurfAppPath(e.target.value)}
                    />
                    <button
                      className="btn btn-secondary"
                      onClick={() => handlePickAppPath('windsurf')}
                      disabled={isAppPathResetDetecting('windsurf')}
                    >
                      {t('settings.general.windsurfPathSelect', '选择')}
                    </button>
                    <button
                      className="btn btn-secondary"
                      onClick={() => handleResetAppPath('windsurf')}
                      disabled={isAppPathResetDetecting('windsurf')}
                    >
                      <RefreshCw size={16} className={isAppPathResetDetecting('windsurf') ? 'spin' : undefined} />
                      {isAppPathResetDetecting('windsurf')
                        ? t('common.loading', '加载中...')
                        : getResetLabelByTarget('windsurf')}
                    </button>
                  </div>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('quickSettings.quotaAlert.enable', '超额预警')}</div>
                  <div className="row-desc">{t('quickSettings.quotaAlert.hint', '当当前账号任意模型配额低于阈值时，发送原生通知并在页面提示快捷切号。')}</div>
                </div>
                <div className="row-control">
                  <label className="switch">
                    <input
                      type="checkbox"
                      checked={windsurfQuotaAlertEnabled}
                      onChange={(e) => setWindsurfQuotaAlertEnabled(e.target.checked)}
                    />
                    <span className="slider"></span>
                  </label>
                </div>
              </div>
              {windsurfQuotaAlertEnabled && (
                <div className="settings-row" style={{ animation: 'fadeUp 0.3s ease both' }}>
                  <div className="row-label">
                    <div className="row-title">{t('quickSettings.quotaAlert.threshold', '预警阈值')}</div>
                    <div className="row-desc">{t('quickSettings.quotaAlert.thresholdDesc', '任意模型配额低于此百分比时触发预警')}</div>
                  </div>
                  <div className="row-control">
                    {windsurfQuotaAlertThresholdCustomMode ? (
                      <div className="settings-inline-input">
                        <input
                          type="number"
                          min={0}
                          max={100}
                          className="settings-select settings-select--input-mode settings-select--with-unit"
                          value={windsurfQuotaAlertThreshold}
                          placeholder={t('quickSettings.inputPercent', '输入百分比')}
                          onChange={(e) => setWindsurfQuotaAlertThreshold(sanitizeNumberInput(e.target.value))}
                          onBlur={() => {
                            const normalized = normalizeNumberInput(windsurfQuotaAlertThreshold, 0, 100);
                            setWindsurfQuotaAlertThreshold(normalized);
                            setWindsurfQuotaAlertThresholdCustomMode(false);
                          }}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') {
                              e.preventDefault();
                              const normalized = normalizeNumberInput(windsurfQuotaAlertThreshold, 0, 100);
                              setWindsurfQuotaAlertThreshold(normalized);
                              setWindsurfQuotaAlertThresholdCustomMode(false);
                            }
                          }}
                        />
                        <span className="settings-input-unit">%</span>
                      </div>
                    ) : (
                      <select
                        className="settings-select"
                        value={windsurfQuotaAlertThreshold}
                        onChange={(e) => {
                          const val = e.target.value;
                          if (val === 'custom') {
                            setWindsurfQuotaAlertThresholdCustomMode(true);
                            setWindsurfQuotaAlertThreshold(windsurfQuotaAlertThreshold || '20');
                            return;
                          }
                          setWindsurfQuotaAlertThresholdCustomMode(false);
                          setWindsurfQuotaAlertThreshold(val);
                        }}
                      >
                        {!windsurfQuotaAlertThresholdIsPreset && (
                          <option value={windsurfQuotaAlertThreshold}>{windsurfQuotaAlertThreshold}%</option>
                        )}
                        <option value="0">0%</option>
                        <option value="20">20%</option>
                        <option value="40">40%</option>
                        <option value="60">60%</option>
                        <option value="custom">{t('settings.general.autoRefreshCustom')}</option>
                      </select>
                    )}
                  </div>
                </div>
              )}
            </div>

              </div>

              <div style={{ order: platformSettingsOrder.kiro }}>
                <div className="group-title">{t('settings.general.kiroSettingsTitle', 'Kiro 设置')}</div>
                <div className="settings-group">
              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.kiroAutoRefresh', 'Kiro 自动刷新配额')}</div>
                  <div className="row-desc">{t('settings.general.kiroAutoRefreshDesc', '后台自动更新频率')}</div>
                </div>
                <div className="row-control">
                  <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                    {kiroAutoRefreshCustomMode ? (
                      <div className="settings-inline-input" style={{ minWidth: '120px', width: 'auto' }}>
                        <input
                          type="number"
                          min={1}
                          max={999}
                          className="settings-select settings-select--input-mode settings-select--with-unit"
                          value={kiroAutoRefresh}
                          placeholder={t('quickSettings.inputMinutes', '输入分钟数')}
                          onChange={(e) => setKiroAutoRefresh(sanitizeNumberInput(e.target.value))}
                          onBlur={() => {
                            const normalized = normalizeNumberInput(kiroAutoRefresh, 1, 999);
                            setKiroAutoRefresh(normalized);
                            setKiroAutoRefreshCustomMode(false);
                          }}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') {
                              e.preventDefault();
                              const normalized = normalizeNumberInput(kiroAutoRefresh, 1, 999);
                              setKiroAutoRefresh(normalized);
                              setKiroAutoRefreshCustomMode(false);
                            }
                          }}
                        />
                        <span className="settings-input-unit">{t('settings.general.minutes')}</span>
                      </div>
                    ) : (
                      <select
                        className="settings-select"
                        style={{ minWidth: '120px', width: 'auto' }}
                        value={kiroAutoRefresh}
                        onChange={(e) => {
                          const val = e.target.value;
                          if (val === 'custom') {
                            setKiroAutoRefreshCustomMode(true);
                            setKiroAutoRefresh(kiroAutoRefresh !== '-1' ? kiroAutoRefresh : '1');
                            return;
                          }
                          setKiroAutoRefreshCustomMode(false);
                          setKiroAutoRefresh(val);
                        }}
                      >
                        {!kiroAutoRefreshIsPreset && (
                          <option value={kiroAutoRefresh}>
                            {kiroAutoRefresh} {t('settings.general.minutes')}
                          </option>
                        )}
                        <option value="-1">{t('settings.general.autoRefreshDisabled')}</option>
                        <option value="2">2 {t('settings.general.minutes')}</option>
                        <option value="5">5 {t('settings.general.minutes')}</option>
                        <option value="10">10 {t('settings.general.minutes')}</option>
                        <option value="15">15 {t('settings.general.minutes')}</option>
                        <option value="custom">{t('settings.general.autoRefreshCustom')}</option>
                      </select>
                    )}
                  </div>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.general.kiroAppPath', 'Kiro 启动路径')}</div>
                  <div className="row-desc">{t('settings.general.kiroAppPathDesc', '留空则使用默认路径')}</div>
                </div>
                <div className="row-control row-control--grow">
                  <div style={{ display: 'flex', gap: '8px', alignItems: 'center', flex: 1 }}>
                    <input
                      type="text"
                      className="settings-input settings-input--path"
                      value={kiroAppPath}
                      placeholder={t('settings.general.kiroAppPathPlaceholder', '默认路径')}
                      onChange={(e) => setKiroAppPath(e.target.value)}
                    />
                    <button
                      className="btn btn-secondary"
                      onClick={() => handlePickAppPath('kiro')}
                      disabled={isAppPathResetDetecting('kiro')}
                    >
                      {t('settings.general.kiroPathSelect', '选择')}
                    </button>
                    <button
                      className="btn btn-secondary"
                      onClick={() => handleResetAppPath('kiro')}
                      disabled={isAppPathResetDetecting('kiro')}
                    >
                      <RefreshCw size={16} className={isAppPathResetDetecting('kiro') ? 'spin' : undefined} />
                      {isAppPathResetDetecting('kiro')
                        ? t('common.loading', '加载中...')
                        : getResetLabelByTarget('kiro')}
                    </button>
                  </div>
                </div>
              </div>

              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('quickSettings.quotaAlert.enable', '超额预警')}</div>
                  <div className="row-desc">{t('quickSettings.quotaAlert.hint', '当当前账号任意模型配额低于阈值时，发送原生通知并在页面提示快捷切号。')}</div>
                </div>
                <div className="row-control">
                  <label className="switch">
                    <input
                      type="checkbox"
                      checked={kiroQuotaAlertEnabled}
                      onChange={(e) => setKiroQuotaAlertEnabled(e.target.checked)}
                    />
                    <span className="slider"></span>
                  </label>
                </div>
              </div>
              {kiroQuotaAlertEnabled && (
                <div className="settings-row" style={{ animation: 'fadeUp 0.3s ease both' }}>
                  <div className="row-label">
                    <div className="row-title">{t('quickSettings.quotaAlert.threshold', '预警阈值')}</div>
                    <div className="row-desc">{t('quickSettings.quotaAlert.thresholdDesc', '任意模型配额低于此百分比时触发预警')}</div>
                  </div>
                  <div className="row-control">
                    {kiroQuotaAlertThresholdCustomMode ? (
                      <div className="settings-inline-input">
                        <input
                          type="number"
                          min={0}
                          max={100}
                          className="settings-select settings-select--input-mode settings-select--with-unit"
                          value={kiroQuotaAlertThreshold}
                          placeholder={t('quickSettings.inputPercent', '输入百分比')}
                          onChange={(e) => setKiroQuotaAlertThreshold(sanitizeNumberInput(e.target.value))}
                          onBlur={() => {
                            const normalized = normalizeNumberInput(kiroQuotaAlertThreshold, 0, 100);
                            setKiroQuotaAlertThreshold(normalized);
                            setKiroQuotaAlertThresholdCustomMode(false);
                          }}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') {
                              e.preventDefault();
                              const normalized = normalizeNumberInput(kiroQuotaAlertThreshold, 0, 100);
                              setKiroQuotaAlertThreshold(normalized);
                              setKiroQuotaAlertThresholdCustomMode(false);
                            }
                          }}
                        />
                        <span className="settings-input-unit">%</span>
                      </div>
                    ) : (
                      <select
                        className="settings-select"
                        value={kiroQuotaAlertThreshold}
                        onChange={(e) => {
                          const val = e.target.value;
                          if (val === 'custom') {
                            setKiroQuotaAlertThresholdCustomMode(true);
                            setKiroQuotaAlertThreshold(kiroQuotaAlertThreshold || '20');
                            return;
                          }
                          setKiroQuotaAlertThresholdCustomMode(false);
                          setKiroQuotaAlertThreshold(val);
                        }}
                      >
                        {!kiroQuotaAlertThresholdIsPreset && (
                          <option value={kiroQuotaAlertThreshold}>{kiroQuotaAlertThreshold}%</option>
                        )}
                        <option value="0">0%</option>
                        <option value="20">20%</option>
                        <option value="40">40%</option>
                        <option value="60">60%</option>
                        <option value="custom">{t('settings.general.autoRefreshCustom')}</option>
                      </select>
                    )}
                  </div>
                </div>
              )}
            </div>
              </div>
            </div>

          </>
        )}

        {/* === Network Tab === */}
        {activeTab === 'network' && (
          <>
            <div className="group-title">{t('settings.network.apiTitle')}</div>
            <div className="settings-group">
              <div className="settings-row">
                <div className="row-label">
                  <div className="row-title">{t('settings.network.wsService')}</div>
                  <div className="row-desc">{t('settings.network.wsServiceDesc')}</div>
                </div>
                <div className="row-control">
                  <label className="switch">
                    <input 
                      type="checkbox" 
                      checked={wsEnabled} 
                      onChange={(e) => setWsEnabled(e.target.checked)} 
                    />
                    <span className="slider"></span>
                  </label>
                </div>
              </div>

              {wsEnabled && (
                <>
                  <div className="settings-row" style={{ animation: 'fadeUp 0.3s ease both' }}>
                    <div className="row-label">
                      <div className="row-title">{t('settings.network.preferredPort')}</div>
                      <div className="row-desc">
                        {t('settings.network.preferredPortDesc').replace('{port}', String(defaultPort))}
                      </div>
                    </div>
                    <div className="row-control">
                      <input 
                        type="number" 
                        className="settings-input"
                        value={wsPort}
                        onChange={(e) => setWsPort(e.target.value)}
                        placeholder={String(defaultPort)}
                        min="1024"
                        max="65535"
                      />
                    </div>
                  </div>
                  
                  {actualPort && (
                    <div className="settings-row" style={{ animation: 'fadeUp 0.3s ease both' }}>
                      <div className="row-label">
                        <div className="row-title">{t('settings.network.currentPort')}</div>
                        <div className="row-desc">
                          {actualPort === parseInt(wsPort, 10) 
                            ? t('settings.network.portNormal')
                            : t('settings.network.portFallback')
                                .replace('{configured}', wsPort)
                                .replace('{actual}', String(actualPort))}
                        </div>
                      </div>
                      <div className="row-control">
                        <span style={{ 
                          fontFamily: 'var(--font-mono)', 
                          fontSize: '14px',
                          color: actualPort === parseInt(wsPort, 10) ? 'var(--accent)' : 'var(--warning, #f59e0b)'
                        }}>
                          ws://127.0.0.1:{actualPort}
                        </span>
                      </div>
                    </div>
                  )}
                </>
              )}
            </div>
            
            {needsRestart && (
              <div style={{ 
                display: 'flex', 
                alignItems: 'center', 
                gap: '8px', 
                padding: '12px 16px',
                marginTop: '12px',
                background: 'rgba(245, 158, 11, 0.1)',
                borderRadius: '8px',
                color: 'var(--warning, #f59e0b)',
                fontSize: '14px'
              }}>
                <AlertCircle size={18} />
                {t('settings.network.restartRequired')}
              </div>
            )}

            <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: '12px' }}>
                <button 
                  className="btn btn-primary" 
                  onClick={handleSaveNetworkConfig}
                  disabled={networkSaving}
                >
                    <Save size={16} /> {networkSaving ? t('common.saving') : t('settings.saveSettings')}
                </button>
            </div>
          </>
        )}

        {/* === About Tab === */}
        {activeTab === 'about' && (
          <div className="about-container">
            <div className="about-logo-section">
              <div className="app-icon-squircle">
                <Rocket size={40} />
              </div>
              <div className="app-info">
                <h2>{t('settings.about.appName')}</h2>
                <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                  <div className="version-tag">{appVersion}</div>
                  <button 
                    className="btn btn-sm btn-ghost"
                    onClick={handleCheckUpdate}
                    disabled={updateChecking}
                    style={{ 
                      fontSize: '12px', 
                      padding: '4px 10px',
                      display: 'flex',
                      alignItems: 'center',
                      gap: '4px'
                    }}
                  >
                    <>
                      <RefreshCw size={14} className={updateChecking ? 'spin' : undefined} />
                      {updateChecking ? t('settings.about.checking') : t('settings.about.checkUpdate')}
                    </>
                  </button>
                </div>
                {updateCheckMessage && (
                  <div
                    className={`action-message${updateCheckMessage.tone ? ` ${updateCheckMessage.tone}` : ''}`}
                    style={{ marginTop: '10px', marginBottom: 0 }}
                  >
                    <span className="action-message-text">{updateCheckMessage.text}</span>
                  </div>
                )}
              </div>
              <p style={{ color: 'var(--text-secondary)', fontSize: '14px' }}>
                {t('settings.about.slogan')}
              </p>
            </div>

            <div className="credits-list">
              <button className="credit-item" onClick={() => openLink('https://github.com/jlcodes99')}>
                <div className="credit-icon"><User size={24} /></div>
                <h3>{t('settings.about.author')}</h3>
                <p>jlcodes99</p>
              </button>
              
              
              <button className="credit-item" onClick={() => openLink('https://github.com/jlcodes99/cockpit-tools')}>
                <div className="credit-icon" style={{ color: '#0f172a' }}><Github size={24} /></div>
                <h3>{t('settings.about.github')}</h3>
                <p>cockpit-tools</p>
              </button>

              <button className="credit-item" onClick={() => openLink('https://github.com/jlcodes99/cockpit-tools/blob/main/docs/DONATE.md')}>
                <div className="credit-icon" style={{ color: '#ef4444' }}><Heart size={24} /></div>
                <h3>{t('settings.about.sponsor')}</h3>
                <p>{t('settings.about.sponsorDesc', 'Donate')}</p>
              </button>

              <button className="credit-item" onClick={() => openLink('https://github.com/jlcodes99/cockpit-tools/issues')}>
                <div className="credit-icon" style={{ color: '#3b82f6' }}><MessageSquare size={24} /></div>
                <h3>{t('settings.about.feedback', '意见反馈')}</h3>
                <p>{t('settings.about.feedbackDesc', 'Issues')}</p>
              </button>
            </div>
          </div>
        )}
        </div>
      </div>
    </main>
  );
}
