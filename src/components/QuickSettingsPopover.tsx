import { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { createPortal } from 'react-dom';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { Settings, RefreshCw, FolderOpen, Zap, X } from 'lucide-react';
import './QuickSettingsPopover.css';

/** GeneralConfig from backend */
interface GeneralConfig {
  language: string;
  theme: string;
  auto_refresh_minutes: number;
  codex_auto_refresh_minutes: number;
  ghcp_auto_refresh_minutes: number;
  close_behavior: string;
  opencode_app_path: string;
  antigravity_app_path: string;
  codex_app_path: string;
  vscode_app_path: string;
  opencode_sync_on_switch: boolean;
  auto_switch_enabled: boolean;
  auto_switch_threshold: number;
}

export type QuickSettingsType = 'antigravity' | 'codex' | 'github_copilot' | 'windsurf';

interface QuickSettingsPopoverProps {
  type: QuickSettingsType;
}

export function QuickSettingsPopover({ type }: QuickSettingsPopoverProps) {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);
  const [config, setConfig] = useState<GeneralConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [customRefresh, setCustomRefresh] = useState('');
  const [customThreshold, setCustomThreshold] = useState('');
  const modalRef = useRef<HTMLDivElement>(null);

  // Load config when modal opens
  useEffect(() => {
    if (isOpen) {
      loadConfig();
    }
  }, [isOpen]);

  // Close on Escape
  useEffect(() => {
    if (!isOpen) return;

    const handleEsc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setIsOpen(false);
    };

    document.addEventListener('keydown', handleEsc);
    return () => {
      document.removeEventListener('keydown', handleEsc);
    };
  }, [isOpen]);

  const loadConfig = async () => {
    try {
      const cfg = await invoke<GeneralConfig>('get_general_config');
      setConfig(cfg);
      // Initialize custom input values
      const refreshKey = getRefreshKeyForType(type);
      const val = cfg[refreshKey];
      const presets = ['-1', '2', '5', '10', '15'];
      if (!presets.includes(String(val))) {
        setCustomRefresh(String(val));
      } else {
        setCustomRefresh('');
      }
      if (type === 'antigravity') {
        const threshPresets = ['3', '5', '10', '15', '20'];
        if (!threshPresets.includes(String(cfg.auto_switch_threshold))) {
          setCustomThreshold(String(cfg.auto_switch_threshold));
        } else {
          setCustomThreshold('');
        }
      }
    } catch (err) {
      console.error('Failed to load config:', err);
    }
  };

  const getRefreshKeyForType = (t: QuickSettingsType): keyof GeneralConfig => {
    switch (t) {
      case 'antigravity': return 'auto_refresh_minutes';
      case 'codex': return 'codex_auto_refresh_minutes';
      case 'github_copilot': return 'ghcp_auto_refresh_minutes';
      case 'windsurf': return 'ghcp_auto_refresh_minutes';
    }
  };

  const saveConfig = useCallback(
    async (updates: Partial<GeneralConfig>) => {
      if (!config || saving) return;
      const merged = { ...config, ...updates };
      setConfig(merged);
      setSaving(true);
      try {
        await invoke('save_general_config', {
          language: merged.language,
          theme: merged.theme,
          autoRefreshMinutes: merged.auto_refresh_minutes,
          codexAutoRefreshMinutes: merged.codex_auto_refresh_minutes,
          ghcpAutoRefreshMinutes: merged.ghcp_auto_refresh_minutes,
          closeBehavior: merged.close_behavior,
          opencodeAppPath: merged.opencode_app_path,
          antigravityAppPath: merged.antigravity_app_path,
          codexAppPath: merged.codex_app_path,
          vscodeAppPath: merged.vscode_app_path,
          opencodeSyncOnSwitch: merged.opencode_sync_on_switch,
          autoSwitchEnabled: merged.auto_switch_enabled,
          autoSwitchThreshold: merged.auto_switch_threshold,
        });
        window.dispatchEvent(new Event('config-updated'));
      } catch (err) {
        console.error('Failed to save config:', err);
      } finally {
        setSaving(false);
      }
    },
    [config, saving]
  );

  const handlePickAppPath = async (target: 'antigravity' | 'codex' | 'vscode') => {
    try {
      const selected = await open({ multiple: false, directory: false });
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (!path || !config) return;

      const key =
        target === 'antigravity'
          ? 'antigravity_app_path'
          : target === 'codex'
            ? 'codex_app_path'
            : 'vscode_app_path';

      saveConfig({ [key]: path });
    } catch (err) {
      console.error('Failed to pick path:', err);
    }
  };

  const handleResetAppPath = async (target: 'antigravity' | 'codex' | 'vscode') => {
    try {
      const detected = await invoke<string | null>('detect_app_path', { app: target });
      const path = detected || '';
      const key =
        target === 'antigravity'
          ? 'antigravity_app_path'
          : target === 'codex'
            ? 'codex_app_path'
            : 'vscode_app_path';
      saveConfig({ [key]: path });
    } catch (err) {
      console.error('Failed to reset path:', err);
    }
  };

  const getTitle = () => {
    switch (type) {
      case 'antigravity':
        return t('quickSettings.antigravity.title', 'Antigravity 设置');
      case 'codex':
        return t('quickSettings.codex.title', 'Codex 设置');
      case 'github_copilot':
        return t('quickSettings.githubCopilot.title', 'GitHub Copilot 设置');
      case 'windsurf':
        return t('quickSettings.windsurf.title', 'Windsurf 设置');
    }
  };

  const getRefreshKey = (): keyof GeneralConfig => {
    return getRefreshKeyForType(type);
  };

  const getRefreshLabel = () => {
    switch (type) {
      case 'antigravity':
        return t('quickSettings.refreshInterval', '配额自动刷新');
      case 'codex':
        return t('quickSettings.codexRefreshInterval', '配额自动刷新');
      case 'github_copilot':
        return t('quickSettings.ghcpRefreshInterval', '配额自动刷新');
      case 'windsurf':
        return t('quickSettings.windsurfRefreshInterval', '配额自动刷新');
    }
  };

  const getAppPath = (): string => {
    if (!config) return '';
    switch (type) {
      case 'antigravity':
        return config.antigravity_app_path;
      case 'codex':
        return config.codex_app_path;
      case 'github_copilot':
        return config.vscode_app_path;
      case 'windsurf':
        return config.vscode_app_path;
    }
  };

  const getAppPathLabel = () => {
    switch (type) {
      case 'antigravity':
        return t('quickSettings.antigravity.appPath', '启动路径');
      case 'codex':
        return t('quickSettings.codex.appPath', '启动路径');
      case 'github_copilot':
        return t('quickSettings.githubCopilot.appPath', 'VS Code 路径');
      case 'windsurf':
        return t('quickSettings.windsurf.appPath', 'VS Code 路径');
    }
  };

  const getAppTarget = (): 'antigravity' | 'codex' | 'vscode' => {
    switch (type) {
      case 'antigravity':
        return 'antigravity';
      case 'codex':
        return 'codex';
      case 'github_copilot':
        return 'vscode';
      case 'windsurf':
        return 'vscode';
    }
  };

  const refreshValue = config ? (config[getRefreshKey()] as number) : 10;
  const refreshPresets = ['-1', '2', '5', '10', '15'];
  const isPreset = refreshPresets.includes(String(refreshValue));

  const thresholdPresets = ['3', '5', '10', '15', '20'];
  const isThresholdPreset = config ? thresholdPresets.includes(String(config.auto_switch_threshold)) : true;

  const handleRefreshSelectChange = (val: string) => {
    if (val === 'custom') {
      setCustomRefresh(String(refreshValue > 0 ? refreshValue : 1));
    } else {
      setCustomRefresh('');
      saveConfig({ [getRefreshKey()]: parseInt(val, 10) });
    }
  };

  const handleCustomRefreshApply = () => {
    const parsed = parseInt(customRefresh, 10);
    if (!isNaN(parsed) && parsed >= 1) {
      saveConfig({ [getRefreshKey()]: parsed });
    }
  };

  const handleThresholdSelectChange = (val: string) => {
    if (val === 'custom') {
      setCustomThreshold(String(config?.auto_switch_threshold ?? 5));
    } else {
      setCustomThreshold('');
      saveConfig({ auto_switch_threshold: parseInt(val, 10) });
    }
  };

  const handleCustomThresholdApply = () => {
    const parsed = parseInt(customThreshold, 10);
    if (!isNaN(parsed) && parsed >= 1 && parsed <= 100) {
      saveConfig({ auto_switch_threshold: parsed });
    }
  };

  const overlayContent = isOpen ? (
    <div className="qs-overlay" onClick={(e) => { if (e.target === e.currentTarget) setIsOpen(false); }}>
      <div className="qs-modal" ref={modalRef}>
        <div className="qs-header">
          <span className="qs-title">{getTitle()}</span>
          <button className="qs-close" onClick={() => setIsOpen(false)} aria-label={t('common.close')}>
            <X size={16} />
          </button>
        </div>

        {config && (
          <div className="qs-body">
            {/* ─── Refresh Interval ─── */}
            <div className="qs-section">
              <div className="qs-section-header">
                <RefreshCw size={15} />
                <span>{getRefreshLabel()}</span>
              </div>
              <div className="qs-field-group">
                <select
                  className="qs-select"
                  value={isPreset ? String(refreshValue) : 'custom'}
                  onChange={(e) => handleRefreshSelectChange(e.target.value)}
                >
                  <option value="-1">{t('settings.general.autoRefreshDisabled')}</option>
                  <option value="2">2 {t('settings.general.minutes')}</option>
                  <option value="5">5 {t('settings.general.minutes')}</option>
                  <option value="10">10 {t('settings.general.minutes')}</option>
                  <option value="15">15 {t('settings.general.minutes')}</option>
                  <option value="custom">{t('quickSettings.customInput', '自定义')}</option>
                </select>
                {(!isPreset || customRefresh) && (
                  <div className="qs-custom-input-row" style={{ animation: 'qsFadeUp 0.2s ease both' }}>
                    <input
                      type="number"
                      className="qs-number-input"
                      value={customRefresh}
                      min={1}
                      max={999}
                      placeholder={t('quickSettings.inputMinutes', '输入分钟数')}
                      onChange={(e) => setCustomRefresh(e.target.value)}
                      onKeyDown={(e) => { if (e.key === 'Enter') handleCustomRefreshApply(); }}
                    />
                    <span className="qs-unit">{t('settings.general.minutes')}</span>
                    <button className="qs-btn qs-btn--primary" onClick={handleCustomRefreshApply}>
                      {t('quickSettings.apply', '确定')}
                    </button>
                  </div>
                )}
              </div>
            </div>

            {/* ─── App Path ─── */}
            <div className="qs-section">
              <div className="qs-section-header">
                <FolderOpen size={15} />
                <span>{getAppPathLabel()}</span>
              </div>
              <div className="qs-path-control">
                <input
                  type="text"
                  className="qs-path-input"
                  value={getAppPath()}
                  placeholder={t('settings.general.codexAppPathPlaceholder', '默认路径')}
                  onChange={(e) => {
                    const key =
                      type === 'antigravity'
                        ? 'antigravity_app_path'
                        : type === 'codex'
                          ? 'codex_app_path'
                          : 'vscode_app_path';
                    saveConfig({ [key]: e.target.value });
                  }}
                />
                <div className="qs-path-actions">
                  <button
                    className="qs-btn"
                    onClick={() => handlePickAppPath(getAppTarget())}
                    title={t('settings.general.codexPathSelect', '选择')}
                  >
                    {t('settings.general.codexPathSelect', '选择')}
                  </button>
                  <button
                    className="qs-btn"
                    onClick={() => handleResetAppPath(getAppTarget())}
                    title={t('settings.general.codexPathReset', '恢复默认')}
                  >
                    <RefreshCw size={12} />
                  </button>
                </div>
              </div>
            </div>

            {/* ─── Codex: opencode sync ─── */}
            {type === 'codex' && (
              <div className="qs-section">
                <div className="qs-row">
                  <div className="qs-row-label">
                    <Zap size={15} />
                    <span>{t('settings.general.opencodeRestart', '切换时同步 OpenCode')}</span>
                  </div>
                  <div className="qs-row-control">
                    <label className="qs-switch">
                      <input
                        type="checkbox"
                        checked={config.opencode_sync_on_switch}
                        onChange={(e) => saveConfig({ opencode_sync_on_switch: e.target.checked })}
                      />
                      <span className="qs-switch-slider"></span>
                    </label>
                  </div>
                </div>
              </div>
            )}

            {/* ─── Antigravity: Auto-switch ─── */}
            {type === 'antigravity' && (
              <div className="qs-section qs-section--highlight">
                <div className="qs-section-header">
                  <Zap size={15} />
                  <span>{t('quickSettings.autoSwitch.title', '自动切号')}</span>
                </div>

                <div className="qs-row">
                  <div className="qs-row-label">
                    <span>{t('quickSettings.autoSwitch.enable', '启用自动切号')}</span>
                  </div>
                  <div className="qs-row-control">
                    <label className="qs-switch">
                      <input
                        type="checkbox"
                        checked={config.auto_switch_enabled}
                        onChange={(e) => saveConfig({ auto_switch_enabled: e.target.checked })}
                      />
                      <span className="qs-switch-slider"></span>
                    </label>
                  </div>
                </div>

                {config.auto_switch_enabled && (
                  <div className="qs-field-group" style={{ animation: 'qsFadeUp 0.2s ease both' }}>
                    <div className="qs-row">
                      <div className="qs-row-label">
                        <span>{t('quickSettings.autoSwitch.threshold', '切号阈值')}</span>
                      </div>
                      <div className="qs-row-control">
                        <select
                          className="qs-select"
                          value={isThresholdPreset && !customThreshold ? String(config.auto_switch_threshold) : 'custom'}
                          onChange={(e) => handleThresholdSelectChange(e.target.value)}
                        >
                          <option value="3">3%</option>
                          <option value="5">5%</option>
                          <option value="10">10%</option>
                          <option value="15">15%</option>
                          <option value="20">20%</option>
                          <option value="custom">{t('quickSettings.customInput', '自定义')}</option>
                        </select>
                      </div>
                    </div>
                    {(!isThresholdPreset || customThreshold) && (
                      <div className="qs-custom-input-row" style={{ animation: 'qsFadeUp 0.2s ease both' }}>
                        <input
                          type="number"
                          className="qs-number-input"
                          value={customThreshold}
                          min={1}
                          max={100}
                          placeholder={t('quickSettings.inputPercent', '输入百分比')}
                          onChange={(e) => setCustomThreshold(e.target.value)}
                          onKeyDown={(e) => { if (e.key === 'Enter') handleCustomThresholdApply(); }}
                        />
                        <span className="qs-unit">%</span>
                        <button className="qs-btn qs-btn--primary" onClick={handleCustomThresholdApply}>
                          {t('quickSettings.apply', '确定')}
                        </button>
                      </div>
                    )}
                  </div>
                )}

                <div className="qs-hint">
                  {t(
                    'quickSettings.autoSwitch.hint',
                    '当任意模型配额低于阈值时，自动切换到配额最高的账号。'
                  )}
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  ) : null;

  return (
    <div className="quick-settings-wrapper">
      <button
        className={`btn btn-secondary icon-only ${isOpen ? 'active' : ''}`}
        onClick={() => setIsOpen(!isOpen)}
        title={getTitle()}
        aria-label={getTitle()}
      >
        <Settings size={14} />
      </button>
      {overlayContent && createPortal(overlayContent, document.body)}
    </div>
  );
}
