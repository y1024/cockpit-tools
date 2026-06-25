import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { BookOpenText, Download, MoreHorizontal, RefreshCw, RotateCw, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import type { PlatformId } from '../types/platform';
import type { PlatformPackageChangelogEntry, PlatformPackageState } from '../types/platformPackage';
import {
  formatPlatformPackageSize,
  getPlatformPackageFromPackages,
  usePlatformPackageStore,
} from '../stores/usePlatformPackageStore';
import { useGlobalModal } from '../hooks/useGlobalModal';
import { getPlatformLabel } from '../utils/platformMeta';
import './PlatformPackageToolbar.css';

type PackageAction = 'install' | 'update' | 'uninstall';

interface PlatformPackageToolbarProps {
  platformId: PlatformId;
  className?: string;
  fallbackState?: PlatformPackageState | null;
}

function normalizeLocaleKey(value: string): string {
  return value.trim().replace('_', '-').toLowerCase();
}

function buildLocaleFallbacks(language: string | undefined): string[] {
  const normalized = normalizeLocaleKey(language || '');
  const fallbacks: string[] = [];
  const push = (value: string) => {
    const key = normalizeLocaleKey(value);
    if (key && !fallbacks.includes(key)) {
      fallbacks.push(key);
    }
  };

  push(normalized);
  if (normalized.includes('-')) {
    push(normalized.split('-')[0]);
  }
  push('en-us');
  push('en');
  return fallbacks;
}

function getLocalizedChangelogNotes(
  entry: PlatformPackageChangelogEntry,
  language: string | undefined,
): string[] {
  const locales = entry.locales || {};
  const localeEntries = Object.entries(locales).map(([key, value]) => [
    normalizeLocaleKey(key),
    value,
  ] as const);

  for (const fallback of buildLocaleFallbacks(language)) {
    const match = localeEntries.find(([key]) => key === fallback);
    if (match && Array.isArray(match[1]?.notes) && match[1].notes.length > 0) {
      return match[1].notes;
    }
  }

  return entry.notes || [];
}

function comparePackageVersions(left: string | null | undefined, right: string | null | undefined): number {
  const parse = (value: string | null | undefined) => (value || '')
    .trim()
    .split(/[.+-]/)
    .slice(0, 3)
    .map((part) => Number.parseInt(part, 10))
    .map((part) => (Number.isFinite(part) ? part : 0));
  const leftParts = parse(left);
  const rightParts = parse(right);
  while (leftParts.length < 3) leftParts.push(0);
  while (rightParts.length < 3) rightParts.push(0);
  for (let index = 0; index < 3; index += 1) {
    if (leftParts[index] !== rightParts[index]) {
      return leftParts[index] - rightParts[index];
    }
  }
  return 0;
}

function getRelevantChangelogEntries(state: PlatformPackageState): PlatformPackageChangelogEntry[] {
  const entries = state.changelog || [];
  if (state.installedVersion) {
    const newerEntries = entries.filter((entry) =>
      comparePackageVersions(entry.version, state.installedVersion) > 0,
    );
    if (newerEntries.length > 0) {
      return newerEntries;
    }
  }
  if (state.latestVersion) {
    const latestEntries = entries.filter((entry) => entry.version === state.latestVersion);
    if (latestEntries.length > 0) {
      return latestEntries;
    }
  }
  return entries;
}

function ChangelogEntryList({
  entries,
  language,
  t,
}: {
  entries: PlatformPackageChangelogEntry[];
  language: string | undefined;
  t: TFunction;
}) {
  if (entries.length <= 0) {
    return (
      <div className="platform-package-changelog-empty">
        {t('platformLayout.packageChangelogEmpty', '暂无更新日志。')}
      </div>
    );
  }

  return (
    <div className="platform-package-changelog-list">
      {entries.map((entry) => {
        const notes = getLocalizedChangelogNotes(entry, language);
        return (
          <section className="platform-package-changelog-entry" key={`${entry.version}:${entry.date || ''}`}>
            <div className="platform-package-changelog-entry-head">
              <strong>v{entry.version}</strong>
              {entry.date ? <span>{entry.date}</span> : null}
            </div>
            {notes.length > 0 ? (
              <ul>
                {notes.map((note, index) => (
                  <li key={`${entry.version}:${index}`}>{note}</li>
                ))}
              </ul>
            ) : (
              <p>{t('platformLayout.packageChangelogEntryEmpty', '此版本暂无说明。')}</p>
            )}
          </section>
        );
      })}
    </div>
  );
}

export function getPlatformPackageShortStatus(
  state: PlatformPackageState | null | undefined,
  t: TFunction,
): { label: string; tone: 'warning' | 'info' | 'danger' | 'muted' } | null {
  if (!state || state.packageMode !== 'hotUpdate') {
    return null;
  }

  if (state.installStatus === 'notInstalled') {
    return {
      label: t('platformLayout.packageInstallRequired', '未安装'),
      tone: 'warning',
    };
  }
  if (state.installStatus === 'updateAvailable') {
    return {
      label: t('platformLayout.packageUpdateAvailableShort', '可更新'),
      tone: 'info',
    };
  }
  if (state.installStatus === 'incompatible') {
    return {
      label: t('platformLayout.packageIncompatibleShort', '不兼容'),
      tone: 'danger',
    };
  }
  if (state.installStatus === 'error' || !state.runtimeReady) {
    return {
      label: t('platformLayout.packageRepairRequired', '需修复'),
      tone: 'danger',
    };
  }
  if (
    state.installStatus === 'installing'
    || state.installStatus === 'updating'
    || state.installStatus === 'uninstalling'
  ) {
    return {
      label: t('platformLayout.packageOperating', '处理中'),
      tone: 'muted',
    };
  }
  return null;
}

export function getPlatformPackageStatusText(
  state: PlatformPackageState,
  t: TFunction,
): string {
  if (state.packageMode === 'bundled') {
    return t('platformLayout.packageBundledStatus', '随主应用提供');
  }

  const version = state.installedVersion || state.latestVersion || '--';
  const installedSize = formatPlatformPackageSize(state.installedSizeBytes);
  const downloadSize = formatPlatformPackageSize(state.downloadSizeBytes);

  switch (state.installStatus) {
    case 'notInstalled':
      return t('platformLayout.packageNotInstalled', {
        size: downloadSize,
        defaultValue: '未下载 · {{size}}',
      });
    case 'updateAvailable':
      return t('platformLayout.packageUpdateAvailable', {
        version: state.latestVersion || '--',
        size: downloadSize,
        defaultValue: '可更新 {{version}} · {{size}}',
      });
    case 'incompatible':
      return t('platformLayout.packageIncompatible', '主应用版本不兼容');
    case 'error':
      return state.errorMessage || t('platformLayout.packageError', '状态异常');
    case 'installing':
    case 'updating':
    case 'uninstalling':
      return t('platformLayout.packageOperating', '处理中');
    default:
      return t('platformLayout.packageInstalled', {
        version,
        size: installedSize,
        defaultValue: '已安装 v{{version}} · {{size}}',
      });
  }
}

function dispatchPlatformPackageChanged(state: PlatformPackageState) {
  if (typeof window === 'undefined') {
    return;
  }
  window.dispatchEvent(
    new CustomEvent('agtools:platform-package-changed', {
      detail: state,
    }),
  );
}

export function PlatformPackageToolbar({
  platformId,
  className,
  fallbackState,
}: PlatformPackageToolbarProps) {
  const { t, i18n } = useTranslation();
  const { showModal } = useGlobalModal();
  const packages = usePlatformPackageStore((state) => state.packages);
  const loading = usePlatformPackageStore((state) => state.loading);
  const checkUpdate = usePlatformPackageStore((state) => state.checkUpdate);
  const installPackage = usePlatformPackageStore((state) => state.installPackage);
  const updatePackage = usePlatformPackageStore((state) => state.updatePackage);
  const uninstallPackage = usePlatformPackageStore((state) => state.uninstallPackage);
  const refreshPackages = usePlatformPackageStore((state) => state.refresh);
  const [actionKey, setActionKey] = useState<string | null>(null);
  const [operationError, setOperationError] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const actionPromisesRef = useRef<Map<string, Promise<PlatformPackageState>>>(new Map());
  const rootRef = useRef<HTMLDivElement | null>(null);

  const platformPackage = useMemo(
    () => getPlatformPackageFromPackages(packages, platformId) ?? fallbackState ?? null,
    [fallbackState, packages, platformId],
  );

  const platformName = getPlatformLabel(platformId, t);

  const runAction = useCallback(async (
    action: PackageAction,
    options?: { requireRuntimeReady?: boolean },
  ): Promise<PlatformPackageState> => {
    const key = `${platformId}:${action}`;
    const existing = actionPromisesRef.current.get(key);
    if (existing) {
      return await existing;
    }

    const promise = (async () => {
      setActionKey(key);
      setOperationError(null);
      let nextState = action === 'install'
        ? await installPackage(platformId)
        : action === 'update'
          ? await updatePackage(platformId)
          : await uninstallPackage(platformId);
      dispatchPlatformPackageChanged(nextState);
      if (options?.requireRuntimeReady && !nextState.runtimeReady) {
        try {
          const refreshedPackages = await refreshPackages();
          const refreshedState = getPlatformPackageFromPackages(refreshedPackages, platformId);
          if (refreshedState) {
            nextState = refreshedState;
            dispatchPlatformPackageChanged(nextState);
          }
        } catch {
          // Keep the original action result; the operation error below will surface it.
        }
      }
      if (options?.requireRuntimeReady && !nextState.runtimeReady) {
        throw new Error(
          nextState.errorMessage || t('platformLayout.packageInstallNotReady', '平台包已处理，但运行组件尚未就绪'),
        );
      }
      return nextState;
    })()
      .catch((error) => {
        const message = error instanceof Error ? error.message : String(error);
        setOperationError(message);
        throw error;
      })
      .finally(() => {
        actionPromisesRef.current.delete(key);
        setActionKey((current) => (current === key ? null : current));
      });

    actionPromisesRef.current.set(key, promise);
    return await promise;
  }, [installPackage, platformId, refreshPackages, t, uninstallPackage, updatePackage]);

  const confirmAction = useCallback((action: PackageAction) => {
    if (!platformPackage) {
      return;
    }
    setMenuOpen(false);

    const version = action === 'update'
      ? platformPackage.latestVersion || '--'
      : platformPackage.latestVersion || platformPackage.installedVersion || '--';
    const size = action === 'uninstall'
      ? formatPlatformPackageSize(platformPackage.installedSizeBytes)
      : formatPlatformPackageSize(platformPackage.downloadSizeBytes);
    const isRepair = action === 'install' && platformPackage.installStatus === 'error';

    const title = action === 'uninstall'
      ? t('platformLayout.packageUninstallConfirmTitle', {
          platform: platformName,
          defaultValue: '卸载 {{platform}} 平台包',
        })
      : action === 'update'
        ? t('platformLayout.packageUpdateConfirmTitle', {
            platform: platformName,
            defaultValue: '更新 {{platform}} 平台包',
          })
        : isRepair
          ? t('platformLayout.packageRepairConfirmTitle', {
              platform: platformName,
              defaultValue: '修复 {{platform}} 平台包',
            })
          : t('platformLayout.packageInstallConfirmTitle', {
              platform: platformName,
              defaultValue: '安装 {{platform}} 平台包',
            });
    const description = action === 'uninstall'
      ? t('platformLayout.packageUninstallConfirmDesc', {
          platform: platformName,
          size,
          defaultValue: '将移除 {{platform}} 的平台包和运行组件，占用 {{size}}；已保存账号数据不会删除。',
        })
      : action === 'update'
        ? t('platformLayout.packageUpdateConfirmDesc', {
            platform: platformName,
            version,
            size,
            defaultValue: '将下载并切换到 {{platform}} 平台包 {{version}}，大小 {{size}}。',
          })
        : isRepair
          ? t('platformLayout.packageRepairConfirmDesc', {
              platform: platformName,
              version,
              size,
              defaultValue: '将重新下载并校验 {{platform}} 平台包 {{version}}，大小 {{size}}。',
            })
          : t('platformLayout.packageInstallConfirmDesc', {
              platform: platformName,
              version,
              size,
              defaultValue: '{{platform}} 需要先下载平台包。版本 {{version}}，大小 {{size}}。',
            });
    const actionLabel = action === 'uninstall'
      ? t('platformLayout.packageUninstall', '卸载')
      : action === 'update'
        ? t('platformLayout.packageUpdate', '更新')
        : isRepair
          ? t('platformLayout.packageRepair', '修复')
          : t('platformLayout.packageDownload', '下载');

    showModal({
      title,
      description,
      width: 'sm',
      actions: [
        {
          id: 'cancel',
          label: t('common.cancel', '取消'),
          variant: 'secondary',
        },
        {
          id: `platform-package-${action}`,
          label: actionLabel,
          variant: action === 'uninstall' ? 'danger' : 'primary',
          onClick: async () => {
            await runAction(action, { requireRuntimeReady: action !== 'uninstall' });
          },
        },
      ],
    });
  }, [platformName, platformPackage, runAction, showModal, t]);

  const handleCheckUpdate = useCallback(async () => {
    if (!platformPackage || actionKey) {
      return;
    }
    const key = `${platformId}:check`;
    setActionKey(key);
    setOperationError(null);
    try {
      const nextState = await checkUpdate(platformId);
      dispatchPlatformPackageChanged(nextState);
    } catch (error) {
      setOperationError(error instanceof Error ? error.message : String(error));
    } finally {
      setActionKey((current) => (current === key ? null : current));
    }
  }, [actionKey, checkUpdate, platformId, platformPackage]);

  const showChangelog = useCallback(() => {
    if (!platformPackage) {
      return;
    }
    setMenuOpen(false);
    const entries = platformPackage.changelog || [];
    showModal({
      title: t('platformLayout.packageChangelogTitle', {
        platform: platformName,
        defaultValue: '{{platform}} 更新日志',
      }),
      width: 'md',
      content: (
        <ChangelogEntryList
          entries={entries}
          language={i18n.language}
          t={t}
        />
      ),
      actions: [
        {
          id: 'close',
          label: t('common.close', '关闭'),
          variant: 'primary',
        },
      ],
    });
  }, [i18n.language, platformName, platformPackage, showModal, t]);

  const showUpdateDialog = useCallback(() => {
    if (!platformPackage || platformPackage.installStatus !== 'updateAvailable') {
      return;
    }

    setMenuOpen(false);
    const latestVersion = platformPackage.latestVersion || '--';
    const currentVersion = platformPackage.installedVersion || '--';
    const downloadSize = formatPlatformPackageSize(platformPackage.downloadSizeBytes);
    const entries = getRelevantChangelogEntries(platformPackage);

    showModal({
      title: t('update_notification.title', '发现新版本'),
      width: 'md',
      content: (
        <div className="platform-package-update-dialog">
          <div className="platform-package-update-version">v{latestVersion}</div>
          <p className="platform-package-update-message">
            {t('update_notification.message', {
              current: currentVersion,
              defaultValue: '当前版本 v{{current}}，新版本已可用。',
            })}
          </p>
          <div className="platform-package-update-meta">
            <span>
              {t('platformLayout.packageUpdateAvailable', {
                version: latestVersion,
                size: downloadSize,
                defaultValue: '可更新 {{version}} · {{size}}',
              })}
            </span>
          </div>
          <div className="platform-package-update-notes">
            <h3>{t('update_notification.whatsNew', '更新内容')}</h3>
            <ChangelogEntryList
              entries={entries}
              language={i18n.language}
              t={t}
            />
          </div>
        </div>
      ),
      actions: [
        {
          id: 'platform-package-skip-update',
          label: t('update_notification.skipThisVersion', '跳过此版本'),
          variant: 'secondary',
        },
        {
          id: 'platform-package-update-now',
          label: t('update_notification.updateNow', '立即更新'),
          variant: 'primary',
          onClick: async () => {
            await runAction('update', { requireRuntimeReady: true });
          },
        },
      ],
    });
  }, [i18n.language, platformPackage, runAction, showModal, t]);

  useEffect(() => {
    setOperationError(null);
  }, [platformPackage?.installStatus, platformPackage?.installedVersion, platformPackage?.latestVersion]);

  useEffect(() => {
    if (!menuOpen) {
      return undefined;
    }

    const handlePointerDown = (event: MouseEvent) => {
      const target = event.target;
      if (target instanceof Node && rootRef.current?.contains(target)) {
        return;
      }
      setMenuOpen(false);
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setMenuOpen(false);
      }
    };

    document.addEventListener('mousedown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('mousedown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [menuOpen]);

  if (!platformPackage) {
    return null;
  }

  const isHotUpdate = platformPackage.packageMode === 'hotUpdate';
  const operating = loading || Boolean(actionKey);
  const statusText = getPlatformPackageStatusText(platformPackage, t);
  const canInstall = isHotUpdate && (platformPackage.installStatus === 'notInstalled'
    || platformPackage.installStatus === 'error'
    || (!platformPackage.runtimeReady
      && platformPackage.installStatus !== 'incompatible'
      && platformPackage.installStatus !== 'updateAvailable'));
  const canUpdate = isHotUpdate && platformPackage.installStatus === 'updateAvailable';
  const shouldShowRepairAction = isHotUpdate && (
    platformPackage.installStatus === 'error'
    || (!platformPackage.runtimeReady && platformPackage.installStatus !== 'notInstalled')
  );
  const hasInstalledPackage = isHotUpdate && Boolean(
    platformPackage.runtimeReady
    || platformPackage.installedVersion
    || platformPackage.installedSizeBytes,
  );
  const currentVersion = platformPackage.installedVersion || '--';
  const latestVersion = platformPackage.latestVersion || '--';
  const topActionKey = canUpdate
    ? `${platformId}:update`
    : canInstall
      ? `${platformId}:install`
      : `${platformId}:check`;
  const topActionLabel = canUpdate
    ? t('platformLayout.packageUpdate', '更新')
    : canInstall
      ? shouldShowRepairAction
        ? t('platformLayout.packageRepair', '修复')
        : t('platformLayout.packageDownload', '下载')
      : t('platformLayout.packageCheckUpdate', '检查更新');
  const topActionTitle = canUpdate
    ? t('platformLayout.packageUpdate', '更新')
    : canInstall
      ? shouldShowRepairAction
        ? t('platformLayout.packageRepair', '修复')
        : t('platformLayout.packageDownload', '下载')
      : t('platformLayout.packageCheckUpdate', '检查更新');
  const renderTopActionIcon = () => {
    if (actionKey === topActionKey) {
      return <RefreshCw size={15} className="loading-spinner" />;
    }
    if (canInstall) {
      return <Download size={15} />;
    }
    return <RotateCw size={15} />;
  };
  const handleTopAction = () => {
    if (operating || !isHotUpdate) {
      return;
    }
    if (canUpdate) {
      showUpdateDialog();
      return;
    }
    if (canInstall) {
      confirmAction('install');
      return;
    }
    void handleCheckUpdate();
  };

  return (
    <div className={`platform-package-toolbar ${className || ''}`.trim()} ref={rootRef}>
      {isHotUpdate && (
        <button
          type="button"
          className={`platform-package-inline-action${canUpdate ? ' is-primary' : ''}`}
          title={topActionTitle}
          onClick={handleTopAction}
          disabled={operating}
        >
          {renderTopActionIcon()}
          <span>{topActionLabel}</span>
        </button>
      )}

      {hasInstalledPackage && (
        <button
          type="button"
          className="platform-package-inline-action is-danger"
          title={t('platformLayout.packageUninstall', '卸载')}
          onClick={() => confirmAction('uninstall')}
          disabled={operating}
        >
          {actionKey === `${platformId}:uninstall`
            ? <RefreshCw size={15} className="loading-spinner" />
            : <Trash2 size={15} />}
          <span>{t('platformLayout.packageUninstall', '卸载')}</span>
        </button>
      )}

      <button
        type="button"
        className={`platform-package-menu-trigger${menuOpen ? ' is-open' : ''}`}
        title={t('common.more', '更多')}
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        onClick={() => setMenuOpen((open) => !open)}
      >
        <MoreHorizontal size={18} />
      </button>

      {menuOpen && (
        <div className="platform-package-menu" role="menu">
          <div className="platform-package-menu-head">
            <div className="platform-package-menu-status" title={statusText}>{statusText}</div>
            <div className="platform-package-menu-meta">
              {isHotUpdate ? (
                <span>
                  {t('platformLayout.packageCurrentVersion', {
                    version: currentVersion,
                    defaultValue: '当前 {{version}}',
                  })}
                </span>
              ) : (
                <span>{t('platformLayout.packageBundledShort', '内置')}</span>
              )}
              {isHotUpdate && platformPackage.installStatus === 'updateAvailable' && (
                <span>
                  {t('platformLayout.packageLatestVersion', {
                    version: latestVersion,
                    defaultValue: '最新 {{version}}',
                  })}
                </span>
              )}
            </div>
          </div>

          {operationError && (
            <div className="platform-package-menu-error" role="alert">
              {operationError}
            </div>
          )}

          {isHotUpdate ? (
            <div className="platform-package-menu-actions">
              <button
                type="button"
                className="platform-package-menu-action"
                onClick={handleCheckUpdate}
                disabled={operating}
                role="menuitem"
                title={t('platformLayout.packageCheckUpdate', '检查更新')}
              >
                <RotateCw size={14} className={actionKey === `${platformId}:check` ? 'loading-spinner' : ''} />
                <span>{t('platformLayout.packageCheckUpdateShort', '检查')}</span>
              </button>
              <button
                type="button"
                className="platform-package-menu-action"
                onClick={showChangelog}
                disabled={operating}
                role="menuitem"
                title={t('platformLayout.packageChangelog', '更新日志')}
              >
                <BookOpenText size={14} />
                <span>{t('platformLayout.packageChangelogShort', '日志')}</span>
              </button>
              {canInstall && (
                <button
                  type="button"
                  className="platform-package-menu-action is-primary"
                  onClick={() => confirmAction('install')}
                  disabled={operating}
                  role="menuitem"
                  title={shouldShowRepairAction
                    ? t('platformLayout.packageRepair', '修复')
                    : t('platformLayout.packageDownload', '下载')}
                >
                  {actionKey === `${platformId}:install`
                    ? <RefreshCw size={14} className="loading-spinner" />
                    : <Download size={14} />}
                  <span>
                    {shouldShowRepairAction
                      ? t('platformLayout.packageRepair', '修复')
                      : t('platformLayout.packageDownload', '下载')}
                  </span>
                </button>
              )}
              {canUpdate && (
                <button
                  type="button"
                  className="platform-package-menu-action is-primary"
                  onClick={showUpdateDialog}
                  disabled={operating}
                  role="menuitem"
                  title={t('platformLayout.packageUpdate', '更新')}
                >
                  <RefreshCw size={14} className={actionKey === `${platformId}:update` ? 'loading-spinner' : ''} />
                  <span>{t('platformLayout.packageUpdate', '更新')}</span>
                </button>
              )}
              {hasInstalledPackage && (
                <button
                  type="button"
                  className="platform-package-menu-action is-danger"
                  onClick={() => confirmAction('uninstall')}
                  disabled={operating}
                  role="menuitem"
                  title={t('platformLayout.packageUninstall', '卸载')}
                >
                  {actionKey === `${platformId}:uninstall`
                    ? <RefreshCw size={14} className="loading-spinner" />
                    : <Trash2 size={14} />}
                  <span>{t('platformLayout.packageUninstall', '卸载')}</span>
                </button>
              )}
            </div>
          ) : (
            <div className="platform-package-menu-note">
              {t(
                'platformLayout.packageBundledDesc',
                '此平台随主应用提供，安装和更新跟随主应用版本。',
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
