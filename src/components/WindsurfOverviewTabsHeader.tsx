import { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Layers } from 'lucide-react';
import { WindsurfIcon } from './icons/WindsurfIcon';

export type WindsurfTab = 'overview' | 'instances';

interface WindsurfOverviewTabsHeaderProps {
  active: WindsurfTab;
  onTabChange?: (tab: WindsurfTab) => void;
}

interface TabSpec {
  key: WindsurfTab;
  label: string;
  icon: ReactNode;
}

export function WindsurfOverviewTabsHeader({
  active,
  onTabChange,
}: WindsurfOverviewTabsHeaderProps) {
  const { t } = useTranslation();
  
  const tabs: TabSpec[] = [
    {
      key: 'overview',
      label: t('windsurf.overview.title', '账号总览'),
      icon: <WindsurfIcon className="tab-icon" />,
    },
    {
      key: 'instances',
      label: t('windsurf.instances.title', '多开实例'),
      icon: <Layers className="tab-icon" />,
    },
  ];

  const subtitle = active === 'instances'
    ? t('windsurf.instances.subtitle', '多实例独立配置，多账号并行运行。')
    : t('windsurf.subtitle', '实时监控所有账号的配额状态。');

  return (
    <>
      <div className="page-header">
        <div className="page-title">{t('windsurf.title', 'Windsurf 账号管理')}</div>
        <div className="page-subtitle">{subtitle}</div>
      </div>
      <div className="page-tabs-row page-tabs-center">
        <div className="page-tabs filter-tabs">
          {tabs.map((tab) => (
            <button
              key={tab.key}
              className={`filter-tab${active === tab.key ? ' active' : ''}`}
              onClick={() => onTabChange?.(tab.key)}
            >
              {tab.icon}
              <span>{tab.label}</span>
            </button>
          ))}
        </div>
      </div>
    </>
  );
}
