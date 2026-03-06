import { useState, useEffect, useRef, useMemo, useCallback, Fragment } from 'react';
import {
  Plus,
  RefreshCw,
  Download,
  Upload,
  Trash2,
  X,
  Globe,
  KeyRound,
  Database,
  Copy,
  Check,
  Play,
  RotateCw,
  CircleAlert,
  LayoutGrid,
  List,
  Search,
  ArrowDownWideNarrow,
  Clock,
  Calendar,
  Tag,
  Eye,
  EyeOff,
  BookOpen,
} from 'lucide-react';
import { useCodexAccountStore } from '../stores/useCodexAccountStore';
import * as codexService from '../services/codexService';
import { TagEditModal } from '../components/TagEditModal';
import { ExportJsonModal } from '../components/ExportJsonModal';
import { type CodexQuotaErrorInfo } from '../types/codex';
import { buildCodexAccountPresentation } from '../presentation/platformAccountPresentation';

import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { confirm as confirmDialog } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { CodexOverviewTabsHeader, CodexTab } from '../components/CodexOverviewTabsHeader';
import { CodexInstancesContent } from './CodexInstancesPage';
import { QuickSettingsPopover } from '../components/QuickSettingsPopover';
import { useProviderAccountsPage } from '../hooks/useProviderAccountsPage';
import type { CodexAccount } from '../types/codex';

const CODEX_TOKEN_SINGLE_EXAMPLE = `{
  "tokens": {
    "id_token": "eyJ...",
    "access_token": "eyJ...",
    "refresh_token": "rt_..."
  }
}`;
const CODEX_TOKEN_BATCH_EXAMPLE = `[
  {
    "id": "codex_demo_1",
    "email": "user@example.com",
    "tokens": {
      "id_token": "eyJ...",
      "access_token": "eyJ...",
      "refresh_token": "rt_..."
    },
    "created_at": 1730000000,
    "last_used": 1730000000
  }
]`;

