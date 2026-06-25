import React, { useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import { createRoot, type Root } from 'react-dom/client';
import { AlarmClock, Layers, ShieldCheck } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { initI18n, syncLanguage } from '../../i18n';
import { AccountsPage } from '../../pages/AccountsPage';
import { InstancesPage } from '../../pages/InstancesPage';
import { WakeupTasksPage } from '../../pages/WakeupTasksPage';
import { WakeupVerificationPage } from '../../pages/WakeupVerificationPage';
import type { Page } from '../../types/navigation';
import type { PlatformId } from '../../types/platform';
import { renderPlatformIcon } from '../../utils/platformMeta';

type AntigravityPlatformId = Extract<PlatformId, 'antigravity' | 'antigravity_ide'>;
type AntigravitySuiteTab = Extract<Page, 'overview' | 'instances' | 'wakeup' | 'verification'>;

type AntigravityRemoteHostApi = {
  platformId: AntigravityPlatformId;
  packageVersion?: string | null;
  locale?: string | null;
  theme?: string | null;
  tabsSlotId?: string | null;
  runtimeParams?: {
    initialTab?: unknown;
  } | null;
};

const roots = new WeakMap<HTMLElement, Root>();

function normalizeTheme(theme: string | null | undefined): string {
  return theme && theme.trim() ? theme : document.documentElement.dataset.theme || 'dark';
}

function normalizeLocale(locale: string | null | undefined): string {
  return locale && locale.trim() ? locale : 'zh-CN';
}

function normalizeTab(value: unknown): AntigravitySuiteTab {
  if (value === 'instances' || value === 'wakeup' || value === 'verification') {
    return value;
  }
  return 'overview';
}

function AntigravityRemoteTabs({
  platformId,
  activeTab,
  onTabChange,
}: {
  platformId: AntigravityPlatformId;
  activeTab: AntigravitySuiteTab;
  onTabChange: (tab: AntigravitySuiteTab) => void;
}) {
  const { t } = useTranslation();
  const tabs = useMemo(
    () => [
      {
        key: 'overview' as const,
        label: t('overview.title', '账号总览'),
        icon: <span className="tab-icon">{renderPlatformIcon(platformId, 16)}</span>,
      },
      {
        key: 'instances' as const,
        label: t('instances.title', '多开实例'),
        icon: <Layers className="tab-icon" />,
      },
      {
        key: 'wakeup' as const,
        label: t('wakeup.title', '唤醒任务'),
        icon: <AlarmClock className="tab-icon" />,
      },
      {
        key: 'verification' as const,
        label: t('wakeup.verification.title', '账户检测'),
        icon: <ShieldCheck className="tab-icon" />,
      },
    ],
    [platformId, t],
  );

  return (
    <>
      {tabs.map((tab) => (
        <button
          key={tab.key}
          type="button"
          className={`filter-tab${activeTab === tab.key ? ' active' : ''}`}
          aria-current={activeTab === tab.key ? 'page' : undefined}
          onClick={() => onTabChange(tab.key)}
        >
          {tab.icon}
          <span>{tab.label}</span>
        </button>
      ))}
    </>
  );
}

function AntigravityRemoteContent({
  activeTab,
  onNavigate,
}: {
  activeTab: AntigravitySuiteTab;
  onNavigate: (page: Page) => void;
}) {
  switch (activeTab) {
    case 'instances':
      return <InstancesPage hideHeader onNavigate={onNavigate} />;
    case 'wakeup':
      return <WakeupTasksPage hideHeader onNavigate={onNavigate} />;
    case 'verification':
      return <WakeupVerificationPage hideHeader onNavigate={onNavigate} />;
    default:
      return <AccountsPage hideHeader onNavigate={onNavigate} />;
  }
}

function AntigravityRemoteApp({
  hostApi,
  tabsContainer,
}: {
  hostApi: AntigravityRemoteHostApi;
  tabsContainer: HTMLElement | null;
}) {
  const [activeTab, setActiveTab] = useState<AntigravitySuiteTab>(() =>
    normalizeTab(hostApi.runtimeParams?.initialTab),
  );

  const handleNavigate = (page: Page) => {
    if (page === 'overview' || page === 'instances' || page === 'wakeup' || page === 'verification') {
      setActiveTab(page);
      return;
    }
    window.dispatchEvent(new CustomEvent('app-request-navigate', { detail: page }));
  };

  const rootClassName = `${hostApi.platformId}-platform-ui-root`;

  return (
    <React.StrictMode>
      {tabsContainer
        ? createPortal(
            <AntigravityRemoteTabs
              platformId={hostApi.platformId}
              activeTab={activeTab}
              onTabChange={setActiveTab}
            />,
            tabsContainer,
          )
        : null}
      <div className={`antigravity-suite-remote ${rootClassName}`}>
        <AntigravityRemoteContent activeTab={activeTab} onNavigate={handleNavigate} />
      </div>
    </React.StrictMode>
  );
}

export async function mountAntigravityRemote(
  container: HTMLElement,
  hostApi: AntigravityRemoteHostApi,
) {
  unmountAntigravityRemote(container);

  const theme = normalizeTheme(hostApi.theme);
  const locale = normalizeLocale(hostApi.locale);
  document.documentElement.dataset.theme = theme;
  document.documentElement.lang = locale;

  await initI18n();
  await syncLanguage(locale);

  const tabsContainer = hostApi.tabsSlotId
    ? document.getElementById(hostApi.tabsSlotId)
    : null;
  const root = createRoot(container);
  roots.set(container, root);
  root.render(<AntigravityRemoteApp hostApi={hostApi} tabsContainer={tabsContainer} />);

  return () => unmountAntigravityRemote(container);
}

export function unmountAntigravityRemote(container: HTMLElement) {
  const root = roots.get(container);
  if (!root) return;
  root.unmount();
  roots.delete(container);
}
