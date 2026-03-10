import { useMemo, useCallback, Fragment, useState } from 'react';
import {
  Plus, RefreshCw, Download, Upload, Trash2, X, Globe, KeyRound, Database,
  Copy, Check, RotateCw, LayoutGrid, List, Search,
  Tag, Play, Eye, EyeOff, CircleAlert, ChevronDown,
} from 'lucide-react';
import { confirm as confirmDialog } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useCodebuddyAccountStore } from '../stores/useCodebuddyAccountStore';
import * as codebuddyService from '../services/codebuddyService';
import { TagEditModal } from '../components/TagEditModal';
import { ExportJsonModal } from '../components/ExportJsonModal';
import {
  CodebuddyAccount,
  getCodebuddyAccountDisplayEmail,
  getCodebuddyPlanBadge,
  getCodebuddyUsage,
  getCodebuddyResourceSummary,
  getCodebuddyExtraCreditSummary,
} from '../types/codebuddy';
import { QuickSettingsPopover } from '../components/QuickSettingsPopover';
import { useProviderAccountsPage } from '../hooks/useProviderAccountsPage';
import { PlatformOverviewTabsHeader, PlatformOverviewTab } from '../components/platform/PlatformOverviewTabsHeader';
import { CodebuddyInstancesContent } from './CodebuddyInstancesPage';

const CB_FLOW_NOTICE_COLLAPSED_KEY = 'agtools.codebuddy.flow_notice_collapsed';
const CB_CURRENT_ACCOUNT_ID_KEY = 'agtools.codebuddy.current_account_id';
const CB_KNOWN_PLAN_FILTERS = ['FREE', 'TRIAL', 'PRO', 'ENTERPRISE'] as const;
const CB_QUOTA_BINDING_MISMATCH_CODE = 'CODEBUDDY_QUOTA_BINDING_ACCOUNT_MISMATCH';
const CODEBUDDY_USAGE_URL = 'https://www.codebuddy.ai/profile/usage';

type QuotaQueryFormState = {
  cookieHeader: string;
};

function createDefaultQuotaQueryForm(): QuotaQueryFormState {
  return {
    cookieHeader: '',
  };
}

type QuotaBindingMismatchMeta = {
  expectedUin: string;
  resourceUin: string;
  payerUin: string;
};

function parseQuotaBindingMismatchMeta(raw: string): QuotaBindingMismatchMeta | null {
  const markerIndex = raw.indexOf(CB_QUOTA_BINDING_MISMATCH_CODE);
  if (markerIndex < 0) return null;
  const payload = raw.slice(markerIndex + CB_QUOTA_BINDING_MISMATCH_CODE.length);
  const segments = payload.split('|').map((item) => item.trim()).filter(Boolean);
  const meta: Record<string, string> = {};
  segments.forEach((segment) => {
    const eqIndex = segment.indexOf('=');
    if (eqIndex <= 0) return;
    const key = segment.slice(0, eqIndex).trim();
    const value = segment.slice(eqIndex + 1).trim();
    meta[key] = value;
  });
  if (!meta.expected_uin) return null;
  return {
    expectedUin: meta.expected_uin || '--',
    resourceUin: meta.resource_uin || '--',
    payerUin: meta.payer_uin || '--',
  };
}

