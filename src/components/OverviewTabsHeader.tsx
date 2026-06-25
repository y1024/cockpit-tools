import { ReactNode, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { AlarmClock, Layers, ShieldCheck } from 'lucide-react';
import { Page } from '../types/navigation';
import { ManualHelpIconButton } from './ManualHelpIconButton';
import { TopCenterPromoBanner } from './TopCenterPromoBanner';
import { PlatformId } from '../types/platform';
import type { PlatformPackageState } from '../types/platformPackage';
import {
  findGroupByPlatform,
  resolveGroupChildName,
  usePlatformLayoutStore,
} from '../stores/usePlatformLayoutStore';
import { getPlatformLabel, renderPlatformIcon } from '../utils/platformMeta';
import { PlatformGroupSwitcher } from './platform/PlatformGroupSwitcher';
import { useAntigravityRuntimeTarget } from '../hooks/useAntigravityRuntimeTarget';
import { PlatformPackageToolbar } from './PlatformPackageToolbar';

interface OverviewTabsHeaderProps {
  active: Page;
  onNavigate?: (page: Page) => void;
  subtitle: string;
  title?: string;
  onOpenManual?: () => void;
  rightSlot?: ReactNode;
  hideTabs?: boolean;
  remoteTabsSlotId?: string;
}

interface TabSpec {
  key: Page;
  label: string;
  icon: ReactNode;
}

export function OverviewTabsHeader({
  active,
  onNavigate,
  subtitle,
  title,
  onOpenManual,
  rightSlot,
  hideTabs = false,
  remoteTabsSlotId,
}: OverviewTabsHeaderProps) {
  void subtitle;
  const { t } = useTranslation();
  const { platformGroups } = usePlatformLayoutStore();
  const currentPlatformId: PlatformId = useAntigravityRuntimeTarget();
  const currentGroup = useMemo(
    () => findGroupByPlatform(platformGroups, currentPlatformId),
    [platformGroups, currentPlatformId],
  );
  const switchablePlatforms = currentGroup ? currentGroup.platformIds : [currentPlatformId];
  const currentPlatformLabel = getPlatformLabel(currentPlatformId, t);
  const currentDisplayName = useMemo(
    () =>
      title
        ? title
        : currentGroup
          ? resolveGroupChildName(currentGroup, currentPlatformId, currentPlatformLabel)
          : currentPlatformLabel,
    [title, currentGroup, currentPlatformId, currentPlatformLabel],
  );
  const switchOptions = useMemo(
    () =>
      switchablePlatforms.map((platformId) => ({
        platformId,
        label: currentGroup
          ? resolveGroupChildName(currentGroup, platformId, getPlatformLabel(platformId, t))
          : getPlatformLabel(platformId, t),
      })),
    [switchablePlatforms, currentGroup, t],
  );
  const antigravityPackageState = useMemo<PlatformPackageState>(() => ({
    platformId: currentPlatformId,
    packageMode: 'bundled',
    installKind: 'coreNativeBoundary',
    installStatus: 'installed',
    runtimeReady: true,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: {
      platforms: [],
      dataPaths: [],
      localStorageKeys: [],
      nativeBoundaries: ['antigravity.native'],
    },
    changelog: [],
  }), [currentPlatformId]);
  const tabs: TabSpec[] = [
    {
      key: 'overview',
      label: t('overview.title'),
      icon: <span className="tab-icon">{renderPlatformIcon(currentPlatformId, 16)}</span>,
    },
    {
      key: 'instances',
      label: t('instances.title', '多开实例'),
      icon: <Layers className="tab-icon" />,
    },
    {
      key: 'wakeup',
      label: t('wakeup.title'),
      icon: <AlarmClock className="tab-icon" />,
    },
    {
      key: 'verification',
      label: t('wakeup.verification.title'),
      icon: <ShieldCheck className="tab-icon" />,
    },
  ];

  return (
    <>
      <div className="page-top-strip">
        <div className="page-top-strip-left">
          <span className="page-top-strip-label">
            {t('settings.general.account', '账号')}
          </span>
          <ManualHelpIconButton className="platform-header-help" onClick={onOpenManual} />
        </div>
        <TopCenterPromoBanner />
        <div className="page-top-strip-right page-top-strip-right-slot">
          {rightSlot ?? (
            <PlatformPackageToolbar
              platformId={currentPlatformId}
              fallbackState={antigravityPackageState}
            />
          )}
        </div>
      </div>
      <div className="page-tabs-row page-tabs-center page-tabs-row-with-leading">
        <div className="page-tabs-leading">
          <PlatformGroupSwitcher
            currentPlatformId={currentPlatformId}
            currentLabel={currentDisplayName}
            options={switchOptions}
            currentGroupId={currentGroup?.id ?? null}
          />
        </div>
        {remoteTabsSlotId ? (
          <div
            id={remoteTabsSlotId}
            className="page-tabs filter-tabs platform-remote-tabs-slot"
          />
        ) : !hideTabs && (
          <div className="page-tabs filter-tabs">
            {tabs.map((tab) => (
              <button
                key={tab.key}
                className={`filter-tab${active === tab.key ? ' active' : ''}`}
                onClick={() => onNavigate?.(tab.key)}
              >
                {tab.icon}
                <span>{tab.label}</span>
              </button>
            ))}
          </div>
        )}
      </div>
    </>
  );
}
