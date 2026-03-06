import { useTranslation } from 'react-i18next';
import { PlatformInstancesContent } from '../components/platform/PlatformInstancesContent';
import { useCodexInstanceStore } from '../stores/useCodexInstanceStore';
import { useCodexAccountStore } from '../stores/useCodexAccountStore';
import type { CodexAccount } from '../types/codex';
import { usePlatformRuntimeSupport } from '../hooks/usePlatformRuntimeSupport';
import {
  buildCodexAccountPresentation,
  buildQuotaPreviewLines,
} from '../presentation/platformAccountPresentation';

/**
 * Codex 多开实例内容组件（不包含 header）
 * 用于嵌入到 CodexAccountsPage 中
 */
interface CodexInstancesContentProps {
  accountsForSelect?: CodexAccount[];
}

export function CodexInstancesContent({ accountsForSelect }: CodexInstancesContentProps = {}) {
  const { t } = useTranslation();
  const instanceStore = useCodexInstanceStore();
  const { accounts: storeAccounts, fetchAccounts } = useCodexAccountStore();
  const accounts = accountsForSelect ?? storeAccounts;
  const isSupportedPlatform = usePlatformRuntimeSupport('macos-only');

  const renderCodexQuotaPreview = (account: CodexAccount) => {
    const presentation = buildCodexAccountPresentation(account, t);
    const lines = buildQuotaPreviewLines(presentation.quotaItems, 3);
    if (lines.length === 0) {
      return <span className="account-quota-empty">{t('instances.quota.empty', '暂无配额缓存')}</span>;
    }
    return (
      <div className="account-quota-preview">
        {lines.map((line) => (
          <span className="account-quota-item" key={line.key}>
            <span className={`quota-dot ${line.quotaClass}`} />
            <span className={`quota-text ${line.quotaClass}`}>
              {line.text}
            </span>
          </span>
        ))}
      </div>
    );
  };

  const renderCodexPlanBadge = (account: CodexAccount) => {
    const presentation = buildCodexAccountPresentation(account, t);
    return <span className={`instance-plan-badge ${presentation.planClass}`}>{presentation.planLabel}</span>;
  };

  return (
    <PlatformInstancesContent
      instanceStore={instanceStore}
      accounts={accounts}
      fetchAccounts={fetchAccounts}
      renderAccountQuotaPreview={renderCodexQuotaPreview}
      renderAccountBadge={renderCodexPlanBadge}
      getAccountSearchText={(account) => {
        const presentation = buildCodexAccountPresentation(account, t);
        return `${presentation.displayName} ${presentation.planLabel}`;
      }}
      appType="codex"
      isSupported={isSupportedPlatform}
      unsupportedTitleKey="common.shared.instances.unsupported.title"
      unsupportedTitleDefault="暂不支持当前系统"
      unsupportedDescKey="codex.instances.unsupported.desc"
      unsupportedDescDefault="Codex 多开实例仅支持 macOS。"
    />
  );
}