export function CodebuddyAccountsPage() {
  const [activeTab, setActiveTab] = useState<PlatformOverviewTab>('overview');
  const [quotaQueryAccountId, setQuotaQueryAccountId] = useState<string | null>(null);
  const [quotaQueryForm, setQuotaQueryForm] = useState<QuotaQueryFormState>(createDefaultQuotaQueryForm);
  const [quotaQuerySubmitting, setQuotaQuerySubmitting] = useState(false);
  const [quotaBindingClearing, setQuotaBindingClearing] = useState(false);
  const [quotaQueryModalError, setQuotaQueryModalError] = useState<string | null>(null);
  const untaggedKey = '__untagged__';
  const store = useCodebuddyAccountStore();

  const page = useProviderAccountsPage<CodebuddyAccount>({
    platformKey: 'CodeBuddy',
    oauthLogPrefix: 'CodebuddyOAuth',
    flowNoticeCollapsedKey: CB_FLOW_NOTICE_COLLAPSED_KEY,
    currentAccountIdKey: CB_CURRENT_ACCOUNT_ID_KEY,
    exportFilePrefix: 'codebuddy_accounts',
    oauthTabKeys: ['oauth'],
    store: {
      accounts: store.accounts,
      loading: store.loading,
      fetchAccounts: store.fetchAccounts,
      deleteAccounts: store.deleteAccounts,
      refreshToken: store.refreshToken,
      refreshAllTokens: store.refreshAllTokens,
      updateAccountTags: store.updateAccountTags,
    },
    oauthService: {
      startLogin: codebuddyService.startCodebuddyOAuthLogin,
      completeLogin: codebuddyService.completeCodebuddyOAuthLogin,
      cancelLogin: codebuddyService.cancelCodebuddyOAuthLogin,
    },
    dataService: {
      importFromJson: codebuddyService.importCodebuddyFromJson,
      importFromLocal: codebuddyService.importCodebuddyFromLocal,
      addWithToken: codebuddyService.addCodebuddyAccountWithToken,
      exportAccounts: codebuddyService.exportCodebuddyAccounts,
      injectToVSCode: codebuddyService.injectCodebuddyToVSCode,
    },
    getDisplayEmail: (account) => getCodebuddyAccountDisplayEmail(account),
  });

  const {
    t, locale, privacyModeEnabled, togglePrivacyMode, maskAccountText,
    viewMode, setViewMode, searchQuery, setSearchQuery, filterType, setFilterType,
    sortDirection, sortBy,
    selected, toggleSelect, toggleSelectAll,
    tagFilter, groupByTag, setGroupByTag, showTagFilter, setShowTagFilter,
    showTagModal, setShowTagModal, tagFilterRef, availableTags,
    toggleTagFilterValue, clearTagFilter, tagDeleteConfirm, setTagDeleteConfirm,
    deletingTag, confirmDeleteTag, openTagModal, handleSaveTags,
    refreshing, refreshingAll, injecting,
    handleRefresh, handleRefreshAll, handleDelete, handleBatchDelete,
    deleteConfirm, setDeleteConfirm, deleting, confirmDelete,
    message, setMessage,
    exporting, handleExport, handleExportByIds,
    showExportModal, exportJsonContent, exportJsonHidden,
    toggleExportJsonHidden, exportJsonCopied, copyExportJson,
    savingExportJson, saveExportJson, exportSavedPath,
    canOpenExportSavedDirectory, openExportSavedDirectory, copyExportSavedPath, exportPathCopied,
    closeExportModal,
    showAddModal, addTab, addStatus, addMessage, tokenInput, setTokenInput,
    importing, openAddModal, closeAddModal,
    handleTokenImport, handleImportJsonFile, handleImportFromLocal, handlePickImportFile, importFileInputRef,
    oauthUrl, oauthUrlCopied, oauthUserCode, oauthUserCodeCopied, oauthMeta,
    oauthPolling, oauthTimedOut,
    oauthPrepareError, oauthCompleteError,
    handleCopyOauthUrl, handleCopyOauthUserCode, handleRetryOauth, handleOpenOauthUrl,
    handleInjectToVSCode,
    isFlowNoticeCollapsed, setIsFlowNoticeCollapsed,
    currentAccountId, formatDate, normalizeTag,
  } = page;

  const accounts = store.accounts;
  const loading = store.loading;
  const quotaQueryAccount = useMemo(
    () => accounts.find((item) => item.id === quotaQueryAccountId) ?? null,
    [accounts, quotaQueryAccountId],
  );
  const quotaQueryHasBinding = !!quotaQueryAccount?.quota_binding;

  const openQuotaQueryModal = useCallback((account: CodebuddyAccount) => {
    setQuotaQueryForm({
      cookieHeader: '',
    });
    setQuotaQueryModalError(null);
    setQuotaQueryAccountId(account.id);
  }, []);

  const closeQuotaQueryModal = useCallback(() => {
    if (quotaQuerySubmitting || quotaBindingClearing) return;
    setQuotaQueryModalError(null);
    setQuotaQueryAccountId(null);
  }, [quotaBindingClearing, quotaQuerySubmitting]);

  const handleCopyQuotaUsageUrl = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(CODEBUDDY_USAGE_URL);
      setMessage({
        text: t('wakeup.errorUi.copySuccess', '已复制'),
      });
    } catch (_err) {
      setMessage({
        tone: 'error',
        text: t('wakeup.errorUi.copyFailed', '复制失败'),
      });
    }
  }, [setMessage, t]);

  const handleOpenQuotaUsageUrl = useCallback(async () => {
    try {
      await openUrl(CODEBUDDY_USAGE_URL);
    } catch (_err) {
      window.open(CODEBUDDY_USAGE_URL, '_blank', 'noopener,noreferrer');
      setMessage({
        tone: 'error',
        text: t('wakeup.errorUi.openFailed', '打开链接失败，已尝试使用浏览器打开'),
      });
    }
  }, [setMessage, t]);

  const setQuotaQueryField = useCallback((key: keyof QuotaQueryFormState, value: string) => {
    setQuotaQueryModalError(null);
    setQuotaQueryForm((prev) => ({ ...prev, [key]: value }));
  }, []);

  const resolveQuotaClass = useCallback((remainPercent: number | null): 'high' | 'medium' | 'critical' => {
    if (remainPercent == null) return 'high';
    if (remainPercent >= 60) return 'high';
    if (remainPercent >= 30) return 'medium';
    return 'critical';
  }, []);

  const formatQuotaUpdatedTime = useCallback((ts: number | null) => {
    if (!ts || !Number.isFinite(ts)) return null;
    const date = new Date(ts);
    if (Number.isNaN(date.getTime())) return null;
    return date.toLocaleString();
  }, []);

  const formatQuotaValue = useCallback((value: number | null | undefined) => {
    if (value == null || !Number.isFinite(value)) return '--';
    return value.toFixed(2);
  }, []);

  const handleSubmitQuotaQuery = useCallback(async () => {
    if (!quotaQueryAccountId || quotaQuerySubmitting) return;
    const cookieHeader = quotaQueryForm.cookieHeader.trim();
    if (!cookieHeader) {
      setQuotaQueryModalError(t('codebuddy.quotaQuery.errors.cookieRequired', '请先粘贴完整 cURL 命令。'));
      return;
    }
    if (!/^\s*curl(?:\.exe)?\b/i.test(cookieHeader)) {
      setQuotaQueryModalError(
        t(
          'codebuddy.quotaQuery.errors.fullCurlRequired',
          '仅支持完整 cURL 命令。请在浏览器 Network 中对 get-user-resource 使用“Copy as cURL”后原样粘贴。',
        ),
      );
      return;
    }

    setQuotaQueryModalError(null);
    setQuotaQuerySubmitting(true);
    try {
      await codebuddyService.queryCodebuddyQuotaWithBinding({
        accountId: quotaQueryAccountId,
        cookieHeader,
      });
      await store.fetchAccounts();
      setMessage({
        text: t('codebuddy.quotaQuery.success', '配额查询成功，已绑定查询参数。'),
      });
      setQuotaQueryModalError(null);
      setQuotaQueryAccountId(null);
    } catch (err) {
      const rawError = String(err);
      await store.fetchAccounts();
      const mismatchMeta = parseQuotaBindingMismatchMeta(rawError);
      if (mismatchMeta) {
        setQuotaQueryModalError(
          t('codebuddy.quotaQuery.errors.accountMismatch', '配额 Cookie 对应账号与当前账号不一致（当前 UIN：{{expectedUin}}，返回 UIN：{{resourceUin}}，payerUin：{{payerUin}}）。', mismatchMeta),
        );
        return;
      }
      setQuotaQueryModalError(
        t('codebuddy.quotaQuery.failed', '配额查询失败：{{error}}', { error: rawError }),
      );
    } finally {
      setQuotaQuerySubmitting(false);
    }
  }, [quotaQueryAccountId, quotaQueryForm, quotaQuerySubmitting, setMessage, store, t]);

  const handleClearQuotaBinding = useCallback(async () => {
    if (!quotaQueryAccountId || quotaBindingClearing || quotaQuerySubmitting || !quotaQueryHasBinding) return;
    const confirmed = await confirmDialog(
      t(
        'codebuddy.quotaQuery.clearBindingConfirm',
        '确认清除当前账号的配额查询绑定参数？清除后将停止自动刷新配额。',
      ),
      {
        title: t('common.appName', 'Cockpit Tools'),
        kind: 'warning',
        okLabel: t('common.confirm', '确认'),
        cancelLabel: t('common.cancel', '取消'),
      },
    );
    if (!confirmed) return;

    setQuotaBindingClearing(true);
    try {
      await codebuddyService.clearCodebuddyQuotaBinding(quotaQueryAccountId);
      await store.fetchAccounts();
      setMessage({
        text: t('codebuddy.quotaQuery.clearBindingSuccess', '已清除配额查询绑定参数。'),
      });
      setQuotaQueryAccountId(null);
    } catch (err) {
      setMessage({
        tone: 'error',
        text: t('codebuddy.quotaQuery.clearBindingFailed', '清除配额绑定失败：{{error}}', {
          error: String(err),
        }),
      });
    } finally {
      setQuotaBindingClearing(false);
    }
  }, [
    quotaBindingClearing,
    quotaQueryAccountId,
    quotaQueryHasBinding,
    quotaQuerySubmitting,
    setMessage,
    store,
    t,
  ]);

  const resolvePlanKey = useCallback(
    (account: CodebuddyAccount) => getCodebuddyPlanBadge(account),
    [],
  );

  const resolveTierBadgeClass = useCallback((plan: string) => {
    switch (plan.toUpperCase()) {
      case 'FREE':
        return 'free';
      case 'TRIAL':
        return 'trial';
      case 'PRO':
        return 'pro';
      case 'ENTERPRISE':
        return 'enterprise';
      default:
        return 'unknown';
    }
  }, []);

  const tierSummary = useMemo(() => {
    const dynamicCounts = new Map<string, number>();
    accounts.forEach((account) => {
      const tier = resolvePlanKey(account);
      dynamicCounts.set(tier, (dynamicCounts.get(tier) ?? 0) + 1);
    });
    const extraKeys = Array.from(dynamicCounts.keys())
      .filter((tier) => !(CB_KNOWN_PLAN_FILTERS as readonly string[]).includes(tier))
      .sort((a, b) => a.localeCompare(b));
    return { all: accounts.length, dynamicCounts, extraKeys };
  }, [accounts, resolvePlanKey]);

  const filteredAccounts = useMemo(() => {
    let result = [...accounts];
    if (searchQuery.trim()) {
      const query = searchQuery.toLowerCase();
      result = result.filter((account) =>
        [account.email, account.nickname || '', account.uid || '', account.enterprise_name || '', account.id]
          .some((item) => item.toLowerCase().includes(query)),
      );
    }
    if (filterType !== 'all') {
      result = result.filter((account) => resolvePlanKey(account) === filterType);
    }
    if (tagFilter.length > 0) {
      const selectedTags = new Set(tagFilter.map(normalizeTag));
      result = result.filter((acc) => (acc.tags || []).map(normalizeTag).some((tag) => selectedTags.has(tag)));
    }
    result.sort((a, b) => {
      const diff = b.created_at - a.created_at;
      return sortDirection === 'desc' ? diff : -diff;
    });
    return result;
  }, [accounts, searchQuery, filterType, resolvePlanKey, tagFilter, normalizeTag, sortBy, sortDirection]);

  const groupedAccounts = useMemo(() => {
    if (!groupByTag) return [] as Array<[string, typeof filteredAccounts]>;
    const groups = new Map<string, typeof filteredAccounts>();
    const selectedTags = new Set(tagFilter.map(normalizeTag));
    filteredAccounts.forEach((account) => {
      const tags = (account.tags || []).map(normalizeTag).filter(Boolean);
      const matchedTags = selectedTags.size > 0 ? tags.filter((tag) => selectedTags.has(tag)) : tags;
      if (matchedTags.length === 0) {
        if (!groups.has(untaggedKey)) groups.set(untaggedKey, []);
        groups.get(untaggedKey)?.push(account);
        return;
      }
      matchedTags.forEach((tag) => {
        if (!groups.has(tag)) groups.set(tag, []);
        groups.get(tag)?.push(account);
      });
    });
    return Array.from(groups.entries()).sort(([aKey], [bKey]) => {
      if (aKey === untaggedKey) return 1;
      if (bKey === untaggedKey) return -1;
      return aKey.localeCompare(bKey);
    });
  }, [filteredAccounts, groupByTag, normalizeTag, tagFilter, untaggedKey]);

  const resolveGroupLabel = (groupKey: string) =>
    groupKey === untaggedKey ? t('accounts.defaultGroup', '默认分组') : groupKey;

  const renderUsageInfo = (account: CodebuddyAccount) => {
    const usage = getCodebuddyUsage(account);
    if (!usage.dosageNotifyCode) return <span className="quota-empty">--</span>;
    if (usage.isNormal) return <span className="quota-value high">{t('codebuddy.usageNormal', '正常')}</span>;
    const msg = locale.startsWith('zh') ? (usage.dosageNotifyZh || usage.dosageNotifyCode) : (usage.dosageNotifyEn || usage.dosageNotifyCode);
    return <span className="quota-value critical" title={msg}>{msg}</span>;
  };

  const renderGridCards = (items: typeof filteredAccounts, groupKey?: string) =>
    items.map((account) => {
      const displayEmail = getCodebuddyAccountDisplayEmail(account);
      const planBadge = resolvePlanKey(account);
      const tierBadgeClass = resolveTierBadgeClass(planBadge);
      const accountTags = (account.tags || []).map((tag) => tag.trim()).filter(Boolean);
      const visibleTags = accountTags.slice(0, 2);
      const moreTagCount = Math.max(0, accountTags.length - visibleTags.length);
      const isSelected = selected.has(account.id);
      const isCurrent = currentAccountId === account.id;
      const resourceSummary = getCodebuddyResourceSummary(account);
      const extraCredit = getCodebuddyExtraCreditSummary(account);
      const quotaClass = resolveQuotaClass(resourceSummary?.remainPercent ?? null);
      const remainText = resourceSummary?.remain != null && resourceSummary?.total != null
        ? `${formatQuotaValue(resourceSummary.remain)} / ${formatQuotaValue(resourceSummary.total)}`
        : '--';
      const expireText = resourceSummary?.cycleEndTime
        ? t('codebuddy.quotaQuery.expireAt', '到期时间：{{time}}', { time: resourceSummary.cycleEndTime })
        : t('codebuddy.quotaQuery.expireUnknown', '到期时间：暂无');
      const updatedText = formatQuotaUpdatedTime(resourceSummary?.boundUpdatedAt ?? null);
      const quotaQueryError = (account.quota_query_last_error || '').trim();
      const quotaQueryErrorAtText = formatQuotaUpdatedTime(account.quota_query_last_error_at ?? null);
      return (
        <div key={groupKey ? `${groupKey}-${account.id}` : account.id}
          className={`ghcp-account-card ${isCurrent ? 'current' : ''} ${isSelected ? 'selected' : ''}`}>
          <div className="card-top">
            <div className="card-select">
              <input type="checkbox" checked={isSelected} onChange={() => toggleSelect(account.id)} />
            </div>
            <span className="account-email" title={maskAccountText(displayEmail)}>{maskAccountText(displayEmail)}</span>
            {isCurrent && <span className="current-tag">{t('accounts.status.current', '当前')}</span>}
            <span className={`tier-badge ${tierBadgeClass}`}>{planBadge}</span>
          </div>
          {accountTags.length > 0 && (
            <div className="card-tags">
              {visibleTags.map((tag, idx) => <span key={`${account.id}-${tag}-${idx}`} className="tag-pill">{tag}</span>)}
              {moreTagCount > 0 && <span className="tag-pill more">+{moreTagCount}</span>}
            </div>
          )}
          <div className="ghcp-quota-section">
            <div className="quota-item">
              <div className="quota-header">
                <span className="quota-name">{t('codebuddy.usage', '用量状态')}</span>
                {renderUsageInfo(account)}
              </div>
            </div>
            <div className="quota-item codebuddy-quota-item">
              <div className="quota-header codebuddy-quota-header">
                <span className="quota-name">{t('codebuddy.quotaQuery.sectionTitle', '配额查询')}</span>
                <button
                  type="button"
                  className="quota-query-btn"
                  onClick={() => openQuotaQueryModal(account)}
                >
                  {t('codebuddy.quotaQuery.button', '查询配额')}
                </button>
              </div>
              {resourceSummary ? (
                <>
                  <div className="quota-header">
                    <span className="quota-label">{t('codebuddy.quotaQuery.quotaLabel', '剩余配额')}</span>
                    <span className={`quota-pct ${quotaClass}`}>{remainText}</span>
                  </div>
                  <div className="quota-bar-track">
                    <div
                      className={`quota-bar ${quotaClass}`}
                      style={{ width: `${Math.min(resourceSummary.remainPercent ?? 0, 100)}%` }}
                    />
                  </div>
                  <div className="quota-item codebuddy-extra-credit">
                    <div className="quota-header">
                      <span className="quota-label">{t('codebuddy.extraCredit.title', '加量包')}</span>
                      <span className={`quota-pct ${resolveQuotaClass(extraCredit.remainPercent)}`}>{`${extraCredit.remain} / ${extraCredit.total}`}</span>
                    </div>
                    <div className="quota-bar-track">
                      <div
                        className={`quota-bar ${resolveQuotaClass(extraCredit.remainPercent)}`}
                        style={{ width: `${Math.min(extraCredit.remainPercent ?? 0, 100)}%` }}
                      />
                    </div>
                  </div>
                  <span className="quota-reset">{expireText}</span>
                  {updatedText && (
                    <span className="quota-reset">
                      {t('codebuddy.quotaQuery.updatedAt', '最近查询：{{time}}', { time: updatedText })}
                    </span>
                  )}
                </>
              ) : (
                <>
                  <span className="quota-empty">{t('codebuddy.quotaQuery.empty', '暂无绑定配额查询信息')}</span>
                  {quotaQueryError && (
                    <>
                      <span className="quota-value critical">
                        {t('codebuddy.quotaQuery.failedRefreshCompact', '刷新配额失败')}
                      </span>
                      {quotaQueryErrorAtText && (
                        <span className="quota-reset">
                          {t('codebuddy.quotaQuery.failedReasonAt', '失败时间：{{time}}', { time: quotaQueryErrorAtText })}
                        </span>
                      )}
                    </>
                  )}
                </>
              )}
            </div>
          </div>
          <div className="card-footer">
            <span className="card-date">{formatDate(account.created_at)}</span>
            <div className="card-actions">
              <button className="card-action-btn success" onClick={() => handleInjectToVSCode?.(account.id)} disabled={!!injecting}
                title={t('common.shared.switchAccount', '切换账号')}>
                {injecting === account.id ? <RefreshCw size={14} className="loading-spinner" /> : <Play size={14} />}
              </button>
              <button className="card-action-btn" onClick={() => openTagModal(account.id)} title={t('accounts.editTags', '编辑标签')}><Tag size={14} /></button>
              <button className="card-action-btn" onClick={() => handleRefresh(account.id)} disabled={refreshing === account.id} title={t('common.shared.refreshQuota', '刷新')}>
                <RotateCw size={14} className={refreshing === account.id ? 'loading-spinner' : ''} />
              </button>
              <button className="card-action-btn" onClick={() => openQuotaQueryModal(account)} title={t('codebuddy.quotaQuery.button', '查询配额')}>
                <Database size={14} />
              </button>
              <button className="card-action-btn export-btn" onClick={() => handleExportByIds([account.id])} title={t('common.shared.export', '导出')}><Upload size={14} /></button>
              <button className="card-action-btn danger" onClick={() => handleDelete(account.id)} title={t('common.delete', '删除')}><Trash2 size={14} /></button>
            </div>
          </div>
        </div>
      );
    });

  const renderTableRows = (items: typeof filteredAccounts, _groupKey?: string) =>
    items.map((account) => {
      const displayEmail = getCodebuddyAccountDisplayEmail(account);
      const planBadge = resolvePlanKey(account);
      const tierBadgeClass = resolveTierBadgeClass(planBadge);
      const isSelected = selected.has(account.id);
      const isCurrent = currentAccountId === account.id;
      const resourceSummary = getCodebuddyResourceSummary(account);
      const extraCredit = getCodebuddyExtraCreditSummary(account);
      const quotaQueryError = (account.quota_query_last_error || '').trim();
      const tableQuotaText = resourceSummary?.remain != null && resourceSummary?.total != null
        ? `${formatQuotaValue(resourceSummary.remain)}/${formatQuotaValue(resourceSummary.total)}`
        : (quotaQueryError
          ? t('codebuddy.quotaQuery.failedRefreshCompact', '刷新配额失败')
          : t('codebuddy.quotaQuery.noQuota', '未查询'));
      const tableExtraText = resourceSummary
        ? `${t('codebuddy.extraCredit.title', '加量包')} ${formatQuotaValue(extraCredit.remain)}/${formatQuotaValue(extraCredit.total)}`
        : (quotaQueryError
          ? t('codebuddy.quotaQuery.failedRefreshCompact', '刷新配额失败')
          : '');
      return (
        <tr key={account.id} className={`${isCurrent ? 'current-row' : ''} ${isSelected ? 'selected-row' : ''}`}>
          <td><input type="checkbox" checked={isSelected} onChange={() => toggleSelect(account.id)} /></td>
          <td>
            <span className="table-email" title={maskAccountText(displayEmail)}>{maskAccountText(displayEmail)}</span>
            {isCurrent && <span className="current-tag">{t('accounts.status.current', '当前')}</span>}
          </td>
          <td><span className={`tier-badge ${tierBadgeClass}`}>{planBadge}</span></td>
          <td>
            <div className="codebuddy-table-usage">
              {renderUsageInfo(account)}
              <span className="kiro-table-subline">{tableQuotaText}</span>
              {tableExtraText && <span className="kiro-table-subline extra-credit-subline">{tableExtraText}</span>}
            </div>
          </td>
          <td className="sticky-action-cell table-action-cell">
            <div className="action-buttons">
              <button className="action-btn success" onClick={() => handleInjectToVSCode?.(account.id)} disabled={!!injecting}><Play size={14} /></button>
              <button className="action-btn" onClick={() => openTagModal(account.id)}><Tag size={14} /></button>
              <button className="action-btn" onClick={() => handleRefresh(account.id)} disabled={refreshing === account.id}><RotateCw size={14} className={refreshing === account.id ? 'loading-spinner' : ''} /></button>
              <button className="action-btn" onClick={() => openQuotaQueryModal(account)} title={t('codebuddy.quotaQuery.button', '查询配额')}><Database size={14} /></button>
              <button className="action-btn" onClick={() => handleExportByIds([account.id])}><Upload size={14} /></button>
              <button className="action-btn danger" onClick={() => handleDelete(account.id)}><Trash2 size={14} /></button>
            </div>
          </td>
        </tr>
      );
    });

  return (
    <div className="ghcp-accounts-page codebuddy-accounts-page">
      <PlatformOverviewTabsHeader
        platform="codebuddy"
        active={activeTab}
        onTabChange={setActiveTab}
      />
      {activeTab === 'instances' ? (
        <CodebuddyInstancesContent accountsForSelect={filteredAccounts} />
      ) : (
        <>
      <div className={`ghcp-flow-notice ${isFlowNoticeCollapsed ? 'collapsed' : ''}`} role="note">
        <button type="button" className="ghcp-flow-notice-toggle" onClick={() => setIsFlowNoticeCollapsed((prev) => !prev)}>
          <div className="ghcp-flow-notice-title">
            <CircleAlert size={16} />
            <span>{t('codebuddy.flowNotice.title', 'CodeBuddy 账号管理说明（点击展开/收起）')}</span>
          </div>
          <ChevronDown size={16} className={`ghcp-flow-notice-arrow ${isFlowNoticeCollapsed ? 'collapsed' : ''}`} />
        </button>
        {!isFlowNoticeCollapsed && (
          <div className="ghcp-flow-notice-body">
            <div className="ghcp-flow-notice-desc">
              {t('codebuddy.flowNotice.desc', '切换账号需读取 CodeBuddy 本地认证存储并调用系统凭据服务进行加解密，数据仅在本地处理。')}
            </div>
            <ul className="ghcp-flow-notice-list">
              <li>{t('codebuddy.flowNotice.permission', '权限范围：读取 CodeBuddy 认证数据库 (state.vscdb)，调用系统凭据能力（macOS Keychain / Windows DPAPI / Linux Secret Service）进行解密/回写。')}</li>
              <li>{t('codebuddy.flowNotice.network', '网络范围：OAuth 授权登录与 Token 刷新需联网请求 codebuddy.ai；配额查询需调用计费 API。不上传本地密钥或凭证。')}</li>
            </ul>
          </div>
        )}
      </div>

      {message && (
        <div className={`message-bar ${message.tone === 'error' ? 'error' : 'success'}`}>
          {message.text}
          <button onClick={() => setMessage(null)}><X size={14} /></button>
        </div>
      )}

      <div className="toolbar">
        <div className="toolbar-left">
          <div className="search-box">
            <Search size={16} className="search-icon" />
            <input type="text" placeholder={t('codebuddy.search', '搜索 CodeBuddy 账号...')} value={searchQuery} onChange={(e) => setSearchQuery(e.target.value)} />
          </div>
          <div className="view-switcher">
            <button className={`view-btn ${viewMode === 'list' ? 'active' : ''}`} onClick={() => setViewMode('list')} title={t('common.shared.view.list', '列表视图')}><List size={16} /></button>
            <button className={`view-btn ${viewMode === 'grid' ? 'active' : ''}`} onClick={() => setViewMode('grid')} title={t('common.shared.view.grid', '卡片视图')}><LayoutGrid size={16} /></button>
          </div>
          <div className="filter-select">
            <select value={filterType} onChange={(e) => setFilterType(e.target.value)}>
              <option value="all">{`ALL (${tierSummary.all})`}</option>
              {CB_KNOWN_PLAN_FILTERS.map((plan) => {
                const count = tierSummary.dynamicCounts.get(plan) ?? 0;
                if (count === 0) return null;
                return <option key={plan} value={plan}>{`${plan} (${count})`}</option>;
              })}
              {tierSummary.extraKeys.map((key) => <option key={key} value={key}>{`${key} (${tierSummary.dynamicCounts.get(key) ?? 0})`}</option>)}
            </select>
          </div>
          <div className="tag-filter" ref={tagFilterRef}>
            <button type="button" className={`tag-filter-btn ${tagFilter.length > 0 ? 'active' : ''}`} onClick={() => setShowTagFilter((prev) => !prev)}>
              <Tag size={14} />
              {tagFilter.length > 0 ? `${t('accounts.filterTagsCount', '标签')}(${tagFilter.length})` : t('accounts.filterTags', '标签筛选')}
            </button>
            {showTagFilter && (
              <div className="tag-filter-panel">
                {availableTags.length === 0 ? (
                  <div className="tag-filter-empty">{t('accounts.noAvailableTags', '暂无可用标签')}</div>
                ) : (
                  <>
                    <div className="tag-filter-header">
                      <label className="group-toggle"><input type="checkbox" checked={groupByTag} onChange={() => setGroupByTag(!groupByTag)} /> {t('accounts.groupByTag', '按标签分组')}</label>
                      {tagFilter.length > 0 && <button className="tag-filter-clear" onClick={clearTagFilter}>{t('common.shared.clear', '清除')}</button>}
                    </div>
                    <div className="tag-filter-list">
                      {availableTags.map((tag) => (
                        <label key={tag} className="tag-filter-item">
                          <input type="checkbox" checked={tagFilter.includes(tag)} onChange={() => toggleTagFilterValue(tag)} />
                          <span>{tag}</span>
                        </label>
                      ))}
                    </div>
                  </>
                )}
              </div>
            )}
          </div>
        </div>
        <div className="toolbar-right">
          <button className="btn btn-primary icon-only" onClick={() => openAddModal('oauth')} title={t('common.shared.addAccount', '添加账号')}><Plus size={14} /></button>
          <button className="btn btn-secondary icon-only" onClick={handleRefreshAll} disabled={refreshingAll || accounts.length === 0} title={t('common.shared.refreshAll', '刷新全部')}>
            <RefreshCw size={14} className={refreshingAll ? 'loading-spinner' : ''} />
          </button>
          <button className="btn btn-secondary icon-only" onClick={togglePrivacyMode}
            title={privacyModeEnabled ? t('privacy.showSensitive', '显示邮箱') : t('privacy.hideSensitive', '隐藏邮箱')}>
            {privacyModeEnabled ? <EyeOff size={14} /> : <Eye size={14} />}
          </button>
          <button className="btn btn-secondary icon-only" onClick={() => openAddModal('token')} disabled={importing} title={t('common.shared.import.label', '导入')}><Download size={14} /></button>
          <button className="btn btn-secondary export-btn icon-only" onClick={handleExport} disabled={exporting}
            title={selected.size > 0 ? `${t('common.shared.export', '导出')} (${selected.size})` : t('common.shared.export', '导出')}>
            <Upload size={14} />
          </button>
          {selected.size > 0 && (
            <button className="btn btn-danger icon-only" onClick={handleBatchDelete} title={`${t('common.delete', '删除')} (${selected.size})`}><Trash2 size={14} /></button>
          )}
          <QuickSettingsPopover type="codebuddy" />
        </div>
      </div>

      {loading && accounts.length === 0 ? (
        <div className="loading-container"><RefreshCw size={24} className="loading-spinner" /><p>{t('common.loading', '加载中...')}</p></div>
      ) : accounts.length === 0 ? (
        <div className="empty-state">
          <Globe size={48} />
          <h3>{t('common.shared.empty.title', '暂无账号')}</h3>
          <p>{t('codebuddy.noAccounts', '暂无 CodeBuddy 账号')}</p>
          <div style={{ display: 'flex', gap: '12px', justifyContent: 'center', marginTop: '16px' }}>
            <button className="btn btn-primary" onClick={() => openAddModal('oauth')}>
              <Plus size={16} /> {t('common.shared.addAccount', '添加账号')}
            </button>
          </div>
        </div>
      ) : filteredAccounts.length === 0 ? (
        <div className="empty-state">
          <h3>{t('common.shared.noMatch.title', '没有匹配的账号')}</h3>
          <p>{t('common.shared.noMatch.desc', '请尝试调整搜索或筛选条件')}</p>
        </div>
      ) : viewMode === 'grid' ? (
        groupByTag ? (
          <div className="tag-group-list">
            {groupedAccounts.map(([groupKey, groupAccounts]) => (
              <div key={groupKey} className="tag-group-section">
                <div className="tag-group-header">
                  <span className="tag-group-title">{resolveGroupLabel(groupKey)}</span>
                  <span className="tag-group-count">{groupAccounts.length}</span>
                </div>
                <div className="tag-group-grid ghcp-accounts-grid">{renderGridCards(groupAccounts, groupKey)}</div>
              </div>
            ))}
          </div>
        ) : (
          <div className="ghcp-accounts-grid">{renderGridCards(filteredAccounts)}</div>
        )
      ) : groupByTag ? (
        <div className="account-table-container grouped">
          <table className="account-table">
            <thead>
              <tr>
                <th style={{ width: 40 }}><input type="checkbox" checked={selected.size === filteredAccounts.length && filteredAccounts.length > 0} onChange={() => toggleSelectAll(filteredAccounts.map((a) => a.id))} /></th>
                <th style={{ width: 240 }}>{t('common.shared.columns.email', '邮箱')}</th>
                <th style={{ width: 120 }}>{t('common.shared.columns.plan', '套餐')}</th>
                <th>{t('codebuddy.usage', '用量状态')}</th>
                <th className="sticky-action-header table-action-header">{t('common.shared.columns.actions', '操作')}</th>
              </tr>
            </thead>
            <tbody>
              {groupedAccounts.map(([groupKey, groupAccounts]) => (
                <Fragment key={groupKey}>
                  <tr className="tag-group-row"><td colSpan={5}><div className="tag-group-header"><span className="tag-group-title">{resolveGroupLabel(groupKey)}</span><span className="tag-group-count">{groupAccounts.length}</span></div></td></tr>
                  {renderTableRows(groupAccounts, groupKey)}
                </Fragment>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <div className="account-table-container">
          <table className="account-table">
            <thead>
              <tr>
                <th style={{ width: 40 }}><input type="checkbox" checked={selected.size === filteredAccounts.length && filteredAccounts.length > 0} onChange={() => toggleSelectAll(filteredAccounts.map((a) => a.id))} /></th>
                <th style={{ width: 240 }}>{t('common.shared.columns.email', '邮箱')}</th>
                <th style={{ width: 120 }}>{t('common.shared.columns.plan', '套餐')}</th>
                <th>{t('codebuddy.usage', '用量状态')}</th>
                <th className="sticky-action-header table-action-header">{t('common.shared.columns.actions', '操作')}</th>
              </tr>
            </thead>
            <tbody>{renderTableRows(filteredAccounts)}</tbody>
          </table>
        </div>
      )}

      {showAddModal && (
        <div className="modal-overlay" onClick={closeAddModal}>
            <div className="modal-content ghcp-add-modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2>{t('codebuddy.addAccount', '添加 CodeBuddy 账号')}</h2>
              <button className="modal-close" onClick={closeAddModal}><X size={18} /></button>
            </div>
            <div className="modal-tabs">
              <button className={`modal-tab ${addTab === 'oauth' ? 'active' : ''}`} onClick={() => openAddModal('oauth')}><Globe size={14} /> {t('common.shared.addModal.oauth', '授权登录')}</button>
              <button className={`modal-tab ${addTab === 'token' ? 'active' : ''}`} onClick={() => openAddModal('token')}><KeyRound size={14} />Token / JSON</button>
              <button className={`modal-tab ${addTab === 'json' ? 'active' : ''}`} onClick={() => openAddModal('json')}><Database size={14} />{t('common.shared.addModal.import', '本地导入')}</button>
            </div>
            <div className="modal-body">
              {addTab === 'oauth' && (
                <div className="add-section oauth-section">
                  <p className="section-desc">
                    {t('codebuddy.oauthDesc', '点击下方按钮将在浏览器中打开 CodeBuddy 授权页面。')}
                  </p>
                  <div className="codebuddy-oauth-feature-card oauth">
                    <p className="feature-title">
                      {t('codebuddy.oauthFeature.oauth.title', '仅授权 IDE 登录信息')}
                    </p>
                    <ul className="feature-list">
                      <li>{t('codebuddy.oauthFeature.oauth.item1', '在浏览器完成 OAuth 后即可添加账号并用于 IDE 切换。')}</li>
                      <li>{t('codebuddy.oauthFeature.oauth.item2', '不会自动绑定配额查询参数。')}</li>
                      <li>{t('codebuddy.oauthFeature.oauth.item3', '如需配额显示，请在账号卡片中手动执行“查询配额”绑定。')}</li>
                    </ul>
                  </div>
                  {oauthPrepareError ? (
                    <div className="add-status error">
                      <CircleAlert size={16} />
                      <span>{oauthPrepareError}</span>
                      <button className="btn btn-sm btn-outline" onClick={handleRetryOauth}>
                        {t('common.shared.oauth.retry', '重新生成授权信息')}
                      </button>
                    </div>
                  ) : oauthUrl ? (
                    <div className="oauth-url-section">
                      <div className="oauth-url-box">
                        <input
                          type="text"
                          value={oauthUrl}
                          readOnly
                          placeholder={t('codebuddy.oauthUrlInputPlaceholder', '可手动输入授权地址')}
                        />
                        <button onClick={handleCopyOauthUrl}>
                          {oauthUrlCopied ? <Check size={16} /> : <Copy size={16} />}
                        </button>
                      </div>
                      {!oauthUrl.includes('user_code=') && oauthUserCode && (
                        <div className="oauth-url-box">
                          <input type="text" value={oauthUserCode} readOnly />
                          <button onClick={handleCopyOauthUserCode}>
                            {oauthUserCodeCopied ? <Check size={16} /> : <Copy size={16} />}
                          </button>
                        </div>
                      )}
                      {oauthMeta && (
                        <p className="oauth-hint">
                          {t('common.shared.oauth.meta', '授权有效期：{{expires}}s；轮询间隔：{{interval}}s', {
                            expires: oauthMeta.expiresIn,
                            interval: oauthMeta.intervalSeconds,
                          })}
                        </p>
                      )}
                      <button
                        className="btn btn-primary btn-full"
                        onClick={handleOpenOauthUrl}
                      >
                        <Globe size={16} />
                        {t('common.shared.oauth.openBrowser', '在浏览器中打开')}
                      </button>
                      {oauthPolling && (
                        <div className="add-status loading">
                          <RefreshCw size={16} className="loading-spinner" />
                          <span>{t('codebuddy.oauthWaiting', '等待授权完成...')}</span>
                        </div>
                      )}
                      {oauthCompleteError && (
                        <div className="add-status error">
                          <CircleAlert size={16} />
                          <span>{oauthCompleteError}</span>
                          {oauthTimedOut && (
                            <button className="btn btn-sm btn-outline" onClick={handleRetryOauth}>
                              {t('common.shared.oauth.timeoutRetry', '刷新授权链接')}
                            </button>
                          )}
                        </div>
                      )}
                      <p className="oauth-hint">
                        {t('common.shared.oauth.hint', 'Once authorized, this window will update automatically')}
                      </p>
                    </div>
                  ) : (
                    <div className="oauth-loading">
                      <RefreshCw size={24} className="loading-spinner" />
                      <span>{t('common.shared.oauth.preparing', '正在准备授权信息...')}</span>
                    </div>
                  )}
                </div>
              )}
              {addTab === 'token' && (
                <div className="add-section token-section">
                  <p className="section-desc">{t('codebuddy.tokenDesc', '粘贴 CodeBuddy 的 access token：')}</p>
                  <textarea className="token-input" value={tokenInput} onChange={(e) => setTokenInput(e.target.value)} placeholder={t('common.shared.token.placeholder', '粘贴 Token 或 JSON...')} />
                  <button className="btn btn-primary btn-full" onClick={handleTokenImport} disabled={importing || !tokenInput.trim()}>
                    {importing ? <RefreshCw size={16} className="loading-spinner" /> : <Download size={16} />}
                    {t('common.shared.token.import', 'Import')}
                  </button>
                </div>
              )}
              {addTab === 'json' && (
                <div className="add-section json-section">
                  <p className="section-desc">{t('codebuddy.import.localDesc', '支持从本机 CodeBuddy 客户端或 JSON 文件导入账号数据。')}</p>
                  <button className="btn btn-secondary btn-full" onClick={() => handleImportFromLocal?.()} disabled={importing}>
                    {importing ? <RefreshCw size={16} className="loading-spinner" /> : <Database size={16} />}
                    {t('codebuddy.import.localClient', '从本机 CodeBuddy 导入')}
                  </button>
                  <div className="oauth-hint" style={{ margin: '8px 0 4px' }}>{t('common.shared.import.orJson', '或从 JSON 文件导入')}</div>
                  <input ref={importFileInputRef} type="file" accept="application/json" style={{ display: 'none' }}
                    onChange={(e) => { const file = e.target.files?.[0]; e.target.value = ''; if (!file) return; void handleImportJsonFile(file); }} />
                  <button className="btn btn-primary btn-full" onClick={handlePickImportFile} disabled={importing}>
                    {importing ? <RefreshCw size={16} className="loading-spinner" /> : <Database size={16} />}
                    {t('common.shared.import.pickFile', '选择 JSON 文件导入')}
                  </button>
                </div>
              )}

              {addStatus !== 'idle' && addStatus !== 'loading' && (
                <div className={`add-status ${addStatus}`}>
                  {addStatus === 'success' ? <Check size={16} /> : <CircleAlert size={16} />}
                  <span>{addMessage || t('common.shared.loginSuccess', '登录成功')}</span>
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {quotaQueryAccount && (
        <div className="modal-overlay" onClick={closeQuotaQueryModal}>
          <div className="modal-content ghcp-add-modal codebuddy-quota-query-modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2>{t('codebuddy.quotaQuery.title', '查询配额')}</h2>
              <button className="modal-close" onClick={closeQuotaQueryModal}>
                <X size={18} />
              </button>
            </div>
            <div className="modal-body">
              <div className="add-section">
                <div className="add-panel">
                  <ol className="codebuddy-quota-steps">
                    <li className="codebuddy-quota-step-with-actions">
                      <span>{t('codebuddy.quotaQuery.manual.step1', '在浏览器中打开下方链接并登录')}</span>
                      <div className="codebuddy-quota-url-actions">
                        <code className="codebuddy-quota-url-text">{CODEBUDDY_USAGE_URL}</code>
                        <div className="codebuddy-quota-url-buttons">
                          <button className="btn btn-ghost btn-sm" type="button" onClick={handleCopyQuotaUsageUrl}>
                            <Copy size={14} />
                            {t('common.copy', '复制')}
                          </button>
                          <button className="btn btn-ghost btn-sm" type="button" onClick={handleOpenQuotaUsageUrl}>
                            <Globe size={14} />
                            {t('common.shared.oauth.openBrowser', '在浏览器中打开')}
                          </button>
                        </div>
                      </div>
                    </li>
                    <li>{t('codebuddy.quotaQuery.manual.step2', '按 F12 打开开发者工具，切换到 Network（网络）面板')}</li>
                    <li>{t('codebuddy.quotaQuery.manual.step3', '刷新页面，找到 get-user-resource 请求')}</li>
                    <li>{t('codebuddy.quotaQuery.manual.step4', '右键该请求 → Copy → Copy as cURL')}</li>
                    <li>{t('codebuddy.quotaQuery.manual.step5', '粘贴到下方输入框')}</li>
                  </ol>
                  <div className="codebuddy-quota-form">
                    <textarea
                      className="token-input"
                      value={quotaQueryForm.cookieHeader}
                      onChange={(e) => setQuotaQueryField('cookieHeader', e.target.value)}
                      placeholder={t('codebuddy.quotaQuery.placeholders.cookieHeader', '粘贴 get-user-resource 的 cURL 命令')}
                    />
                  </div>
                </div>
                {quotaQueryAccount && (
                  <p className="oauth-hint">
                    {t('codebuddy.quotaQuery.bindTarget', '绑定账号：{{email}}', {
                      email: maskAccountText(getCodebuddyAccountDisplayEmail(quotaQueryAccount)),
                    })}
                  </p>
                )}
                {quotaQueryModalError && (
                  <div className="add-status error">
                    <CircleAlert size={16} />
                    <span>{quotaQueryModalError}</span>
                  </div>
                )}
                <div className="modal-footer">
                  <button className="btn btn-secondary" onClick={closeQuotaQueryModal} disabled={quotaQuerySubmitting || quotaBindingClearing}>
                    {t('common.cancel', '取消')}
                  </button>
                  <button
                    className="btn btn-secondary"
                    onClick={handleClearQuotaBinding}
                    disabled={!quotaQueryHasBinding || quotaQuerySubmitting || quotaBindingClearing}
                  >
                    {quotaBindingClearing ? (
                      <RefreshCw size={16} className="loading-spinner" />
                    ) : (
                      <Trash2 size={16} />
                    )}
                    {t('codebuddy.quotaQuery.clearBinding', '清除绑定')}
                  </button>
                  <button className="btn btn-primary" onClick={handleSubmitQuotaQuery} disabled={quotaQuerySubmitting || quotaBindingClearing}>
                    {quotaQuerySubmitting ? (
                      <RefreshCw size={16} className="loading-spinner" />
                    ) : (
                      <Database size={16} />
                    )}
                    {t('codebuddy.quotaQuery.submit', '绑定并查询')}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      )}

      {deleteConfirm && (
        <div className="modal-overlay" onClick={() => !deleting && setDeleteConfirm(null)}>
          <div className="modal confirm-modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2>{t('common.confirmDelete', '确认删除')}</h2>
              <button
                className="modal-close"
                onClick={() => !deleting && setDeleteConfirm(null)}
                aria-label={t('common.close', '关闭')}
              >
                <X />
              </button>
            </div>
            <div className="modal-body">
              <p>{deleteConfirm.message}</p>
            </div>
            <div className="modal-footer">
              <button className="btn btn-secondary" onClick={() => setDeleteConfirm(null)} disabled={deleting}>{t('common.cancel', '取消')}</button>
              <button className="btn btn-danger" onClick={confirmDelete} disabled={deleting}>{deleting ? t('common.processing', '处理中...') : t('common.confirm', '确认')}</button>
            </div>
          </div>
        </div>
      )}

      {tagDeleteConfirm && (
        <div className="modal-overlay" onClick={() => !deletingTag && setTagDeleteConfirm(null)}>
          <div className="modal confirm-modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2>{t('common.confirmDeleteTag', '确认删除标签')}</h2>
              <button
                className="modal-close"
                onClick={() => !deletingTag && setTagDeleteConfirm(null)}
                aria-label={t('common.close', '关闭')}
              >
                <X />
              </button>
            </div>
            <div className="modal-body">
              <p>{t('common.deleteTagWarning', { tag: tagDeleteConfirm, defaultValue: '确定要从所有账号中移除标签 "{{tag}}" 吗？' })}</p>
            </div>
            <div className="modal-footer">
              <button className="btn btn-secondary" onClick={() => setTagDeleteConfirm(null)} disabled={deletingTag}>{t('common.cancel', '取消')}</button>
              <button className="btn btn-danger" onClick={confirmDeleteTag} disabled={deletingTag}>{deletingTag ? t('common.processing', '处理中...') : t('common.confirm', '确认')}</button>
            </div>
          </div>
        </div>
      )}

      <ExportJsonModal
        isOpen={showExportModal}
        title={`${t('common.shared.export', '导出')} JSON`}
        jsonContent={exportJsonContent}
        hidden={exportJsonHidden}
        copied={exportJsonCopied}
        saving={savingExportJson}
        savedPath={exportSavedPath}
        canOpenSavedDirectory={canOpenExportSavedDirectory}
        pathCopied={exportPathCopied}
        onClose={closeExportModal}
        onToggleHidden={toggleExportJsonHidden}
        onCopyJson={copyExportJson}
        onSaveJson={saveExportJson}
        onOpenSavedDirectory={openExportSavedDirectory}
        onCopySavedPath={copyExportSavedPath}
      />

      <TagEditModal
        isOpen={!!showTagModal}
        initialTags={accounts.find((a) => a.id === showTagModal)?.tags || []}
        availableTags={availableTags}
        onClose={() => setShowTagModal(null)}
        onSave={handleSaveTags}
      />
        </>
      )}
    </div>
  );
}
