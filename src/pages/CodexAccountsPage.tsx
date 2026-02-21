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
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useCodexAccountStore } from '../stores/useCodexAccountStore';
import * as codexService from '../services/codexService';
import { TagEditModal } from '../components/TagEditModal';
import {
  getCodexPlanDisplayName,
  getCodexQuotaClass,
  getCodexQuotaWindows,
  formatCodexResetTime,
  type CodexQuotaErrorInfo,
} from '../types/codex';

import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { confirm as confirmDialog } from '@tauri-apps/plugin-dialog';
import { save } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { invoke } from '@tauri-apps/api/core';
import { CodexOverviewTabsHeader, CodexTab } from '../components/CodexOverviewTabsHeader';
import { CodexInstancesContent } from './CodexInstancesPage';
import { QuickSettingsPopover } from '../components/QuickSettingsPopover';
import {
  isPrivacyModeEnabledByDefault,
  maskSensitiveValue,
  persistPrivacyModeEnabled,
} from '../utils/privacy';

export function CodexAccountsPage() {
  const { t, i18n } = useTranslation();
  const locale = i18n.language || 'zh-CN';
  const untaggedKey = '__untagged__';
  const [activeTab, setActiveTab] = useState<CodexTab>('overview');

  const {
    accounts,
    currentAccount,
    loading,
    fetchAccounts,
    fetchCurrentAccount,
    deleteAccounts,
    refreshQuota,
    refreshAllQuotas,
    switchAccount,
    updateAccountTags,
  } = useCodexAccountStore();

  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [showAddModal, setShowAddModal] = useState(false);
  const [addTab, setAddTab] = useState<'oauth' | 'token' | 'import'>('oauth');
  const [refreshing, setRefreshing] = useState<string | null>(null);
  const [refreshingAll, setRefreshingAll] = useState(false);
  const [viewMode, setViewMode] = useState<'grid' | 'list'>('grid');
  const [privacyModeEnabled, setPrivacyModeEnabled] = useState<boolean>(() =>
    isPrivacyModeEnabledByDefault()
  );
  const [searchQuery, setSearchQuery] = useState('');
  const [filterType, setFilterType] = useState<'all' | 'FREE' | 'PLUS' | 'PRO' | 'TEAM' | 'ENTERPRISE'>('all');
  const [tagFilter, setTagFilter] = useState<string[]>([]);
  const [groupByTag, setGroupByTag] = useState(false);
  const [showTagFilter, setShowTagFilter] = useState(false);
  const [showTagModal, setShowTagModal] = useState<string | null>(null);
  const [sortBy, setSortBy] = useState<'weekly' | 'hourly' | 'created_at' | 'weekly_reset' | 'hourly_reset'>('created_at');
  const [sortDirection, setSortDirection] = useState<'asc' | 'desc'>('desc');
  const [exporting, setExporting] = useState(false);
  const [addMessage, setAddMessage] = useState<string | null>(null);
  const [addStatus, setAddStatus] = useState<'idle' | 'loading' | 'success' | 'error'>('idle');
  const [oauthUrl, setOauthUrl] = useState<string | null>(null);
  const [oauthUrlCopied, setOauthUrlCopied] = useState(false);
  const [oauthPrepareError, setOauthPrepareError] = useState<string | null>(null);
  const [oauthPortInUse, setOauthPortInUse] = useState<number | null>(null);
  const [oauthTimeoutInfo, setOauthTimeoutInfo] = useState<{ loginId?: string; callbackUrl?: string; timeoutSeconds?: number } | null>(null);
  const [tokenInput, setTokenInput] = useState('');
  const [importing, setImporting] = useState(false);
  const [switching, setSwitching] = useState<string | null>(null);
  const [message, setMessage] = useState<{ text: string; tone?: 'error' } | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<{ ids: string[]; message: string } | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [tagDeleteConfirm, setTagDeleteConfirm] = useState<{ tag: string; count: number } | null>(null);
  const [deletingTag, setDeletingTag] = useState(false);

  const showAddModalRef = useRef(showAddModal);
  const addTabRef = useRef(addTab);
  const addStatusRef = useRef(addStatus);
  const oauthActiveRef = useRef(false);
  const oauthLoginIdRef = useRef<string | null>(null);
  const oauthCompletingRef = useRef(false);
  const oauthEventSeqRef = useRef(0);
  const oauthAttemptSeqRef = useRef(0);
  const tagFilterRef = useRef<HTMLDivElement | null>(null);
  const oauthLog = useCallback((...args: unknown[]) => {
    console.info('[CodexOAuth]', ...args);
  }, []);
  const togglePrivacyMode = useCallback(() => {
    setPrivacyModeEnabled((prev) => {
      const next = !prev;
      persistPrivacyModeEnabled(next);
      return next;
    });
  }, []);
  const maskAccountText = useCallback(
    (value?: string | null) => maskSensitiveValue(value, privacyModeEnabled),
    [privacyModeEnabled]
  );

  useEffect(() => {
    showAddModalRef.current = showAddModal;
    addTabRef.current = addTab;
    addStatusRef.current = addStatus;
  }, [showAddModal, addTab, addStatus]);

  useEffect(() => {
    if (!showTagFilter) return;
    const handleClick = (event: MouseEvent) => {
      if (!tagFilterRef.current) return;
      if (!tagFilterRef.current.contains(event.target as Node)) {
        setShowTagFilter(false);
      }
    };
    document.addEventListener('mousedown', handleClick);
    return () => document.removeEventListener('mousedown', handleClick);
  }, [showTagFilter]);

  useEffect(() => {
    fetchAccounts();
    fetchCurrentAccount();
  }, [fetchAccounts, fetchCurrentAccount]);

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
    console.error('准备 Codex OAuth 链接失败:', e);
  }, [t]);

  const completeOauthSuccess = useCallback(async () => {
    oauthLog('授权完成并保存成功', {
      loginId: oauthLoginIdRef.current,
    });
    await fetchAccounts();
    await fetchCurrentAccount();
    setAddStatus('success');
    setAddMessage(t('common.shared.oauth.success', '授权成功'));
    setTimeout(() => {
      setShowAddModal(false);
      resetAddModalState();
    }, 1200);
  }, [fetchAccounts, fetchCurrentAccount, t]);

  const completeOauthError = useCallback((e: unknown) => {
    console.error('[CodexOAuth] 授权完成失败', {
      loginId: oauthLoginIdRef.current,
      error: String(e),
    });
    setAddStatus('error');
    setAddMessage(t('common.shared.oauth.failed', '授权失败') + ': ' + String(e));
  }, [t]);

  const getCallbackUrlFromAuthUrl = useCallback((authUrl: string): string | null => {
    try {
      return new URL(authUrl).searchParams.get('redirect_uri');
    } catch {
      return null;
    }
  }, []);

  const isOauthTimeoutState = useMemo(() => !!oauthTimeoutInfo, [oauthTimeoutInfo]);

  useEffect(() => {
    let unlistenExtension: UnlistenFn | undefined;
    let unlistenTimeout: UnlistenFn | undefined;
    let disposed = false;
    oauthLog('OAuth 事件监听 effect 挂载');

    listen<{ loginId?: string }>('codex-oauth-login-completed', async (event) => {
      const eventSeq = ++oauthEventSeqRef.current;
      oauthLog('收到 OAuth 回调事件', {
        eventSeq,
        payload: event.payload,
        showAddModal: showAddModalRef.current,
        addTab: addTabRef.current,
        addStatus: addStatusRef.current,
        completing: oauthCompletingRef.current,
        expectedLoginId: oauthLoginIdRef.current,
      });
      if (!showAddModalRef.current) {
        oauthLog('OAuth 回调事件被忽略：弹框已关闭', { eventSeq });
        return;
      }
      if (addTabRef.current !== 'oauth') {
        oauthLog('OAuth 回调事件被忽略：当前不在 OAuth Tab', { eventSeq, addTab: addTabRef.current });
        return;
      }
      if (addStatusRef.current === 'loading') {
        oauthLog('OAuth 回调事件被忽略：UI 已是 loading', { eventSeq });
        return;
      }
      if (oauthCompletingRef.current) {
        oauthLog('OAuth 回调事件被忽略：complete 正在进行中', { eventSeq });
        return;
      }

      const loginId = event.payload?.loginId;
      if (!loginId) {
        oauthLog('OAuth 回调事件被忽略：payload 没有 loginId', { eventSeq });
        return;
      }
      if (oauthLoginIdRef.current && oauthLoginIdRef.current !== loginId) {
        console.warn('[CodexOAuth] 收到非当前登录会话的完成事件，已忽略', {
          eventSeq,
          expectedLoginId: oauthLoginIdRef.current,
          receivedLoginId: loginId,
          payload: event.payload,
        });
        return;
      }
      const attemptId = ++oauthAttemptSeqRef.current;
      const startedAt = Date.now();
      oauthLog('收到 OAuth 回调事件，开始完成登录', {
        eventSeq,
        attemptId,
        loginId,
        payload: event.payload,
      });

      setAddStatus('loading');
      setAddMessage(t('codex.oauth.exchanging', '正在交换令牌...'));
      oauthCompletingRef.current = true;

      try {
        const account = await codexService.completeCodexOAuthLogin(loginId);
        oauthLog('completeCodexOAuthLogin 成功', {
          eventSeq,
          attemptId,
          durationMs: Date.now() - startedAt,
          account,
        });
        await completeOauthSuccess();
      } catch (e) {
        oauthLog('completeCodexOAuthLogin 失败', {
          eventSeq,
          attemptId,
          durationMs: Date.now() - startedAt,
          error: String(e),
        });
        completeOauthError(e);
      } finally {
        oauthCompletingRef.current = false;
        oauthLog('OAuth complete 收尾', { eventSeq, attemptId });
      }
    }).then((fn) => {
      if (disposed) {
        fn();
      } else {
        unlistenExtension = fn;
        oauthLog('已注册监听: codex-oauth-login-completed');
      }
    });

    listen<{ loginId?: string; callbackUrl?: string; timeoutSeconds?: number }>('codex-oauth-login-timeout', async (event) => {
      if (!showAddModalRef.current) {
        oauthLog('收到超时事件，但弹框已关闭，忽略', event.payload);
        return;
      }
      if (addTabRef.current !== 'oauth') return;

      const payload = event.payload ?? {};
      const loginId = payload.loginId;
      if (oauthLoginIdRef.current && loginId && oauthLoginIdRef.current !== loginId) {
        console.warn('[CodexOAuth] 收到非当前登录会话的超时事件，已忽略', {
          expectedLoginId: oauthLoginIdRef.current,
          receivedLoginId: loginId,
          payload,
        });
        return;
      }

      oauthActiveRef.current = false;
      setOauthUrlCopied(false);
      setOauthPortInUse(null);
      setOauthTimeoutInfo(payload);
      // 超时场景保持 oauthUrl 区域可见，通过主按钮切换为“刷新授权链接”来重试
      setOauthPrepareError(null);
      setAddStatus('idle');
      setAddMessage('');
      oauthLog('收到授权超时事件，已展示重试入口', payload);
    }).then((fn) => {
      if (disposed) {
        fn();
      } else {
        unlistenTimeout = fn;
        oauthLog('已注册监听: codex-oauth-login-timeout');
      }
    });

    return () => {
      disposed = true;
      oauthLog('OAuth 事件监听 effect 卸载，准备取消监听');
      if (unlistenExtension) unlistenExtension();
      if (unlistenTimeout) unlistenTimeout();
    };
  }, [completeOauthError, completeOauthSuccess, t, oauthLog]);

  const prepareOauthUrl = useCallback(() => {
    if (!showAddModalRef.current || addTabRef.current !== 'oauth') return;
    if (oauthActiveRef.current) return;
    oauthActiveRef.current = true;
    setOauthPrepareError(null);
    setOauthPortInUse(null);
    setOauthTimeoutInfo(null);
    oauthLog('开始准备授权链接');

    const openPreparedUrl = (url: string) => {
      if (typeof url === 'string' && url.length > 0 && showAddModalRef.current && addTabRef.current === 'oauth') {
        setOauthUrl(url);
        oauthLog('授权链接已就绪并展示在弹框', {
          loginId: oauthLoginIdRef.current,
          authUrl: url,
          callbackUrl: getCallbackUrlFromAuthUrl(url),
        });
        return true;
      }
      console.warn('[CodexOAuth] 授权链接返回后界面状态已变化，放弃展示');
      oauthActiveRef.current = false;
      return false;
    };

    codexService.startCodexOAuthLogin()
      .then(({ loginId, authUrl }) => {
        oauthLoginIdRef.current = loginId ?? null;
        oauthLog('OAuth start 成功', {
          loginId: oauthLoginIdRef.current,
          authUrl,
          callbackUrl: getCallbackUrlFromAuthUrl(authUrl),
        });
        const opened = openPreparedUrl(authUrl);
        if (!opened) {
          oauthLoginIdRef.current = null;
        }
      })
      .catch((e) => {
        handleOauthPrepareError(e);
      });
  }, [getCallbackUrlFromAuthUrl, handleOauthPrepareError]);

  useEffect(() => {
    if (!showAddModal || addTab !== 'oauth' || oauthUrl || oauthTimeoutInfo) return;
    prepareOauthUrl();
  }, [showAddModal, addTab, oauthUrl, oauthTimeoutInfo, prepareOauthUrl]);

  useEffect(() => {
    if (showAddModal && addTab === 'oauth') return;
    if (!oauthActiveRef.current) return;
    const loginId = oauthLoginIdRef.current ?? undefined;
    oauthLog('弹框关闭或切换标签，准备取消授权流程', { loginId });
    codexService.cancelCodexOAuthLogin(loginId).catch(() => {});
    oauthActiveRef.current = false;
    oauthLoginIdRef.current = null;
    setOauthUrl('');
    setOauthUrlCopied(false);
    setOauthTimeoutInfo(null);
  }, [showAddModal, addTab]);

  const resolveQuotaErrorMeta = useCallback((quotaError?: CodexQuotaErrorInfo) => {
    if (!quotaError?.message) {
      return {
        statusCode: '',
        errorCode: '',
        displayText: '',
        rawMessage: '',
      };
    }

    const rawMessage = quotaError.message;
    const statusCode =
      rawMessage.match(/API 返回错误\s+(\d{3})/i)?.[1] ||
      rawMessage.match(/status[=: ]+(\d{3})/i)?.[1] ||
      '';
    const errorCode =
      quotaError.code ||
      rawMessage.match(/\[error_code:([^\]]+)\]/)?.[1] ||
      '';

    return {
      statusCode,
      errorCode,
      displayText: errorCode || rawMessage,
      rawMessage,
    };
  }, []);

  const handleRefresh = async (accountId: string) => {
    setRefreshing(accountId);
    try {
      await refreshQuota(accountId);
    } catch (e) {
      console.error(e);
      await fetchAccounts();
    }
    setRefreshing(null);
  };

  const handleRefreshAll = async () => {
    setRefreshingAll(true);
    try {
      await refreshAllQuotas();
    } catch (e) {
      console.error(e);
    }
    setRefreshingAll(false);
  };

  const handleDelete = (accountId: string) => {
    setDeleteConfirm({
      ids: [accountId],
      message: t('messages.deleteConfirm', '确定要删除此账号吗？'),
    });
  };

  const handleBatchDelete = () => {
    if (selected.size === 0) return;
    setDeleteConfirm({
      ids: Array.from(selected),
      message: t('messages.batchDeleteConfirm', { count: selected.size }),
    });
  };

  const confirmDelete = async () => {
    if (!deleteConfirm || deleting) return;
    setDeleting(true);
    try {
      await deleteAccounts(deleteConfirm.ids);
      setSelected((prev) => {
        const next = new Set(prev);
        deleteConfirm.ids.forEach((id) => next.delete(id));
        return next;
      });
      setDeleteConfirm(null);
    } finally {
      setDeleting(false);
    }
  };

  const resetAddModalState = () => {
    setAddStatus('idle');
    setAddMessage('');
    setTokenInput('');
    setOauthUrl('');
    setOauthUrlCopied(false);
    setOauthPrepareError(null);
    setOauthPortInUse(null);
    setOauthTimeoutInfo(null);
    oauthLoginIdRef.current = null;
    oauthCompletingRef.current = false;
  };

  const openAddModal = (tab: 'oauth' | 'token' | 'import') => {
    setAddTab(tab);
    setShowAddModal(true);
    resetAddModalState();
  };

  const closeAddModal = () => {
    setShowAddModal(false);
    resetAddModalState();
  };

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
    setImporting(true);
    setAddStatus('loading');
    setAddMessage(t('codex.import.importing', '正在导入本地账号...'));
    try {
      const account = await codexService.importCodexFromLocal();
      await fetchAccounts();
      
      try {
        await refreshQuota(account.id);
        await fetchAccounts();
      } catch (quotaErr) {
        console.warn('配额刷新失败（可稍后重试）:', quotaErr);
      }
      
      setAddStatus('success');
      setAddMessage(
        t('codex.import.successMsg', '导入成功: {{email}}').replace(
          '{{email}}',
          maskAccountText(account.email)
        )
      );
      setTimeout(() => {
        setShowAddModal(false);
        resetAddModalState();
      }, 1200);
    } catch (e) {
      setAddStatus('error');
      const errorMsg = String(e).replace(/^Error:\s*/, '');
      setAddMessage(t('common.shared.import.failedMsg', '导入失败: {{error}}').replace('{{error}}', errorMsg));
    }
    setImporting(false);
  };

  const handleTokenImport = async () => {
    const trimmed = tokenInput.trim();
    if (!trimmed) {
      setAddStatus('error');
      setAddMessage(t('common.shared.token.empty', '请输入 Token 或 JSON'));
      return;
    }

    setImporting(true);
    setAddStatus('loading');
    setAddMessage(t('common.shared.token.importing', '正在导入...'));

    try {
      const accounts = await codexService.importCodexFromJson(trimmed);
      await fetchAccounts();
      for (const acc of accounts) {
        await refreshQuota(acc.id).catch(() => {});
      }
      await fetchAccounts();
      setAddStatus('success');
      setAddMessage(t('common.shared.token.importSuccessMsg', '成功导入 {{count}} 个账号').replace('{{count}}', String(accounts.length)));
      setTimeout(() => {
        setShowAddModal(false);
        resetAddModalState();
      }, 1200);
    } catch (e) {
      setAddStatus('error');
      const errorMsg = String(e).replace(/^Error:\s*/, '');
      setAddMessage(t('common.shared.token.importFailedMsg', '导入失败: {{error}}').replace('{{error}}', errorMsg));
    }
    setImporting(false);
  };

  const handleCopyOauthUrl = async () => {
    if (!oauthUrl) return;
    try {
      await navigator.clipboard.writeText(oauthUrl);
      oauthLog('已复制授权链接', {
        loginId: oauthLoginIdRef.current,
        authUrl: oauthUrl,
      });
      setOauthUrlCopied(true);
      window.setTimeout(() => setOauthUrlCopied(false), 1200);
    } catch (e) {
      console.error('复制失败:', e);
    }
  };

  const handleReleaseOauthPort = async () => {
    const port = oauthPortInUse;
    if (!port) return;
    const confirmed = await confirmDialog(
      t('codex.oauth.portInUseConfirm', { port }),
      {
        title: t('codex.oauth.portInUseTitle'),
        kind: 'warning',
        okLabel: t('common.confirm'),
        cancelLabel: t('common.cancel'),
      }
    );
    if (!confirmed) return;

    setOauthPrepareError(null);
    try {
      await codexService.closeCodexOAuthPort();
    } catch (e) {
      setOauthPrepareError(t('codex.oauth.portCloseFailed', { error: String(e) }));
      setOauthPortInUse(port);
      return;
    }

    prepareOauthUrl();
  };

  const handleRetryOauthAfterTimeout = () => {
    oauthLog('用户点击刷新授权链接', {
      lastTimeout: oauthTimeoutInfo,
    });
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
    oauthLog('用户点击在浏览器打开授权链接', {
      loginId: oauthLoginIdRef.current,
      authUrl: oauthUrl,
      callbackUrl: getCallbackUrlFromAuthUrl(oauthUrl),
    });
    try {
      await openUrl(oauthUrl);
    } catch (e) {
      console.error('打开浏览器失败:', e);
      await navigator.clipboard.writeText(oauthUrl).catch(() => {});
      setOauthUrlCopied(true);
      setTimeout(() => setOauthUrlCopied(false), 1200);
    }
  };

  const resolveDefaultExportPath = async (fileName: string) => {
    const userAgent = navigator.userAgent.toLowerCase();
    if (!userAgent.includes('mac')) return fileName;
    try {
      const dir = await invoke<string>('get_downloads_dir');
      if (!dir) return fileName;
      const normalized = dir.endsWith('/') ? dir.slice(0, -1) : dir;
      return `${normalized}/${fileName}`;
    } catch (e) {
      console.error('获取下载目录失败:', e);
      return fileName;
    }
  };

  const saveJsonFile = async (json: string, defaultFileName: string) => {
    const defaultPath = await resolveDefaultExportPath(defaultFileName);
    const filePath = await save({
      defaultPath,
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (!filePath) return null;
    await invoke('save_text_file', { path: filePath, content: json });
    return filePath;
  };

  const handleExport = async () => {
    setExporting(true);
    try {
      const ids = selected.size > 0 ? Array.from(selected) : accounts.map((a) => a.id);
      const json = await codexService.exportCodexAccounts(ids);
      const defaultName = `codex_accounts_${new Date().toISOString().slice(0, 10)}.json`;
      const savedPath = await saveJsonFile(json, defaultName);
      if (savedPath) {
        setMessage({ text: `${t('common.success')}: ${savedPath}` });
      }
    } catch (e) {
      setMessage({ text: t('messages.exportFailed', { error: String(e) }), tone: 'error' });
    }
    setExporting(false);
  };

  const formatDate = (timestamp: number) => {
    const d = new Date(timestamp * 1000);
    return (
      d.toLocaleDateString(locale, { year: 'numeric', month: '2-digit', day: '2-digit' }) +
      ' ' +
      d.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' })
    );
  };

  const toggleSelect = (id: string) => {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setSelected(next);
  };

  const toggleSelectAll = () => {
    const allIds = filteredAccounts.map((account) => account.id);
    const allSelected = selected.size === allIds.length && allIds.length > 0;
    setSelected(allSelected ? new Set() : new Set(allIds));
  };

  const normalizePlan = (planType?: string) => getCodexPlanDisplayName(planType);

  const normalizeTag = (tag: string) => tag.trim().toLowerCase();

  const availableTags = useMemo(() => {
    const set = new Set<string>();
    accounts.forEach((account) => {
      (account.tags || []).forEach((tag) => {
        const normalized = normalizeTag(tag);
        if (normalized) set.add(normalized);
      });
    });
    return Array.from(set).sort((a, b) => a.localeCompare(b));
  }, [accounts]);

  const tierCounts = useMemo(() => {
    const counts = {
      all: accounts.length,
      FREE: 0,
      PLUS: 0,
      PRO: 0,
      TEAM: 0,
      ENTERPRISE: 0,
    };
    accounts.forEach((account) => {
      const tier = normalizePlan(account.plan_type);
      if (tier in counts) {
        counts[tier as keyof typeof counts] += 1;
      }
    });
    return counts;
  }, [accounts]);

  const filteredAccounts = useMemo(() => {
    let result = [...accounts];

    if (searchQuery.trim()) {
      const query = searchQuery.toLowerCase();
      result = result.filter((account) => account.email.toLowerCase().includes(query));
    }

    if (filterType !== 'all') {
      result = result.filter((account) => normalizePlan(account.plan_type) === filterType);
    }

    if (tagFilter.length > 0) {
      const selectedTags = new Set(tagFilter.map(normalizeTag));
      result = result.filter((acc) => {
        const tags = (acc.tags || []).map(normalizeTag);
        return tags.some((tag) => selectedTags.has(tag));
      });
    }

    result.sort((a, b) => {
      if (sortBy === 'created_at') {
        const diff = b.created_at - a.created_at;
        return sortDirection === 'desc' ? diff : -diff;
      }

      if (sortBy === 'weekly_reset' || sortBy === 'hourly_reset') {
        const aReset =
          sortBy === 'weekly_reset'
            ? a.quota?.weekly_reset_time ?? null
            : a.quota?.hourly_reset_time ?? null;
        const bReset =
          sortBy === 'weekly_reset'
            ? b.quota?.weekly_reset_time ?? null
            : b.quota?.hourly_reset_time ?? null;
        if (aReset === null && bReset === null) return 0;
        if (aReset === null) return 1;
        if (bReset === null) return -1;
        const diff = bReset - aReset;
        return sortDirection === 'desc' ? diff : -diff;
      }

      const aValue = sortBy === 'weekly' ? a.quota?.weekly_percentage ?? -1 : a.quota?.hourly_percentage ?? -1;
      const bValue = sortBy === 'weekly' ? b.quota?.weekly_percentage ?? -1 : b.quota?.hourly_percentage ?? -1;
      const diff = bValue - aValue;
      return sortDirection === 'desc' ? diff : -diff;
    });

    return result;
  }, [accounts, filterType, searchQuery, sortBy, sortDirection, tagFilter]);

  const groupedAccounts = useMemo(() => {
    if (!groupByTag) return [] as Array<[string, typeof filteredAccounts]>;
    const groups = new Map<string, typeof filteredAccounts>();
    const selectedTags = new Set(tagFilter.map(normalizeTag));

    filteredAccounts.forEach((account) => {
      const tags = (account.tags || []).map(normalizeTag).filter(Boolean);
      const matchedTags = selectedTags.size > 0
        ? tags.filter((tag) => selectedTags.has(tag))
        : tags;

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
  }, [filteredAccounts, groupByTag, tagFilter, untaggedKey]);

  const quotaColumnLabels = useMemo(() => {
    const source = filteredAccounts.length > 0 ? filteredAccounts : accounts;
    const allWindows = source.map((account) => getCodexQuotaWindows(account.quota));
    const firstWithWindows = allWindows.find((windows) => windows.length > 0) ?? [];
    const firstWithSecondary = allWindows.find((windows) => windows.length > 1) ?? [];

    return {
      primary: firstWithWindows[0]?.label ?? '5h',
      secondary: firstWithSecondary[1]?.label ?? (firstWithWindows.length > 0 ? '—' : 'Weekly'),
    };
  }, [accounts, filteredAccounts]);

  const toggleTagFilterValue = (tag: string) => {
    setTagFilter((prev) => {
      if (prev.includes(tag)) return prev.filter((item) => item !== tag);
      return [...prev, tag];
    });
  };

  const clearTagFilter = () => {
    setTagFilter([]);
  };

  const requestDeleteTag = (tag: string) => {
    const normalized = normalizeTag(tag);
    if (!normalized) return;
    const count = accounts.filter((account) =>
      (account.tags || []).some((item) => normalizeTag(item) === normalized)
    ).length;
    setTagDeleteConfirm({ tag: normalized, count });
  };

  const confirmDeleteTag = async () => {
    if (!tagDeleteConfirm || deletingTag) return;
    setDeletingTag(true);
    const target = tagDeleteConfirm.tag;
    const affected = accounts.filter((account) =>
      (account.tags || []).some((item) => normalizeTag(item) === target)
    );

    try {
      await Promise.allSettled(
        affected.map((account) => {
          const nextTags = (account.tags || []).filter(
            (item) => normalizeTag(item) !== target
          );
          return codexService.updateCodexAccountTags(account.id, nextTags);
        })
      );
      setTagFilter((prev) => prev.filter((item) => normalizeTag(item) !== target));
      await fetchAccounts();
    } finally {
      setDeletingTag(false);
      setTagDeleteConfirm(null);
      setShowTagFilter(false);
    }
  };

  const openTagModal = (accountId: string) => {
    setShowTagModal(accountId);
  };

  const handleSaveTags = async (tags: string[]) => {
    if (!showTagModal) return;
    await updateAccountTags(showTagModal, tags);
    setShowTagModal(null);
  };

  const resolveGroupLabel = (groupKey: string) =>
    groupKey === untaggedKey ? t('accounts.defaultGroup', '默认分组') : groupKey;

  const renderGridCards = (items: typeof filteredAccounts, groupKey?: string) =>
    items.map((account) => {
      const isCurrent = currentAccount?.id === account.id;
      const planKey = getCodexPlanDisplayName(account.plan_type);
      const planLabel = planKey;
      const isSelected = selected.has(account.id);
      const quotaWindows = getCodexQuotaWindows(account.quota);
      const quotaErrorMeta = resolveQuotaErrorMeta(account.quota_error);
      const hasQuotaError = Boolean(quotaErrorMeta.rawMessage);

      return (
        <div
          key={groupKey ? `${groupKey}-${account.id}` : account.id}
          className={`codex-account-card ${isCurrent ? 'current' : ''} ${isSelected ? 'selected' : ''}`}
        >
          <div className="card-top">
            <div className="card-select">
              <input
                type="checkbox"
                checked={isSelected}
                onChange={() => toggleSelect(account.id)}
              />
            </div>
            <span className="account-email" title={maskAccountText(account.email)}>
              {maskAccountText(account.email)}
            </span>
            {isCurrent && <span className="current-tag">{t('codex.current', '当前')}</span>}
            {hasQuotaError && (
              <span className="codex-status-pill quota-error" title={quotaErrorMeta.rawMessage}>
                <CircleAlert size={12} />
                {quotaErrorMeta.statusCode || t('codex.quotaError.badge', '配额异常')}
              </span>
            )}
            <span className={`tier-badge ${planKey.toLowerCase()}`}>{planLabel}</span>
          </div>

          <div className="codex-quota-section">
            {hasQuotaError && (
              <div className="quota-error-inline" title={quotaErrorMeta.rawMessage}>
                <CircleAlert size={14} />
                <span>{quotaErrorMeta.displayText}</span>
              </div>
            )}
            {quotaWindows.map((window) => {
              const QuotaIcon = window.id === 'secondary' ? Calendar : Clock;
              return (
                <div key={window.id} className="quota-item">
                  <div className="quota-header">
                    <QuotaIcon size={14} />
                    <span className="quota-label">{window.label}</span>
                    <span className={`quota-pct ${getCodexQuotaClass(window.percentage)}`}>
                      {window.percentage}%
                    </span>
                  </div>
                  <div className="quota-bar-track">
                    <div
                      className={`quota-bar ${getCodexQuotaClass(window.percentage)}`}
                      style={{ width: `${window.percentage}%` }}
                    />
                  </div>
                  {window.resetTime && (
                    <span className="quota-reset">
                      {formatCodexResetTime(window.resetTime, t)}
                    </span>
                  )}
                </div>
              );
            })}

            {!account.quota && (
              <div className="quota-empty">{t('common.shared.quota.noData', '暂无配额数据')}</div>
            )}
          </div>

          <div className="card-footer">
            <span className="card-date">{formatDate(account.created_at)}</span>
            <div className="card-actions">
              <button
                className="card-action-btn"
                onClick={() => openTagModal(account.id)}
                title={t('accounts.editTags', '编辑标签')}
              >
                <Tag size={14} />
              </button>
              <button
                className={`card-action-btn ${!isCurrent ? 'success' : ''}`}
                onClick={() => handleSwitch(account.id)}
                disabled={!!switching}
                title={t('codex.switch', '切换')}
              >
                {switching === account.id ? (
                  <RefreshCw size={14} className="loading-spinner" />
                ) : (
                  <Play size={14} />
                )}
              </button>
              <button
                className="card-action-btn"
                onClick={() => handleRefresh(account.id)}
                disabled={refreshing === account.id}
                title={t('common.shared.refreshQuota', '刷新配额')}
              >
                <RotateCw
                  size={14}
                  className={refreshing === account.id ? 'loading-spinner' : ''}
                />
              </button>
              <button
                className="card-action-btn danger"
                onClick={() => handleDelete(account.id)}
                title={t('common.delete', '删除')}
              >
                <Trash2 size={14} />
              </button>
            </div>
          </div>
        </div>
      );
    });

  const renderTableRows = (items: typeof filteredAccounts, groupKey?: string) =>
    items.map((account) => {
      const isCurrent = currentAccount?.id === account.id;
      const planKey = getCodexPlanDisplayName(account.plan_type);
      const planLabel = planKey;
      const quotaWindows = getCodexQuotaWindows(account.quota);
      const primaryWindow = quotaWindows[0];
      const secondaryWindow = quotaWindows[1];
      const quotaErrorMeta = resolveQuotaErrorMeta(account.quota_error);
      const hasQuotaError = Boolean(quotaErrorMeta.rawMessage);
      return (
        <tr key={groupKey ? `${groupKey}-${account.id}` : account.id} className={isCurrent ? 'current' : ''}>
          <td>
            <input
              type="checkbox"
              checked={selected.has(account.id)}
              onChange={() => toggleSelect(account.id)}
            />
          </td>
          <td>
            <div className="account-cell">
              <div className="account-main-line">
                <span className="account-email-text" title={maskAccountText(account.email)}>
                  {maskAccountText(account.email)}
                </span>
                {isCurrent && <span className="mini-tag current">{t('codex.current', '当前')}</span>}
              </div>
              {hasQuotaError && (
                <div className="account-sub-line">
                  <span className="codex-status-pill quota-error" title={quotaErrorMeta.rawMessage}>
                    <CircleAlert size={12} />
                    {quotaErrorMeta.statusCode || t('codex.quotaError.badge', '配额异常')}
                  </span>
                </div>
              )}
            </div>
          </td>
          <td>
            <span className={`tier-badge ${planKey.toLowerCase()}`}>{planLabel}</span>
          </td>
          <td>
            {primaryWindow ? (
              <div className="quota-item">
                <div className="quota-header">
                  <span className="quota-name">{primaryWindow.label}</span>
                  <span className={`quota-value ${getCodexQuotaClass(primaryWindow.percentage)}`}>
                    {primaryWindow.percentage}%
                  </span>
                </div>
                <div className="quota-progress-track">
                  <div
                    className={`quota-progress-bar ${getCodexQuotaClass(primaryWindow.percentage)}`}
                    style={{ width: `${primaryWindow.percentage}%` }}
                  />
                </div>
                {primaryWindow.resetTime && (
                  <div className="quota-footer">
                    <span className="quota-reset">
                      {formatCodexResetTime(primaryWindow.resetTime, t)}
                    </span>
                  </div>
                )}
              </div>
            ) : (
              <div className="quota-empty">—</div>
            )}
          </td>
          <td>
            {secondaryWindow ? (
              <div className="quota-item">
                <div className="quota-header">
                  <span className="quota-name">{secondaryWindow.label}</span>
                  <span className={`quota-value ${getCodexQuotaClass(secondaryWindow.percentage)}`}>
                    {secondaryWindow.percentage}%
                  </span>
                </div>
                <div className="quota-progress-track">
                  <div
                    className={`quota-progress-bar ${getCodexQuotaClass(secondaryWindow.percentage)}`}
                    style={{ width: `${secondaryWindow.percentage}%` }}
                  />
                </div>
                {secondaryWindow.resetTime && (
                  <div className="quota-footer">
                    <span className="quota-reset">
                      {formatCodexResetTime(secondaryWindow.resetTime, t)}
                    </span>
                  </div>
                )}
              </div>
            ) : (
              <div className="quota-empty">—</div>
            )}
            {hasQuotaError && (
              <div className="quota-error-inline table" title={quotaErrorMeta.rawMessage}>
                <CircleAlert size={12} />
                <span>{quotaErrorMeta.displayText}</span>
              </div>
            )}
          </td>
          <td className="sticky-action-cell table-action-cell">
            <div className="action-buttons">
              <button
                className="action-btn"
                onClick={() => openTagModal(account.id)}
                title={t('accounts.editTags', '编辑标签')}
              >
                <Tag size={14} />
              </button>
              <button
                className={`action-btn ${!isCurrent ? 'success' : ''}`}
                onClick={() => handleSwitch(account.id)}
                disabled={!!switching}
                title={t('codex.switch', '切换')}
              >
                {switching === account.id ? <RefreshCw size={14} className="loading-spinner" /> : <Play size={14} />}
              </button>
              <button
                className="action-btn"
                onClick={() => handleRefresh(account.id)}
                disabled={refreshing === account.id}
                title={t('common.shared.refreshQuota', '刷新配额')}
              >
                <RotateCw size={14} className={refreshing === account.id ? 'loading-spinner' : ''} />
              </button>
              <button
                className="action-btn danger"
                onClick={() => handleDelete(account.id)}
                title={t('common.delete', '删除')}
              >
                <Trash2 size={14} />
              </button>
            </div>
          </td>
        </tr>
      );
    });

  return (
    <div className="codex-accounts-page">
      <CodexOverviewTabsHeader active={activeTab} onTabChange={setActiveTab} />

      {activeTab === 'overview' && (
        <>

      {message && (
        <div className={`message-bar ${message.tone === 'error' ? 'error' : 'success'}`}>
          {message.text}
          <button onClick={() => setMessage(null)}>
            <X size={14} />
          </button>
        </div>
      )}

      <div className="toolbar">
        <div className="toolbar-left">
          <div className="search-box">
            <Search size={16} className="search-icon" />
            <input
              type="text"
              placeholder={t('common.shared.search', '搜索账号...')}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
            />
          </div>

          <div className="view-switcher">
            <button
              className={`view-btn ${viewMode === 'list' ? 'active' : ''}`}
              onClick={() => setViewMode('list')}
              title={t('common.shared.view.list', '列表视图')}
            >
              <List size={16} />
            </button>
            <button
              className={`view-btn ${viewMode === 'grid' ? 'active' : ''}`}
              onClick={() => setViewMode('grid')}
              title={t('common.shared.view.grid', '卡片视图')}
            >
              <LayoutGrid size={16} />
            </button>
          </div>

          <div className="filter-select">
            <select
              value={filterType}
              onChange={(e) => setFilterType(e.target.value as typeof filterType)}
              aria-label={t('common.shared.filterLabel', '筛选')}
            >
              <option value="all">{t('common.shared.filter.all', { count: tierCounts.all })}</option>
              <option value="FREE">{`FREE (${tierCounts.FREE})`}</option>
              <option value="PLUS">{`PLUS (${tierCounts.PLUS})`}</option>
              <option value="PRO">{`PRO (${tierCounts.PRO})`}</option>
              <option value="TEAM">{`TEAM (${tierCounts.TEAM})`}</option>
              <option value="ENTERPRISE">{`ENTERPRISE (${tierCounts.ENTERPRISE})`}</option>
            </select>
          </div>

          <div className="tag-filter" ref={tagFilterRef}>
            <button
              type="button"
              className={`tag-filter-btn ${tagFilter.length > 0 ? 'active' : ''}`}
              onClick={() => setShowTagFilter((prev) => !prev)}
              aria-label={t('accounts.filterTags', '标签筛选')}
            >
              <Tag size={14} />
              {tagFilter.length > 0 ? `${t('accounts.filterTagsCount', '标签')}(${tagFilter.length})` : t('accounts.filterTags', '标签筛选')}
            </button>
            {showTagFilter && (
              <div className="tag-filter-panel">
                {availableTags.length === 0 ? (
                  <div className="tag-filter-empty">{t('accounts.noAvailableTags', '暂无可用标签')}</div>
                ) : (
                  <div className="tag-filter-options">
                    {availableTags.map((tag) => (
                      <label key={tag} className={`tag-filter-option ${tagFilter.includes(tag) ? 'selected' : ''}`}>
                        <input
                          type="checkbox"
                          checked={tagFilter.includes(tag)}
                          onChange={() => toggleTagFilterValue(tag)}
                        />
                        <span className="tag-filter-name">{tag}</span>
                        <button
                          type="button"
                          className="tag-filter-delete"
                          onClick={(e) => {
                            e.preventDefault();
                            e.stopPropagation();
                            requestDeleteTag(tag);
                          }}
                          aria-label={t('accounts.deleteTagAria', {
                            tag,
                            defaultValue: '删除标签 {{tag}}',
                          })}
                        >
                          <X size={12} />
                        </button>
                      </label>
                    ))}
                  </div>
                )}
                <div className="tag-filter-divider" />
                <label className="tag-filter-group-toggle">
                  <input
                    type="checkbox"
                    checked={groupByTag}
                    onChange={(e) => setGroupByTag(e.target.checked)}
                  />
                  <span>{t('accounts.groupByTag', '按标签分组展示')}</span>
                </label>
                {tagFilter.length > 0 && (
                  <button type="button" className="tag-filter-clear" onClick={clearTagFilter}>
                    {t('accounts.clearFilter', '清空筛选')}
                  </button>
                )}
              </div>
            )}
          </div>

          <div className="sort-select">
            <ArrowDownWideNarrow size={14} className="sort-icon" />
            <select
              value={sortBy}
              onChange={(e) => setSortBy(e.target.value as typeof sortBy)}
              aria-label={t('common.shared.sortLabel', '排序')}
            >
              <option value="created_at">{t('common.shared.sort.createdAt', '按创建时间')}</option>
              <option value="weekly">{t('codex.sort.weekly', '按周配额')}</option>
              <option value="hourly">{t('codex.sort.hourly', '按5小时配额')}</option>
              <option value="weekly_reset">{t('codex.sort.weeklyReset', '按周配额重置时间')}</option>
              <option value="hourly_reset">{t('codex.sort.hourlyReset', '按5小时配额重置时间')}</option>
            </select>
          </div>

          <button
            className="sort-direction-btn"
            onClick={() => setSortDirection((prev) => (prev === 'desc' ? 'asc' : 'desc'))}
            title={
              sortDirection === 'desc'
                ? t('common.shared.sort.descTooltip', '当前：降序，点击切换为升序')
                : t('common.shared.sort.ascTooltip', '当前：升序，点击切换为降序')
            }
            aria-label={t('common.shared.sort.toggleDirection', '切换排序方向')}
          >
            {sortDirection === 'desc' ? '⬇' : '⬆'}
          </button>
        </div>
        <div className="toolbar-right">
          <button
            className="btn btn-primary icon-only"
            onClick={() => openAddModal('oauth')}
            title={t('common.shared.addAccount', '添加账号')}
            aria-label={t('common.shared.addAccount', '添加账号')}
          >
            <Plus size={14} />
          </button>
          <button
            className="btn btn-secondary icon-only"
            onClick={handleRefreshAll}
            disabled={refreshingAll || accounts.length === 0}
            title={t('common.shared.refreshAll', '刷新全部')}
            aria-label={t('common.shared.refreshAll', '刷新全部')}
          >
            <RefreshCw size={14} className={refreshingAll ? 'loading-spinner' : ''} />
          </button>
          <button
            className="btn btn-secondary icon-only"
            onClick={togglePrivacyMode}
            title={
              privacyModeEnabled
                ? t('privacy.showSensitive', '显示邮箱')
                : t('privacy.hideSensitive', '隐藏邮箱')
            }
            aria-label={
              privacyModeEnabled
                ? t('privacy.showSensitive', '显示邮箱')
                : t('privacy.hideSensitive', '隐藏邮箱')
            }
          >
            {privacyModeEnabled ? <EyeOff size={14} /> : <Eye size={14} />}
          </button>
          <button
            className="btn btn-secondary icon-only"
            onClick={() => openAddModal('token')}
            disabled={importing}
            title={t('common.shared.import.label', '导入')}
            aria-label={t('common.shared.import.label', '导入')}
          >
            <Download size={14} />
          </button>
          <button
            className="btn btn-secondary export-btn icon-only"
            onClick={handleExport}
            disabled={exporting}
            title={selected.size > 0 ? `${t('common.shared.export', '导出')} (${selected.size})` : t('common.shared.export', '导出')}
            aria-label={selected.size > 0 ? `${t('common.shared.export', '导出')} (${selected.size})` : t('common.shared.export', '导出')}
          >
            <Upload size={14} />
          </button>
          {selected.size > 0 && (
            <button
              className="btn btn-danger icon-only"
              onClick={handleBatchDelete}
              title={`${t('common.delete', '删除')} (${selected.size})`}
              aria-label={`${t('common.delete', '删除')} (${selected.size})`}
            >
              <Trash2 size={14} />
            </button>
          )}
            <QuickSettingsPopover type="codex" />
        </div>
      </div>

      {loading && accounts.length === 0 ? (
        <div className="loading-container">
          <RefreshCw size={24} className="loading-spinner" />
          <p>{t('common.loading', '加载中...')}</p>
        </div>
      ) : accounts.length === 0 ? (
        <div className="empty-state">
          <Globe size={48} />
          <h3>{t('common.shared.empty.title', '暂无账号')}</h3>
          <p>{t('codex.empty.description', '点击"添加账号"开始管理您的 Codex 账号')}</p>
          <button className="btn btn-primary" onClick={() => openAddModal('oauth')}>
            <Plus size={16} />
            {t('common.shared.addAccount', '添加账号')}
          </button>
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
                <div className="tag-group-grid codex-accounts-grid">
                  {renderGridCards(groupAccounts, groupKey)}
                </div>
              </div>
            ))}
          </div>
        ) : (
          <div className="codex-accounts-grid">
            {renderGridCards(filteredAccounts)}
          </div>
        )
      ) : groupByTag ? (
        <div className="account-table-container grouped">
          <table className="account-table">
            <thead>
              <tr>
                <th style={{ width: 40 }}>
                  <input
                    type="checkbox"
                    checked={selected.size === filteredAccounts.length && filteredAccounts.length > 0}
                    onChange={toggleSelectAll}
                  />
                </th>
                <th style={{ width: 260 }}>{t('common.shared.columns.email', '账号')}</th>
                <th style={{ width: 140 }}>{t('common.shared.columns.plan', '订阅')}</th>
                <th>{quotaColumnLabels.primary}</th>
                <th>{quotaColumnLabels.secondary}</th>
                <th className="sticky-action-header table-action-header">{t('common.shared.columns.actions', '操作')}</th>
              </tr>
            </thead>
            <tbody>
              {groupedAccounts.map(([groupKey, groupAccounts]) => (
                <Fragment key={groupKey}>
                  <tr className="tag-group-row">
                    <td colSpan={6}>
                      <div className="tag-group-header">
                        <span className="tag-group-title">{resolveGroupLabel(groupKey)}</span>
                        <span className="tag-group-count">{groupAccounts.length}</span>
                      </div>
                    </td>
                  </tr>
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
                <th style={{ width: 40 }}>
                  <input
                    type="checkbox"
                    checked={selected.size === filteredAccounts.length && filteredAccounts.length > 0}
                    onChange={toggleSelectAll}
                  />
                </th>
                <th style={{ width: 260 }}>{t('common.shared.columns.email', '账号')}</th>
                <th style={{ width: 140 }}>{t('common.shared.columns.plan', '订阅')}</th>
                <th>{quotaColumnLabels.primary}</th>
                <th>{quotaColumnLabels.secondary}</th>
                <th className="sticky-action-header table-action-header">{t('common.shared.columns.actions', '操作')}</th>
              </tr>
            </thead>
            <tbody>
              {renderTableRows(filteredAccounts)}
            </tbody>
          </table>
        </div>
      )}

      {showAddModal && (
        <div className="modal-overlay" onClick={closeAddModal}>
          <div className="modal-content codex-add-modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2>{t('codex.addModal.title', '添加 Codex 账号')}</h2>
              <button className="modal-close" onClick={closeAddModal} aria-label={t('common.close', '关闭')}>
                <X />
              </button>
            </div>

            <div className="modal-tabs">
              <button
                className={`modal-tab ${addTab === 'oauth' ? 'active' : ''}`}
                onClick={() => openAddModal('oauth')}
              >
                <Globe size={14} />
                OAuth
              </button>
              <button
                className={`modal-tab ${addTab === 'token' ? 'active' : ''}`}
                onClick={() => openAddModal('token')}
              >
                <KeyRound size={14} />
                Token / JSON
              </button>
              <button
                className={`modal-tab ${addTab === 'import' ? 'active' : ''}`}
                onClick={() => openAddModal('import')}
              >
                <Database size={14} />
                {t('accounts.tabs.import', '本地导入')}
              </button>
            </div>

            <div className="modal-body">
              {addTab === 'oauth' && (
                <div className="add-section">
                  <p className="section-desc">
                    {t('codex.oauth.desc', '通过 OpenAI 官方 OAuth 授权登录您的 Codex 账号。')}
                  </p>

                  {oauthPrepareError ? (
                    <div className="add-status error">
                      <CircleAlert size={16} />
                      <span>{oauthPrepareError}</span>
                      {oauthPortInUse && (
                        <button className="btn btn-sm btn-outline" onClick={handleReleaseOauthPort}>
                          {t('codex.oauth.portInUseAction', 'Close port and retry')}
                        </button>
                      )}
                      {!oauthPortInUse && oauthTimeoutInfo && (
                        <button className="btn btn-sm btn-outline" onClick={handleRetryOauthAfterTimeout}>
                          {t('codex.oauth.timeoutRetry', '刷新授权链接')}
                        </button>
                      )}
                    </div>
                  ) : oauthUrl ? (
                    <div className="oauth-url-section">
                      <div className="oauth-url-box">
                        <input type="text" value={oauthUrl} readOnly />
                        <button onClick={handleCopyOauthUrl}>
                          {oauthUrlCopied ? <Check size={16} /> : <Copy size={16} />}
                        </button>
                      </div>
                      <button
                        className="btn btn-primary btn-full"
                        onClick={isOauthTimeoutState ? handleRetryOauthAfterTimeout : handleOpenOauthUrl}
                      >
                        {isOauthTimeoutState ? <RefreshCw size={16} /> : <Globe size={16} />}
                        {isOauthTimeoutState
                          ? t('codex.oauth.timeoutRetry', '刷新授权链接')
                          : t('common.shared.oauth.openBrowser', 'Open in Browser')}
                      </button>
                      {isOauthTimeoutState && (
                        <div className="add-status error">
                          <CircleAlert size={16} />
                          <span>{t('codex.oauth.timeout', '授权超时，请点击“刷新授权链接”后重试。')}</span>
                        </div>
                      )}
                      <p className="oauth-hint">
                        {t('common.shared.oauth.hint', 'Once authorized, this window will update automatically')}
                      </p>
                    </div>
                  ) : (
                    <div className="oauth-loading">
                      <RefreshCw size={24} className="loading-spinner" />
                      <span>{t('codex.oauth.preparing', '正在准备授权链接...')}</span>
                    </div>
                  )}
                </div>
              )}

              {addTab === 'token' && (
                <div className="add-section">
                  <p className="section-desc">
                    {t('codex.token.desc', '粘贴您的 Codex Access Token 或导出的 JSON 数据。')}
                  </p>
                  <textarea
                    className="token-input"
                    value={tokenInput}
                    onChange={(e) => setTokenInput(e.target.value)}
                    placeholder={t('codex.token.placeholder', '粘贴 Token 或 JSON...')}
                  />
                  <button
                    className="btn btn-primary btn-full"
                    onClick={handleTokenImport}
                    disabled={importing || !tokenInput.trim()}
                  >
                    {importing ? <RefreshCw size={16} className="loading-spinner" /> : <Download size={16} />}
                    {t('common.shared.token.import', 'Import')}
                  </button>
                </div>
              )}

              {addTab === 'import' && (
                <div className="add-section">
                  <p className="section-desc">
                    {t('codex.import.localDesc', '从本地已登录的会话中导入 Codex 账号。')}
                  </p>
                  <button className="btn btn-primary btn-full" onClick={handleImportFromLocal} disabled={importing}>
                    {importing ? <RefreshCw size={16} className="loading-spinner" /> : <Database size={16} />}
                    {t('codex.local.import', 'Get Local Account')}
                  </button>
                </div>
              )}

              {addStatus !== 'idle' && addStatus !== 'loading' && (
                <div className={`add-status ${addStatus}`}>
                  {addStatus === 'success' ? <Check size={16} /> : <CircleAlert size={16} />}
                  <span>{addMessage}</span>
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {deleteConfirm && (
        <div className="modal-overlay" onClick={() => !deleting && setDeleteConfirm(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2>{t('common.confirm')}</h2>
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
              <button className="btn btn-secondary" onClick={() => setDeleteConfirm(null)} disabled={deleting}>
                {t('common.cancel')}
              </button>
              <button className="btn btn-danger" onClick={confirmDelete} disabled={deleting}>
                {t('common.confirm')}
              </button>
            </div>
          </div>
        </div>
      )}

      {tagDeleteConfirm && (
        <div className="modal-overlay" onClick={() => !deletingTag && setTagDeleteConfirm(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2>{t('common.confirm')}</h2>
              <button
                className="modal-close"
                onClick={() => !deletingTag && setTagDeleteConfirm(null)}
                aria-label={t('common.close', '关闭')}
              >
                <X />
              </button>
            </div>
            <div className="modal-body">
              <p>
                {t('accounts.confirmDeleteTag', 'Delete tag "{{tag}}"? This tag will be removed from {{count}} accounts.', { tag: tagDeleteConfirm.tag, count: tagDeleteConfirm.count })}
              </p>
            </div>
            <div className="modal-footer">
              <button className="btn btn-secondary" onClick={() => setTagDeleteConfirm(null)} disabled={deletingTag}>
                {t('common.cancel')}
              </button>
              <button className="btn btn-danger" onClick={confirmDeleteTag} disabled={deletingTag}>
                {deletingTag ? t('common.processing', '处理中...') : t('common.confirm')}
              </button>
            </div>
          </div>
        </div>
      )}

      <TagEditModal
        isOpen={!!showTagModal}
        initialTags={accounts.find((a) => a.id === showTagModal)?.tags || []}
        availableTags={availableTags}
        onClose={() => setShowTagModal(null)}
        onSave={handleSaveTags}
      />
        </>
      )}

      {activeTab === 'instances' && (
        <CodexInstancesContent />
      )}
    </div>
  );
}
