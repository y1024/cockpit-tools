import { useEffect, useMemo } from 'react';
import { OverviewTabsHeader } from '../components/OverviewTabsHeader';
import { PlatformPackageToolbar } from '../components/PlatformPackageToolbar';
import { PlatformPackageUnavailablePage } from '../components/PlatformPackageUnavailablePage';
import { PlatformRuntimePageHost } from '../components/platform/PlatformRuntimePageHost';
import { useAntigravityRuntimeTarget } from '../hooks/useAntigravityRuntimeTarget';
import {
  getPlatformPackageFromPackages,
  usePlatformPackageStore,
} from '../stores/usePlatformPackageStore';
import type { Page } from '../types/navigation';

const ANTIGRAVITY_REMOTE_TABS_SLOT_ID = 'antigravity-remote-tabs-slot';

type AntigravitySuiteTab = Extract<Page, 'overview' | 'instances' | 'wakeup' | 'verification'>;

interface AntigravitySuitePageProps {
  initialTab?: AntigravitySuiteTab;
  onNavigate?: (page: Page) => void;
}

function normalizeTab(value: Page | undefined): AntigravitySuiteTab {
  if (value === 'instances' || value === 'wakeup' || value === 'verification') {
    return value;
  }
  return 'overview';
}

export function AntigravitySuitePage({
  initialTab,
  onNavigate,
}: AntigravitySuitePageProps) {
  const platformId = useAntigravityRuntimeTarget();
  const packages = usePlatformPackageStore((state) => state.packages);
  const initialized = usePlatformPackageStore((state) => state.initialized);
  const refreshPlatformPackages = usePlatformPackageStore((state) => state.refresh);
  const platformPackage = useMemo(
    () => getPlatformPackageFromPackages(packages, platformId),
    [packages, platformId],
  );
  const activeTab = normalizeTab(initialTab);
  const runtimeParams = useMemo(
    () => ({ initialTab: activeTab }),
    [activeTab],
  );

  useEffect(() => {
    if (initialized) {
      return;
    }
    void refreshPlatformPackages().catch((error) => {
      console.error('Failed to refresh Antigravity platform package:', error);
    });
  }, [initialized, refreshPlatformPackages]);

  const runtimeReady = Boolean(
    platformPackage
    && platformPackage.packageMode === 'hotUpdate'
    && platformPackage.runtimeReady
    && (
      platformPackage.installStatus === 'installed'
      || platformPackage.installStatus === 'updateAvailable'
    ),
  );

  return (
    <div className="antigravity-suite-page">
      <OverviewTabsHeader
        active={activeTab}
        onNavigate={onNavigate}
        subtitle=""
        hideTabs={runtimeReady}
        remoteTabsSlotId={runtimeReady ? ANTIGRAVITY_REMOTE_TABS_SLOT_ID : undefined}
        rightSlot={<PlatformPackageToolbar platformId={platformId} />}
      />

      {!runtimeReady ? (
        <PlatformPackageUnavailablePage platformId={platformId} state={platformPackage} />
      ) : platformPackage ? (
        <PlatformRuntimePageHost
          platformId={platformId}
          state={platformPackage}
          tabsSlotId={ANTIGRAVITY_REMOTE_TABS_SLOT_ID}
          runtimeParams={runtimeParams}
        />
      ) : (
        <PlatformPackageUnavailablePage platformId={platformId} state={platformPackage} />
      )}
    </div>
  );
}
