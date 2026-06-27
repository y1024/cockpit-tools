import { useCallback, useState } from 'react';
import { Download, RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { PlatformId } from '../types/platform';
import type { PlatformPackageState } from '../types/platformPackage';
import { useGlobalModal } from '../hooks/useGlobalModal';
import {
  formatPlatformPackageSize,
  getPlatformPackageFromPackages,
  usePlatformPackageStore,
} from '../stores/usePlatformPackageStore';
import { getPlatformLabel, renderPlatformIcon } from '../utils/platformMeta';
import {
  getPlatformPackageStatusText,
  PlatformPackageOperationProgress,
} from './PlatformPackageToolbar';
import './PlatformPackageUnavailablePage.css';

interface PlatformPackageUnavailablePageProps {
  platformId: PlatformId;
  state: PlatformPackageState | null | undefined;
  className?: string;
}

export function PlatformPackageUnavailablePage({
  platformId,
  state,
  className,
}: PlatformPackageUnavailablePageProps) {
  const { t } = useTranslation();
  const { showModal } = useGlobalModal();
  const installPackage = usePlatformPackageStore((store) => store.installPackage);
  const refreshPackages = usePlatformPackageStore((store) => store.refresh);
  const [actionKey, setActionKey] = useState<string | null>(null);
  const [operationError, setOperationError] = useState<string | null>(null);
  const platformName = getPlatformLabel(platformId, t);
  const needsRepair = Boolean(
    state
    && (
      state.installStatus === 'error'
      || state.installStatus === 'incompatible'
      || (!state.runtimeReady && state.installStatus !== 'notInstalled')
    ),
  );
  const statusText = state
    ? getPlatformPackageStatusText(state, t)
    : t('common.loading', '加载中...');
  const stateOperating = state?.installStatus === 'installing'
    || state?.installStatus === 'updating'
    || state?.installStatus === 'uninstalling';
  const operating = actionKey === 'install' || stateOperating;
  const canInstall = Boolean(
    state
    && state.packageMode === 'hotUpdate'
    && !stateOperating
    && (
      state.installStatus === 'notInstalled'
      || state.installStatus === 'error'
      || (!state.runtimeReady && state.installStatus !== 'incompatible')
    ),
  );

  const runInstall = useCallback(async () => {
    setActionKey('install');
    setOperationError(null);
    try {
      let nextState = await installPackage(platformId);
      if (typeof window !== 'undefined') {
        window.dispatchEvent(
          new CustomEvent('agtools:platform-package-changed', {
            detail: nextState,
          }),
        );
      }
      if (!nextState.runtimeReady) {
        try {
          const refreshedPackages = await refreshPackages();
          const refreshedState = getPlatformPackageFromPackages(refreshedPackages, platformId);
          if (refreshedState) {
            nextState = refreshedState;
            if (typeof window !== 'undefined') {
              window.dispatchEvent(
                new CustomEvent('agtools:platform-package-changed', {
                  detail: nextState,
                }),
              );
            }
          }
        } catch {
          // Keep the original action result; the operation error below will surface it.
        }
      }
      if (!nextState.runtimeReady) {
        throw new Error(
          nextState.errorMessage || t('platformLayout.packageInstallNotReady', '平台包已处理，但运行组件尚未就绪'),
        );
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setOperationError(message);
      throw error;
    } finally {
      setActionKey(null);
    }
  }, [installPackage, platformId, refreshPackages, t]);

  const confirmInstall = useCallback(() => {
    if (!state || !canInstall || operating) {
      return;
    }

    const version = state.latestVersion || state.installedVersion || '--';
    const size = formatPlatformPackageSize(state.downloadSizeBytes);
    const isRepair = state.installStatus === 'error' || (
      !state.runtimeReady
      && state.installStatus !== 'notInstalled'
      && state.installStatus !== 'incompatible'
    );

    showModal({
      title: isRepair
        ? t('platformLayout.packageRepairConfirmTitle', {
            platform: platformName,
            defaultValue: '修复 {{platform}} 平台包',
          })
        : t('platformLayout.packageInstallConfirmTitle', {
            platform: platformName,
            defaultValue: '安装 {{platform}} 平台包',
          }),
      description: isRepair
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
            defaultValue: '{{platform}} 需要先下载平台包后才能打开。版本 {{version}}，大小 {{size}}。',
          }),
      content: (
        <PlatformPackageOperationProgress
          platformId={platformId}
          operation="install"
          fallbackTotalBytes={state.downloadSizeBytes}
        />
      ),
      width: 'sm',
      actions: [
        {
          id: 'cancel',
          label: t('common.cancel', '取消'),
          variant: 'secondary',
        },
        {
          id: 'platform-package-unavailable-install',
          label: isRepair
            ? t('platformLayout.packageRepair', '修复')
            : t('platformLayout.packageInstallAndOpen', '安装并打开'),
          variant: 'primary',
          onClick: runInstall,
        },
      ],
    });
  }, [canInstall, operating, platformId, platformName, runInstall, showModal, state, t]);

  return (
    <section
      className={`platform-package-unavailable-page is-${state?.installStatus || 'loading'} ${className || ''}`.trim()}
      role="status"
      aria-live="polite"
    >
      <div className="platform-package-unavailable-icon">
        {renderPlatformIcon(platformId, 42)}
      </div>
      <div className="platform-package-unavailable-copy">
        <h2>
          {needsRepair
            ? t('platformLayout.packagePageRepairTitle', {
                platform: platformName,
                defaultValue: '{{platform}} 平台包需要修复',
              })
            : t('platformLayout.packagePageInstallTitle', {
                platform: platformName,
                defaultValue: '{{platform}} 平台包未安装',
              })}
        </h2>
        <p>
          {state?.errorMessage
            || t('platformLayout.packagePageInstallDesc', {
              platform: platformName,
              defaultValue: '可直接在此页面安装，也可使用右上角的平台包操作安装或修复后再管理账号。安装前不会加载账号、OAuth、切号或后台刷新逻辑。',
            })}
        </p>
        <div className="platform-package-unavailable-status" title={statusText}>
          {statusText}
        </div>
        {canInstall && (
          <div className="platform-package-unavailable-actions">
            <button
              type="button"
              className="platform-package-unavailable-action"
              onClick={confirmInstall}
              disabled={operating}
              title={needsRepair
                ? t('platformLayout.packageRepairAndOpen', '修复并打开')
                : t('platformLayout.packageInstallAndOpen', '安装并打开')}
            >
              {operating
                ? <RefreshCw size={15} className="loading-spinner" />
                : <Download size={15} />}
              <span>
                {needsRepair
                  ? t('platformLayout.packageRepairAndOpen', '修复并打开')
                  : t('platformLayout.packageInstallAndOpen', '安装并打开')}
              </span>
            </button>
          </div>
        )}
        {operationError && (
          <div className="platform-package-unavailable-error" role="alert">
            {operationError}
          </div>
        )}
      </div>
    </section>
  );
}