export function CodexAccountsPage() {
  const [activeTab, setActiveTab] = useState<CodexTab>('overview');
  const untaggedKey = '__untagged__';

  const store = useCodexAccountStore();

  // Use the common hook WITHOUT oauthService since Codex uses Tauri event-based OAuth
  const page = useProviderAccountsPage<CodexAccount>({
    platformKey: 'Codex',
    oauthLogPrefix: 'CodexOAuth',
    exportFilePrefix: 'codex_accounts',
    store: {
      accounts: store.accounts,
      loading: store.loading,
      fetchAccounts: store.fetchAccounts,
      deleteAccounts: store.deleteAccounts,
      refreshToken: (id) => store.refreshQuota(id).then(() => {}),
      refreshAllTokens: () => store.refreshAllQuotas().then(() => {}),
      updateAccountTags: store.updateAccountTags,
    },
    dataService: {
      importFromJson: codexService.importCodexFromJson,
      exportAccounts: codexService.exportCodexAccounts,
    },
    getDisplayEmail: (account) => account.email ?? account.id,
  });

  const {
    t, maskAccountText, privacyModeEnabled, togglePrivacyMode,
    viewMode, setViewMode, searchQuery, setSearchQuery, filterType, setFilterType,
    sortBy, setSortBy, sortDirection, setSortDirection,
    selected, toggleSelect, toggleSelectAll,
    tagFilter, groupByTag, setGroupByTag, showTagFilter, setShowTagFilter,
    showTagModal, setShowTagModal, tagFilterRef, availableTags,
    toggleTagFilterValue, clearTagFilter, tagDeleteConfirm, setTagDeleteConfirm,
    deletingTag, requestDeleteTag, confirmDeleteTag, openTagModal, handleSaveTags,
    refreshing, refreshingAll,
    handleRefresh, handleRefreshAll, handleDelete, handleBatchDelete,
    deleteConfirm, setDeleteConfirm, deleting, confirmDelete,
    message, setMessage,
    exporting, handleExport, handleExportByIds,
    showExportModal, closeExportModal, exportJsonContent, exportJsonHidden,
    toggleExportJsonHidden, exportJsonCopied, copyExportJson,
    savingExportJson, saveExportJson, exportSavedPath,
    canOpenExportSavedDirectory, openExportSavedDirectory, copyExportSavedPath, exportPathCopied,
    showAddModal, addTab, addStatus, addMessage, tokenInput, setTokenInput,
    importing, openAddModal, closeAddModal,
    formatDate, normalizeTag,
  } = page;

  const { accounts, loading, currentAccount, fetchAccounts, fetchCurrentAccount, switchAccount, refreshQuota } = store;

  // ─── Codex-specific: OAuth via Tauri events ──────────────────────────

  const [oauthUrl, setOauthUrl] = useState<string | null>(null);
  const [oauthUrlCopied, setOauthUrlCopied] = useState(false);
  const [oauthPrepareError, setOauthPrepareError] = useState<string | null>(null);
  const [oauthPortInUse, setOauthPortInUse] = useState<number | null>(null);
  const [oauthTimeoutInfo, setOauthTimeoutInfo] = useState<{ loginId?: string; callbackUrl?: string; timeoutSeconds?: number } | null>(null);
  const [switching, setSwitching] = useState<string | null>(null);

  const showAddModalRef = useRef(showAddModal);
  const addTabRef = useRef(addTab);
  const addStatusRef = useRef(addStatus);
  const oauthActiveRef = useRef(false);
  const oauthLoginIdRef = useRef<string | null>(null);
  const oauthCompletingRef = useRef(false);
  const oauthEventSeqRef = useRef(0);
  const oauthAttemptSeqRef = useRef(0);

  const oauthLog = useCallback((...args: unknown[]) => {
    console.info('[CodexOAuth]', ...args);
  }, []);

  useEffect(() => {
    showAddModalRef.current = showAddModal;
    addTabRef.current = addTab;
    addStatusRef.current = addStatus;
  }, [showAddModal, addTab, addStatus]);

  useEffect(() => {
    fetchAccounts();
    fetchCurrentAccount();
  }, [fetchAccounts, fetchCurrentAccount]);

  // Hook provides setAddStatus/setAddMessage but we need refs to page's versions
  const { setAddStatus, setAddMessage, resetAddModalState, setShowAddModal } = page;

  const handleOauthPrepareError = useCallback((e: unknown) => {
    console.error('[CodexOAuth] 准备授权链接失败', { error: String(e) });
    oauthActiveRef.current = false;
    setOauthTimeoutInfo(null);
    const match = String(e).match(/CODEX_OAUTH_PORT_IN_USE:(\d+)/);
    if (match) {
      const port = Number(match[1]);
      setOauthPortInUse(Number.isNaN(port) ? null : port);
      setOauthPrepareError(t('codex.oauth.portInUse', { port: match[1] }));
      return;
    }
    setOauthPrepareError(t('common.shared.oauth.failed', '授权失败') + ': ' + String(e));
  }, [t]);

  const completeOauthSuccess = useCallback(async () => {
    oauthLog('授权完成并保存成功', { loginId: oauthLoginIdRef.current });
    await fetchAccounts();
    await fetchCurrentAccount();
    setAddStatus('success');
    setAddMessage(t('common.shared.oauth.success', '授权成功'));
    setTimeout(() => {
      setShowAddModal(false);
      resetAddModalState();
    }, 1200);
  }, [fetchAccounts, fetchCurrentAccount, t, oauthLog, setAddStatus, setAddMessage, setShowAddModal, resetAddModalState]);

  const completeOauthError = useCallback((e: unknown) => {
    setAddStatus('error');
    setAddMessage(t('common.shared.oauth.failed', '授权失败') + ': ' + String(e));
  }, [t, setAddStatus, setAddMessage]);

  const isOauthTimeoutState = useMemo(() => !!oauthTimeoutInfo, [oauthTimeoutInfo]);

  useEffect(() => {
    let unlistenExtension: UnlistenFn | undefined;
    let unlistenTimeout: UnlistenFn | undefined;
    let disposed = false;

    listen<{ loginId?: string }>('codex-oauth-login-completed', async (event) => {
      ++oauthEventSeqRef.current;
      if (!showAddModalRef.current || addTabRef.current !== 'oauth' || addStatusRef.current === 'loading' || oauthCompletingRef.current) return;
      const loginId = event.payload?.loginId;
      if (!loginId) return;
      if (oauthLoginIdRef.current && oauthLoginIdRef.current !== loginId) return;
      ++oauthAttemptSeqRef.current;
      setAddStatus('loading');
      setAddMessage(t('codex.oauth.exchanging', '正在交换令牌...'));
      oauthCompletingRef.current = true;
      try {
        await codexService.completeCodexOAuthLogin(loginId);
        await completeOauthSuccess();
      } catch (e) {
        completeOauthError(e);
      } finally {
        oauthCompletingRef.current = false;
      }
    }).then((fn) => { if (disposed) fn(); else unlistenExtension = fn; });

    listen<{ loginId?: string; callbackUrl?: string; timeoutSeconds?: number }>('codex-oauth-login-timeout', async (event) => {
      if (!showAddModalRef.current || addTabRef.current !== 'oauth') return;
      const payload = event.payload ?? {};
      const loginId = payload.loginId;
      if (oauthLoginIdRef.current && loginId && oauthLoginIdRef.current !== loginId) return;
      oauthActiveRef.current = false;
      setOauthUrlCopied(false);
      setOauthPortInUse(null);
      setOauthTimeoutInfo(payload);
      setOauthPrepareError(null);
      setAddStatus('idle');
      setAddMessage('');
    }).then((fn) => { if (disposed) fn(); else unlistenTimeout = fn; });

    return () => { disposed = true; unlistenExtension?.(); unlistenTimeout?.(); };
  }, [completeOauthError, completeOauthSuccess, t, setAddStatus, setAddMessage]);

  const prepareOauthUrl = useCallback(() => {
    if (!showAddModalRef.current || addTabRef.current !== 'oauth') return;
    if (oauthActiveRef.current) return;
    oauthActiveRef.current = true;
    setOauthPrepareError(null);
    setOauthPortInUse(null);
    setOauthTimeoutInfo(null);

    codexService.startCodexOAuthLogin()
      .then(({ loginId, authUrl }) => {
        oauthLoginIdRef.current = loginId ?? null;
        if (typeof authUrl === 'string' && authUrl.length > 0 && showAddModalRef.current && addTabRef.current === 'oauth') {
          setOauthUrl(authUrl);
        } else {
          oauthActiveRef.current = false;
        }
      })
      .catch((e) => handleOauthPrepareError(e));
  }, [handleOauthPrepareError]);

  useEffect(() => {
    if (!showAddModal || addTab !== 'oauth' || oauthUrl || oauthTimeoutInfo) return;
    prepareOauthUrl();
  }, [showAddModal, addTab, oauthUrl, oauthTimeoutInfo, prepareOauthUrl]);

  useEffect(() => {
    if (showAddModal && addTab === 'oauth') return;
    if (!oauthActiveRef.current) return;
    const loginId = oauthLoginIdRef.current ?? undefined;
    codexService.cancelCodexOAuthLogin(loginId).catch(() => {});
    oauthActiveRef.current = false;
    oauthLoginIdRef.current = null;
    setOauthUrl('');
    setOauthUrlCopied(false);
    setOauthTimeoutInfo(null);
  }, [showAddModal, addTab]);

  const handleCopyOauthUrl = async () => {
    if (!oauthUrl) return;
    try { await navigator.clipboard.writeText(oauthUrl); setOauthUrlCopied(true); setTimeout(() => setOauthUrlCopied(false), 1200); } catch {}
  };

  const handleReleaseOauthPort = async () => {
    const port = oauthPortInUse;
    if (!port) return;
    const confirmed = await confirmDialog(t('codex.oauth.portInUseConfirm', { port }), { title: t('codex.oauth.portInUseTitle'), kind: 'warning', okLabel: t('common.confirm'), cancelLabel: t('common.cancel') });
    if (!confirmed) return;
    setOauthPrepareError(null);
    try { await codexService.closeCodexOAuthPort(); } catch (e) { setOauthPrepareError(t('codex.oauth.portCloseFailed', { error: String(e) })); setOauthPortInUse(port); return; }
    prepareOauthUrl();
  };

  const handleRetryOauthAfterTimeout = () => {
    oauthActiveRef.current = false;
    oauthLoginIdRef.current = null;
    setOauthTimeoutInfo(null);
    setOauthPrepareError(null);
    setOauthPortInUse(null);
    setOauthUrl('');
    setOauthUrlCopied(false);
    prepareOauthUrl();
  };

  const handleOpenOauthUrl = async () => {
    if (!oauthUrl) return;
    try { await openUrl(oauthUrl); } catch { await navigator.clipboard.writeText(oauthUrl).catch(() => {}); setOauthUrlCopied(true); setTimeout(() => setOauthUrlCopied(false), 1200); }
  };

  // ─── Codex-specific: Switch / Import ─────────────────────────────────

  const handleSwitch = async (accountId: string) => {
    setMessage(null);
    setSwitching(accountId);
    try {
      const account = await switchAccount(accountId);
      setMessage({ text: t('codex.switched', { email: maskAccountText(account.email) }) });
    } catch (e) {
      setMessage({ text: t('codex.switchFailed', { error: String(e) }), tone: 'error' });
    }
    setSwitching(null);
  };

  const handleImportFromLocal = async () => {
    page.setAddStatus('loading');
    page.setAddMessage(t('codex.import.importing', '正在导入本地账号...'));
    try {
      const account = await codexService.importCodexFromLocal();
      await fetchAccounts();
      try { await refreshQuota(account.id); await fetchAccounts(); } catch {}
      page.setAddStatus('success');
      page.setAddMessage(t('codex.import.successMsg', '导入成功: {{email}}').replace('{{email}}', maskAccountText(account.email)));
      setTimeout(() => { closeAddModal(); }, 1200);
    } catch (e) {
      page.setAddStatus('error');
      page.setAddMessage(t('common.shared.import.failedMsg', '导入失败: {{error}}').replace('{{error}}', String(e).replace(/^Error:\s*/, '')));
    }
  };

  const handleTokenImport = async () => {
    const trimmed = tokenInput.trim();
    if (!trimmed) { page.setAddStatus('error'); page.setAddMessage(t('common.shared.token.empty', '请输入 Token 或 JSON')); return; }
    page.setAddStatus('loading');
    page.setAddMessage(t('common.shared.token.importing', '正在导入...'));
    try {
      const imported = await codexService.importCodexFromJson(trimmed);
      await fetchAccounts();
      for (const acc of imported) { await refreshQuota(acc.id).catch(() => {}); }
      await fetchAccounts();
      page.setAddStatus('success');
      page.setAddMessage(t('common.shared.token.importSuccessMsg', '成功导入 {{count}} 个账号').replace('{{count}}', String(imported.length)));
      setTimeout(() => { closeAddModal(); }, 1200);
    } catch (e) {
      page.setAddStatus('error');
      page.setAddMessage(t('common.shared.token.importFailedMsg', '导入失败: {{error}}').replace('{{error}}', String(e).replace(/^Error:\s*/, '')));
    }
  };

  // ─── Platform-specific: Presentation ─────────────────────────────────

  const resolveQuotaErrorMeta = useCallback((quotaError?: CodexQuotaErrorInfo) => {
    if (!quotaError?.message) return { statusCode: '', errorCode: '', displayText: '', rawMessage: '' };
    const rawMessage = quotaError.message;
    const statusCode = rawMessage.match(/API 返回错误\s+(\d{3})/i)?.[1] || rawMessage.match(/status[=: ]+(\d{3})/i)?.[1] || '';
    const errorCode = quotaError.code || rawMessage.match(/\[error_code:([^\]]+)\]/)?.[1] || '';
    return { statusCode, errorCode, displayText: errorCode || rawMessage, rawMessage };
  }, []);

  const accountPresentations = useMemo(() => {
    const map = new Map<string, ReturnType<typeof buildCodexAccountPresentation>>();
    accounts.forEach((a) => map.set(a.id, buildCodexAccountPresentation(a, t)));
    return map;
  }, [accounts, t]);

  const resolvePresentation = useCallback(
    (account: CodexAccount) => accountPresentations.get(account.id) ?? buildCodexAccountPresentation(account, t),
    [accountPresentations, t],
  );

  const resolveSingleExportBaseName = useCallback(
    (account: CodexAccount) => {
      const display = (resolvePresentation(account).displayName || account.id).trim();
      const atIndex = display.indexOf('@');
      return atIndex > 0 ? display.slice(0, atIndex) : display;
    },
    [resolvePresentation],
  );

  const resolvePlanKey = useCallback(
    (account: CodexAccount) => resolvePresentation(account).planClass.toUpperCase(),
    [resolvePresentation],
  );

  const tierCounts = useMemo(() => {
    const counts = { all: accounts.length, FREE: 0, PLUS: 0, PRO: 0, TEAM: 0, ENTERPRISE: 0 };
    accounts.forEach((a) => { const tier = resolvePlanKey(a); if (tier in counts) counts[tier as keyof typeof counts] += 1; });
    return counts;
  }, [accounts, resolvePlanKey]);

  // ─── Filtering & Sorting ────────────────────────────────────────────
  const compareAccountsBySort = useCallback((a: CodexAccount, b: CodexAccount) => {
    if (sortBy === 'created_at') {
      const diff = b.created_at - a.created_at;
      return sortDirection === 'desc' ? diff : -diff;
    }
    if (sortBy === 'weekly_reset' || sortBy === 'hourly_reset') {
      const aR = sortBy === 'weekly_reset' ? a.quota?.weekly_reset_time ?? null : a.quota?.hourly_reset_time ?? null;
      const bR = sortBy === 'weekly_reset' ? b.quota?.weekly_reset_time ?? null : b.quota?.hourly_reset_time ?? null;
      if (aR == null && bR == null) return 0;
      if (aR == null) return 1;
      if (bR == null) return -1;
      return sortDirection === 'desc' ? bR - aR : aR - bR;
    }
    const aV = sortBy === 'weekly' ? a.quota?.weekly_percentage ?? -1 : a.quota?.hourly_percentage ?? -1;
    const bV = sortBy === 'weekly' ? b.quota?.weekly_percentage ?? -1 : b.quota?.hourly_percentage ?? -1;
    return sortDirection === 'desc' ? bV - aV : aV - bV;
  }, [sortBy, sortDirection]);

  const sortedAccountsForInstances = useMemo(
    () => [...accounts].sort(compareAccountsBySort),
    [accounts, compareAccountsBySort],
  );

  const filteredAccounts = useMemo(() => {
    let result = [...accounts];
    if (searchQuery.trim()) {
      const query = searchQuery.toLowerCase();
      result = result.filter((a) => resolvePresentation(a).displayName.toLowerCase().includes(query));
    }
    if (filterType !== 'all') result = result.filter((a) => resolvePlanKey(a) === filterType);
    if (tagFilter.length > 0) {
      const selectedTags = new Set(tagFilter.map(normalizeTag));
      result = result.filter((a) => (a.tags || []).map(normalizeTag).some((tag) => selectedTags.has(tag)));
    }
    result.sort(compareAccountsBySort);
    return result;
  }, [accounts, compareAccountsBySort, filterType, normalizeTag, resolvePlanKey, resolvePresentation, searchQuery, tagFilter]);

  const groupedAccounts = useMemo(() => {
    if (!groupByTag) return [] as Array<[string, typeof filteredAccounts]>;
    const groups = new Map<string, typeof filteredAccounts>();
    const selectedTags = new Set(tagFilter.map(normalizeTag));
    filteredAccounts.forEach((a) => {
      const tags = (a.tags || []).map(normalizeTag).filter(Boolean);
      const matchedTags = selectedTags.size > 0 ? tags.filter((tag) => selectedTags.has(tag)) : tags;
      if (matchedTags.length === 0) { if (!groups.has(untaggedKey)) groups.set(untaggedKey, []); groups.get(untaggedKey)?.push(a); return; }
      matchedTags.forEach((tag) => { if (!groups.has(tag)) groups.set(tag, []); groups.get(tag)?.push(a); });
    });
    return Array.from(groups.entries()).sort(([a], [b]) => { if (a === untaggedKey) return 1; if (b === untaggedKey) return -1; return a.localeCompare(b); });
  }, [filteredAccounts, groupByTag, normalizeTag, tagFilter, untaggedKey]);

  const quotaColumnLabels = useMemo(() => {
    const source = filteredAccounts.length > 0 ? filteredAccounts : accounts;
    const allItems = source.map((a) => resolvePresentation(a).quotaItems);
    const firstWithWindows = allItems.find((items) => items.length > 0) ?? [];
    const firstWithSecondary = allItems.find((items) => items.length > 1) ?? [];
    return { primary: firstWithWindows[0]?.label ?? '5h', secondary: firstWithSecondary[1]?.label ?? (firstWithWindows.length > 0 ? '—' : 'Weekly') };
  }, [accounts, filteredAccounts, resolvePresentation]);

  const resolveGroupLabel = (groupKey: string) => groupKey === untaggedKey ? t('accounts.defaultGroup', '默认分组') : groupKey;

  // ─── Render helpers ──────────────────────────────────────────────────

  const renderGridCards = (items: typeof filteredAccounts, groupKey?: string) =>
    items.map((account) => {
      const presentation = resolvePresentation(account);
      const isCurrent = currentAccount?.id === account.id;
      const planClass = presentation.planClass || 'unknown';
      const isSelected = selected.has(account.id);
      const quotaItems = presentation.quotaItems;
      const quotaErrorMeta = resolveQuotaErrorMeta(account.quota_error);
      const hasQuotaError = Boolean(quotaErrorMeta.rawMessage);
      const accountTags = (account.tags || []).map((tag) => tag.trim()).filter(Boolean);
      const visibleTags = accountTags.slice(0, 2);
      const moreTagCount = Math.max(0, accountTags.length - visibleTags.length);
      return (
        <div key={groupKey ? `${groupKey}-${account.id}` : account.id} className={`codex-account-card ${isCurrent ? 'current' : ''} ${isSelected ? 'selected' : ''}`}>
          <div className="card-top">
            <div className="card-select"><input type="checkbox" checked={isSelected} onChange={() => toggleSelect(account.id)} /></div>
            <span className="account-email" title={maskAccountText(presentation.displayName)}>{maskAccountText(presentation.displayName)}</span>
            {isCurrent && <span className="current-tag">{t('codex.current', '当前')}</span>}
            {hasQuotaError && (<span className="codex-status-pill quota-error" title={quotaErrorMeta.rawMessage}><CircleAlert size={12} />{quotaErrorMeta.statusCode || t('codex.quotaError.badge', '配额异常')}</span>)}
            <span className={`tier-badge ${planClass}`}>{presentation.planLabel}</span>
          </div>
          {accountTags.length > 0 && (<div className="card-tags">{visibleTags.map((tag, idx) => (<span key={`${account.id}-${tag}-${idx}`} className="tag-pill">{tag}</span>))}{moreTagCount > 0 && <span className="tag-pill more">+{moreTagCount}</span>}</div>)}
          <div className="codex-quota-section">
            {hasQuotaError && (<div className="quota-error-inline" title={quotaErrorMeta.rawMessage}><CircleAlert size={14} /><span>{quotaErrorMeta.displayText}</span></div>)}
            {quotaItems.map((item, index) => {
              const QuotaIcon = index === 1 ? Calendar : Clock;
              return (<div key={item.key} className="quota-item"><div className="quota-header"><QuotaIcon size={14} /><span className="quota-label">{item.label}</span><span className={`quota-pct ${item.quotaClass}`}>{item.valueText}</span></div>
                <div className="quota-bar-track"><div className={`quota-bar ${item.quotaClass}`} style={{ width: `${item.percentage}%` }} /></div>
                {item.resetText && <span className="quota-reset">{item.resetText}</span>}</div>);
            })}
            {quotaItems.length === 0 && (<div className="quota-empty">{t('common.shared.quota.noData', '暂无配额数据')}</div>)}
          </div>
          <div className="card-footer">
            <span className="card-date">{formatDate(account.created_at)}</span>
            <div className="card-actions">
              <button className="card-action-btn" onClick={() => openTagModal(account.id)} title={t('accounts.editTags', '编辑标签')}><Tag size={14} /></button>
              <button className={`card-action-btn ${!isCurrent ? 'success' : ''}`} onClick={() => handleSwitch(account.id)} disabled={!!switching} title={t('codex.switch', '切换')}>
                {switching === account.id ? <RefreshCw size={14} className="loading-spinner" /> : <Play size={14} />}
              </button>
              <button className="card-action-btn" onClick={() => handleRefresh(account.id)} disabled={refreshing === account.id} title={t('common.shared.refreshQuota', '刷新配额')}>
                <RotateCw size={14} className={refreshing === account.id ? 'loading-spinner' : ''} />
              </button>
              <button
                className="card-action-btn export-btn"
                onClick={() => handleExportByIds([account.id], resolveSingleExportBaseName(account))}
                title={t('common.shared.export', '导出')}
              >
                <Upload size={14} />
              </button>
              <button className="card-action-btn danger" onClick={() => handleDelete(account.id)} title={t('common.delete', '删除')}><Trash2 size={14} /></button>
            </div>
          </div>
        </div>
      );
    });

  const renderTableRows = (items: typeof filteredAccounts, groupKey?: string) =>
    items.map((account) => {
      const presentation = resolvePresentation(account);
      const isCurrent = currentAccount?.id === account.id;
      const planClass = presentation.planClass || 'unknown';
      const primaryWindow = presentation.quotaItems[0];
      const secondaryWindow = presentation.quotaItems[1];
      const quotaErrorMeta = resolveQuotaErrorMeta(account.quota_error);
      const hasQuotaError = Boolean(quotaErrorMeta.rawMessage);
      return (
        <tr key={groupKey ? `${groupKey}-${account.id}` : account.id} className={isCurrent ? 'current' : ''}>
          <td><input type="checkbox" checked={selected.has(account.id)} onChange={() => toggleSelect(account.id)} /></td>
          <td><div className="account-cell"><div className="account-main-line"><span className="account-email-text" title={maskAccountText(presentation.displayName)}>{maskAccountText(presentation.displayName)}</span>
            {isCurrent && <span className="mini-tag current">{t('codex.current', '当前')}</span>}</div>
            {hasQuotaError && (<div className="account-sub-line"><span className="codex-status-pill quota-error" title={quotaErrorMeta.rawMessage}><CircleAlert size={12} />{quotaErrorMeta.statusCode || t('codex.quotaError.badge', '配额异常')}</span></div>)}</div></td>
          <td><span className={`tier-badge ${planClass}`}>{presentation.planLabel}</span></td>
          <td>{primaryWindow ? (<div className="quota-item"><div className="quota-header"><span className="quota-name">{primaryWindow.label}</span><span className={`quota-value ${primaryWindow.quotaClass}`}>{primaryWindow.valueText}</span></div>
            <div className="quota-progress-track"><div className={`quota-progress-bar ${primaryWindow.quotaClass}`} style={{ width: `${primaryWindow.percentage}%` }} /></div>
            {primaryWindow.resetText && (<div className="quota-footer"><span className="quota-reset">{primaryWindow.resetText}</span></div>)}</div>) : (<div className="quota-empty">—</div>)}</td>
          <td>{secondaryWindow ? (<div className="quota-item"><div className="quota-header"><span className="quota-name">{secondaryWindow.label}</span><span className={`quota-value ${secondaryWindow.quotaClass}`}>{secondaryWindow.valueText}</span></div>
            <div className="quota-progress-track"><div className={`quota-progress-bar ${secondaryWindow.quotaClass}`} style={{ width: `${secondaryWindow.percentage}%` }} /></div>
            {secondaryWindow.resetText && (<div className="quota-footer"><span className="quota-reset">{secondaryWindow.resetText}</span></div>)}</div>) : (<div className="quota-empty">—</div>)}
            {hasQuotaError && (<div className="quota-error-inline table" title={quotaErrorMeta.rawMessage}><CircleAlert size={12} /><span>{quotaErrorMeta.displayText}</span></div>)}</td>
          <td className="sticky-action-cell table-action-cell"><div className="action-buttons">
            <button className="action-btn" onClick={() => openTagModal(account.id)} title={t('accounts.editTags', '编辑标签')}><Tag size={14} /></button>
            <button className={`action-btn ${!isCurrent ? 'success' : ''}`} onClick={() => handleSwitch(account.id)} disabled={!!switching} title={t('codex.switch', '切换')}>
              {switching === account.id ? <RefreshCw size={14} className="loading-spinner" /> : <Play size={14} />}
            </button>
            <button className="action-btn" onClick={() => handleRefresh(account.id)} disabled={refreshing === account.id} title={t('common.shared.refreshQuota', '刷新配额')}>
              <RotateCw size={14} className={refreshing === account.id ? 'loading-spinner' : ''} />
            </button>
            <button
              className="action-btn"
              onClick={() => handleExportByIds([account.id], resolveSingleExportBaseName(account))}
              title={t('common.shared.export', '导出')}
            >
              <Upload size={14} />
            </button>
            <button className="action-btn danger" onClick={() => handleDelete(account.id)} title={t('common.delete', '删除')}><Trash2 size={14} /></button>
          </div></td>
        </tr>
      );
    });

  return (
    <div className="codex-accounts-page">
      <CodexOverviewTabsHeader active={activeTab} onTabChange={setActiveTab} />

      {activeTab === 'overview' && (<>
      {message && (<div className={`message-bar ${message.tone === 'error' ? 'error' : 'success'}`}>{message.text}<button onClick={() => setMessage(null)}><X size={14} /></button></div>)}

      <div className="toolbar">
        <div className="toolbar-left">
          <div className="search-box"><Search size={16} className="search-icon" /><input type="text" placeholder={t('common.shared.search', '搜索账号...')} value={searchQuery} onChange={(e) => setSearchQuery(e.target.value)} /></div>
          <div className="view-switcher">
            <button className={`view-btn ${viewMode === 'list' ? 'active' : ''}`} onClick={() => setViewMode('list')} title={t('common.shared.view.list', '列表视图')}><List size={16} /></button>
            <button className={`view-btn ${viewMode === 'grid' ? 'active' : ''}`} onClick={() => setViewMode('grid')} title={t('common.shared.view.grid', '卡片视图')}><LayoutGrid size={16} /></button>
          </div>
          <div className="filter-select">
            <select value={filterType} onChange={(e) => setFilterType(e.target.value)} aria-label={t('common.shared.filterLabel', '筛选')}>
              <option value="all">{t('common.shared.filter.all', { count: tierCounts.all })}</option>
              <option value="FREE">{`FREE (${tierCounts.FREE})`}</option>
              <option value="PLUS">{`PLUS (${tierCounts.PLUS})`}</option>
              <option value="PRO">{`PRO (${tierCounts.PRO})`}</option>
              <option value="TEAM">{`TEAM (${tierCounts.TEAM})`}</option>
              <option value="ENTERPRISE">{`ENTERPRISE (${tierCounts.ENTERPRISE})`}</option>
            </select>
          </div>
          <div className="tag-filter" ref={tagFilterRef}>
            <button type="button" className={`tag-filter-btn ${tagFilter.length > 0 ? 'active' : ''}`} onClick={() => setShowTagFilter((prev) => !prev)} aria-label={t('accounts.filterTags', '标签筛选')}>
              <Tag size={14} />{tagFilter.length > 0 ? `${t('accounts.filterTagsCount', '标签')}(${tagFilter.length})` : t('accounts.filterTags', '标签筛选')}
            </button>
            {showTagFilter && (<div className="tag-filter-panel">
              {availableTags.length === 0 ? (<div className="tag-filter-empty">{t('accounts.noAvailableTags', '暂无可用标签')}</div>) : (
                <div className="tag-filter-options">{availableTags.map((tag) => (
                  <label key={tag} className={`tag-filter-option ${tagFilter.includes(tag) ? 'selected' : ''}`}>
                    <input type="checkbox" checked={tagFilter.includes(tag)} onChange={() => toggleTagFilterValue(tag)} /><span className="tag-filter-name">{tag}</span>
                    <button type="button" className="tag-filter-delete" onClick={(e) => { e.preventDefault(); e.stopPropagation(); requestDeleteTag(tag); }} aria-label={t('accounts.deleteTagAria', { tag, defaultValue: '删除标签 {{tag}}' })}><X size={12} /></button>
                  </label>))}</div>)}
              <div className="tag-filter-divider" /><label className="tag-filter-group-toggle"><input type="checkbox" checked={groupByTag} onChange={(e) => setGroupByTag(e.target.checked)} /><span>{t('accounts.groupByTag', '按标签分组展示')}</span></label>
              {tagFilter.length > 0 && (<button type="button" className="tag-filter-clear" onClick={clearTagFilter}>{t('accounts.clearFilter', '清空筛选')}</button>)}
            </div>)}
          </div>
          <div className="sort-select"><ArrowDownWideNarrow size={14} className="sort-icon" />
            <select value={sortBy} onChange={(e) => setSortBy(e.target.value)} aria-label={t('common.shared.sortLabel', '排序')}>
              <option value="created_at">{t('common.shared.sort.createdAt', '按创建时间')}</option>
              <option value="weekly">{t('codex.sort.weekly', '按周配额')}</option>
              <option value="hourly">{t('codex.sort.hourly', '按5小时配额')}</option>
              <option value="weekly_reset">{t('codex.sort.weeklyReset', '按周配额重置时间')}</option>
              <option value="hourly_reset">{t('codex.sort.hourlyReset', '按5小时配额重置时间')}</option>
            </select>
          </div>
          <button className="sort-direction-btn" onClick={() => setSortDirection((prev) => (prev === 'desc' ? 'asc' : 'desc'))}
            title={sortDirection === 'desc' ? t('common.shared.sort.descTooltip', '当前：降序，点击切换为升序') : t('common.shared.sort.ascTooltip', '当前：升序，点击切换为降序')}
            aria-label={t('common.shared.sort.toggleDirection', '切换排序方向')}>{sortDirection === 'desc' ? '⬇' : '⬆'}</button>
        </div>
        <div className="toolbar-right">
          <button className="btn btn-primary icon-only" onClick={() => openAddModal('oauth')} title={t('common.shared.addAccount', '添加账号')}><Plus size={14} /></button>
          <button className="btn btn-secondary icon-only" onClick={handleRefreshAll} disabled={refreshingAll || accounts.length === 0} title={t('common.shared.refreshAll', '刷新全部')}>
            <RefreshCw size={14} className={refreshingAll ? 'loading-spinner' : ''} /></button>
          <button className="btn btn-secondary icon-only" onClick={togglePrivacyMode} title={privacyModeEnabled ? t('privacy.showSensitive', '显示邮箱') : t('privacy.hideSensitive', '隐藏邮箱')}>
            {privacyModeEnabled ? <EyeOff size={14} /> : <Eye size={14} />}</button>
          <button className="btn btn-secondary icon-only" onClick={() => openAddModal('token')} disabled={importing} title={t('common.shared.import.label', '导入')}><Download size={14} /></button>
          <button className="btn btn-secondary export-btn icon-only" onClick={handleExport} disabled={exporting}
            title={selected.size > 0 ? `${t('common.shared.export', '导出')} (${selected.size})` : t('common.shared.export', '导出')}><Upload size={14} /></button>
          {selected.size > 0 && (<button className="btn btn-danger icon-only" onClick={handleBatchDelete} title={`${t('common.delete', '删除')} (${selected.size})`}><Trash2 size={14} /></button>)}
          <QuickSettingsPopover type="codex" />
        </div>
      </div>

      {loading && accounts.length === 0 ? (
        <div className="loading-container"><RefreshCw size={24} className="loading-spinner" /><p>{t('common.loading', '加载中...')}</p></div>
      ) : accounts.length === 0 ? (
        <div className="empty-state"><Globe size={48} /><h3>{t('common.shared.empty.title', '暂无账号')}</h3><p>{t('codex.empty.description', '点击"添加账号"开始管理您的 Codex 账号')}</p>
          <div style={{ display: 'flex', gap: '12px', justifyContent: 'center', marginTop: '16px' }}>
            <button className="btn btn-primary" onClick={() => openAddModal('oauth')}><Plus size={16} />{t('common.shared.addAccount', '添加账号')}</button>
            <button className="btn btn-secondary" onClick={() => window.dispatchEvent(new CustomEvent('app-request-navigate', { detail: 'manual' }))}><BookOpen size={16} />{t('manual.navTitle', '功能使用手册')}</button>
          </div>
        </div>
      ) : filteredAccounts.length === 0 ? (
        <div className="empty-state"><h3>{t('common.shared.noMatch.title', '没有匹配的账号')}</h3><p>{t('common.shared.noMatch.desc', '请尝试调整搜索或筛选条件')}</p></div>
      ) : viewMode === 'grid' ? (
        groupByTag ? (<div className="tag-group-list">{groupedAccounts.map(([gk, ga]) => (<div key={gk} className="tag-group-section"><div className="tag-group-header"><span className="tag-group-title">{resolveGroupLabel(gk)}</span><span className="tag-group-count">{ga.length}</span></div>
          <div className="tag-group-grid codex-accounts-grid">{renderGridCards(ga, gk)}</div></div>))}</div>
        ) : (<div className="codex-accounts-grid">{renderGridCards(filteredAccounts)}</div>)
      ) : groupByTag ? (
        <div className="account-table-container grouped"><table className="account-table"><thead><tr>
          <th style={{ width: 40 }}><input type="checkbox" checked={selected.size === filteredAccounts.length && filteredAccounts.length > 0} onChange={() => toggleSelectAll(filteredAccounts.map((a) => a.id))} /></th>
          <th style={{ width: 260 }}>{t('common.shared.columns.email', '账号')}</th><th style={{ width: 140 }}>{t('common.shared.columns.plan', '订阅')}</th>
          <th>{quotaColumnLabels.primary}</th><th>{quotaColumnLabels.secondary}</th><th className="sticky-action-header table-action-header">{t('common.shared.columns.actions', '操作')}</th></tr></thead>
          <tbody>{groupedAccounts.map(([gk, ga]) => (<Fragment key={gk}><tr className="tag-group-row"><td colSpan={6}><div className="tag-group-header"><span className="tag-group-title">{resolveGroupLabel(gk)}</span><span className="tag-group-count">{ga.length}</span></div></td></tr>
            {renderTableRows(ga, gk)}</Fragment>))}</tbody></table></div>
      ) : (
        <div className="account-table-container"><table className="account-table"><thead><tr>
          <th style={{ width: 40 }}><input type="checkbox" checked={selected.size === filteredAccounts.length && filteredAccounts.length > 0} onChange={() => toggleSelectAll(filteredAccounts.map((a) => a.id))} /></th>
          <th style={{ width: 260 }}>{t('common.shared.columns.email', '账号')}</th><th style={{ width: 140 }}>{t('common.shared.columns.plan', '订阅')}</th>
          <th>{quotaColumnLabels.primary}</th><th>{quotaColumnLabels.secondary}</th><th className="sticky-action-header table-action-header">{t('common.shared.columns.actions', '操作')}</th></tr></thead>
          <tbody>{renderTableRows(filteredAccounts)}</tbody></table></div>
      )}

      {showAddModal && (<div className="modal-overlay" onClick={closeAddModal}><div className="modal-content codex-add-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header"><h2>{t('codex.addModal.title', '添加 Codex 账号')}</h2><button className="modal-close" onClick={closeAddModal} aria-label={t('common.close', '关闭')}><X /></button></div>
        <div className="modal-tabs">
          <button className={`modal-tab ${addTab === 'oauth' ? 'active' : ''}`} onClick={() => openAddModal('oauth')}><Globe size={14} />{t('common.shared.addModal.oauth', 'OAuth Authorization')}</button>
          <button className={`modal-tab ${addTab === 'token' ? 'active' : ''}`} onClick={() => openAddModal('token')}><KeyRound size={14} />Token / JSON</button>
          <button className={`modal-tab ${addTab === 'import' ? 'active' : ''}`} onClick={() => openAddModal('import')}><Database size={14} />{t('accounts.tabs.import', '本地导入')}</button>
        </div>
        <div className="modal-body">
          {addTab === 'oauth' && (<div className="add-section">
            <p className="section-desc">{t('codex.oauth.desc', '通过 OpenAI 官方 OAuth 授权您的 Codex 账号。')}</p>
            {oauthPrepareError ? (<div className="add-status error"><CircleAlert size={16} /><span>{oauthPrepareError}</span>
              {oauthPortInUse && (<button className="btn btn-sm btn-outline" onClick={handleReleaseOauthPort}>{t('codex.oauth.portInUseAction', 'Close port and retry')}</button>)}
              {!oauthPortInUse && oauthTimeoutInfo && (<button className="btn btn-sm btn-outline" onClick={handleRetryOauthAfterTimeout}>{t('codex.oauth.timeoutRetry', '刷新授权链接')}</button>)}</div>
            ) : oauthUrl ? (<div className="oauth-url-section">
              <div className="oauth-url-box"><input type="text" value={oauthUrl} readOnly /><button onClick={handleCopyOauthUrl}>{oauthUrlCopied ? <Check size={16} /> : <Copy size={16} />}</button></div>
              <button className="btn btn-primary btn-full" onClick={isOauthTimeoutState ? handleRetryOauthAfterTimeout : handleOpenOauthUrl}>
                {isOauthTimeoutState ? <RefreshCw size={16} /> : <Globe size={16} />}{isOauthTimeoutState ? t('codex.oauth.timeoutRetry', '刷新授权链接') : t('common.shared.oauth.openBrowser', 'Open in Browser')}</button>
              {isOauthTimeoutState && (<div className="add-status error"><CircleAlert size={16} /><span>{t('codex.oauth.timeout', '授权超时，请点击"刷新授权链接"后重试。')}</span></div>)}
              <p className="oauth-hint">{t('common.shared.oauth.hint', 'Once authorized, this window will update automatically')}</p></div>
            ) : (<div className="oauth-loading"><RefreshCw size={24} className="loading-spinner" /><span>{t('codex.oauth.preparing', '正在准备授权链接...')}</span></div>)}</div>)}
          {addTab === 'token' && (<div className="add-section">
            <p className="section-desc">{t('codex.token.desc', '粘贴您的 Codex Access Token 或导出的 JSON 数据。')}</p>
            <details className="token-format-collapse"><summary className="token-format-collapse-summary">必填字段与示例（点击展开）</summary>
              <div className="token-format"><p className="token-format-required">必填字段：auth.json 需包含 tokens.id_token 与 tokens.access_token；账号数组需包含 id、email、tokens、created_at、last_used</p>
                <div className="token-format-group"><div className="token-format-label">单条示例（auth.json）</div><pre className="token-format-code">{CODEX_TOKEN_SINGLE_EXAMPLE}</pre></div>
                <div className="token-format-group"><div className="token-format-label">批量示例（账号数组）</div><pre className="token-format-code">{CODEX_TOKEN_BATCH_EXAMPLE}</pre></div></div></details>
            <textarea className="token-input" value={tokenInput} onChange={(e) => setTokenInput(e.target.value)} placeholder={t('codex.token.placeholder', '粘贴 Token 或 JSON...')} />
            <button className="btn btn-primary btn-full" onClick={handleTokenImport} disabled={importing || !tokenInput.trim()}>
              {importing ? <RefreshCw size={16} className="loading-spinner" /> : <Download size={16} />}{t('common.shared.token.import', 'Import')}</button></div>)}
          {addTab === 'import' && (<div className="add-section">
            <p className="section-desc">{t('codex.import.localDesc', '从本地已登录的会话中导入 Codex 账号。')}</p>
            <button className="btn btn-primary btn-full" onClick={handleImportFromLocal} disabled={importing}>
              {importing ? <RefreshCw size={16} className="loading-spinner" /> : <Database size={16} />}{t('codex.local.import', 'Get Local Account')}</button></div>)}
          {addStatus !== 'idle' && addStatus !== 'loading' && (<div className={`add-status ${addStatus}`}>{addStatus === 'success' ? <Check size={16} /> : <CircleAlert size={16} />}<span>{addMessage}</span></div>)}
        </div>
      </div></div>)}

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

      {deleteConfirm && (<div className="modal-overlay" onClick={() => !deleting && setDeleteConfirm(null)}><div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header"><h2>{t('common.confirm')}</h2><button className="modal-close" onClick={() => !deleting && setDeleteConfirm(null)} aria-label={t('common.close', '关闭')}><X /></button></div>
        <div className="modal-body"><p>{deleteConfirm.message}</p></div>
        <div className="modal-footer"><button className="btn btn-secondary" onClick={() => setDeleteConfirm(null)} disabled={deleting}>{t('common.cancel')}</button>
          <button className="btn btn-danger" onClick={confirmDelete} disabled={deleting}>{t('common.confirm')}</button></div></div></div>)}

      {tagDeleteConfirm && (<div className="modal-overlay" onClick={() => !deletingTag && setTagDeleteConfirm(null)}><div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header"><h2>{t('common.confirm')}</h2><button className="modal-close" onClick={() => !deletingTag && setTagDeleteConfirm(null)} aria-label={t('common.close', '关闭')}><X /></button></div>
        <div className="modal-body"><p>{t('accounts.confirmDeleteTag', 'Delete tag "{{tag}}"? This tag will be removed from {{count}} accounts.', { tag: tagDeleteConfirm.tag, count: tagDeleteConfirm.count })}</p></div>
        <div className="modal-footer"><button className="btn btn-secondary" onClick={() => setTagDeleteConfirm(null)} disabled={deletingTag}>{t('common.cancel')}</button>
          <button className="btn btn-danger" onClick={confirmDeleteTag} disabled={deletingTag}>{deletingTag ? t('common.processing', '处理中...') : t('common.confirm')}</button></div></div></div>)}

      <TagEditModal isOpen={!!showTagModal} initialTags={accounts.find((a) => a.id === showTagModal)?.tags || []} availableTags={availableTags}
        onClose={() => setShowTagModal(null)} onSave={handleSaveTags} />
      </>)}

      {activeTab === 'instances' && (
        <CodexInstancesContent accountsForSelect={sortedAccountsForInstances} />
      )}
    </div>
  );
}
