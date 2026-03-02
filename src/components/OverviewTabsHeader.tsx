import { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { AlarmClock, Fingerprint, Layers, ShieldCheck, HelpCircle } from 'lucide-react';
import { Page } from '../types/navigation';
import { RobotIcon } from './icons/RobotIcon';

interface OverviewTabsHeaderProps {
  active: Page;
  onNavigate?: (page: Page) => void;
  subtitle: string;
  title?: string;
  onOpenManual?: () => void;
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
}: OverviewTabsHeaderProps) {
  const { t } = useTranslation();
  const tabs: TabSpec[] = [
    {
      key: 'overview',
      label: t('overview.title'),
      icon: <RobotIcon className="tab-icon" />,
    },
    {
      key: 'instances',
      label: t('instances.title', '多开实例'),
      icon: <Layers className="tab-icon" />,
    },
    {
      key: 'fingerprints',
      label: t('fingerprints.title'),
      icon: <Fingerprint className="tab-icon" />,
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
      <div className="page-header">
        <div className="page-title" style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          {title ?? t('overview.brandTitle')}
          {onOpenManual && (
            <button
              className="btn btn-secondary icon-only"
              onClick={onOpenManual}
              title={t('manual.navTitle', '功能使用手册')}
              style={{ padding: '6px', borderRadius: '50%', background: 'transparent', border: 'none', color: 'var(--text-secondary)' }}
            >
              <HelpCircle size={18} />
            </button>
          )}
        </div>
        <div className="page-subtitle">{subtitle}</div>
      </div>
      <div className="page-tabs-row page-tabs-center">
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
      </div>
    </>
  );
}
