import { useTranslation } from 'react-i18next';
import { PlatformInstancesContent } from '../components/platform/PlatformInstancesContent';
import { useCodexInstanceStore } from '../stores/useCodexInstanceStore';
import { useCodexAccountStore } from '../stores/useCodexAccountStore';
import type { CodexAccount } from '../types/codex';
import {
  getCodexPlanDisplayName,
  getCodexQuotaClass,
  getCodexQuotaWindows,
} from '../types/codex';
import { usePlatformRuntimeSupport } from '../hooks/usePlatformRuntimeSupport';

/**
 * Codex 多开实例内容组件（不包含 header）
 * 用于嵌入到 CodexAccountsPage 中
 */
export function CodexInstancesContent() {
  const { t } = useTranslation();
  const instanceStore = useCodexInstanceStore();
  const { accounts, fetchAccounts } = useCodexAccountStore();
  const isSupportedPlatform = usePlatformRuntimeSupport('macos-only');

  const resolveQuotaClass = (percentage: number) => {
    const mapped = getCodexQuotaClass(percentage);
    return mapped === 'critical' ? 'low' : mapped;
  };

  const renderCodexQuotaPreview = (account: CodexAccount) => {
    if (!account.quota) {
      return <span className="account-quota-empty">{t('instances.quota.empty', '暂无配额缓存')}</span>;
    }
    const windows = getCodexQuotaWindows(account.quota);
    if (windows.length === 0) {
      return <span className="account-quota-empty">{t('instances.quota.empty', '暂无配额缓存')}</span>;
    }
    return (
      <div className="account-quota-preview">
        {windows.map((window) => (
          <span className="account-quota-item" key={window.id}>
            <span className={`quota-dot ${resolveQuotaClass(window.percentage)}`} />
            <span className={`quota-text ${resolveQuotaClass(window.percentage)}`}>
              {window.label} {window.percentage}%
            </span>
          </span>
        ))}
      </div>
    );
  };

  const renderCodexPlanBadge = (account: CodexAccount) => {
    const planName = getCodexPlanDisplayName(account.plan_type);
    return <span className={`instance-plan-badge ${planName.toLowerCase()}`}>{planName}</span>;
  };

  return (
    <PlatformInstancesContent
      instanceStore={instanceStore}
      accounts={accounts}
      fetchAccounts={fetchAccounts}
      renderAccountQuotaPreview={renderCodexQuotaPreview}
      renderAccountBadge={renderCodexPlanBadge}
      getAccountSearchText={(account) =>
        `${account.email} ${getCodexPlanDisplayName(account.plan_type)}`
      }
      appType="codex"
      isSupported={isSupportedPlatform}
      unsupportedTitleKey="common.shared.instances.unsupported.title"
      unsupportedTitleDefault="暂不支持当前系统"
      unsupportedDescKey="codex.instances.unsupported.desc"
      unsupportedDescDefault="Codex 多开实例仅支持 macOS。"
    />
  );
}
