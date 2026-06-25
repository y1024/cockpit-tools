import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { PlatformId } from '../../types/platform';
import type { PlatformPackageState, PlatformPackageUiEntry } from '../../types/platformPackage';
import { getPlatformPackageUiEntry } from '../../services/platformPackageService';
import './PlatformRuntimePageHost.css';

interface PlatformRuntimePageHostProps {
  platformId: PlatformId;
  state: PlatformPackageState;
  className?: string;
  tabsSlotId?: string;
  runtimeParams?: Record<string, unknown>;
}

type PlatformRemoteHostApi = {
  platformId: PlatformId;
  packageVersion: string | null;
  locale: string;
  theme: string;
  state: PlatformPackageState;
  tabsSlotId?: string;
  runtimeParams?: Record<string, unknown>;
};

type PlatformRemoteModule = {
  mount?: (
    container: HTMLElement,
    hostApi: PlatformRemoteHostApi,
  ) =>
    | void
    | (() => void)
    | { unmount?: () => void }
    | Promise<void | (() => void) | { unmount?: () => void }>;
  unmount?: (container: HTMLElement) => void;
};

type CachedPlatformRemoteRuntime = {
  key: string;
  platformId: PlatformId;
  entryPromise: Promise<PlatformPackageUiEntry>;
  remotePromise: Promise<PlatformRemoteModule>;
  moduleUrl: string | null;
  styleElement: HTMLStyleElement | null;
  lastUsedAt: number;
};

const REMOTE_RUNTIME_CACHE_MAX = 24;
const platformRemoteRuntimeCache = new Map<string, CachedPlatformRemoteRuntime>();

function buildRemoteModuleUrl(source: string): string {
  const blob = new Blob([source], { type: 'text/javascript;charset=utf-8' });
  return URL.createObjectURL(blob);
}

function buildRemoteRuntimeCacheKey(
  platformId: PlatformId,
  state: PlatformPackageState,
): string {
  return [
    platformId,
    state.installedVersion || state.latestVersion || 'unknown',
    state.installedSizeBytes ?? 'unknown-size',
  ].join('@');
}

function cleanupCachedRemoteRuntime(cached: CachedPlatformRemoteRuntime): void {
  cached.styleElement?.remove();
  cached.styleElement = null;
  if (cached.moduleUrl) {
    URL.revokeObjectURL(cached.moduleUrl);
    cached.moduleUrl = null;
  }
}

function evictStaleRemoteRuntimeCache(): void {
  while (platformRemoteRuntimeCache.size > REMOTE_RUNTIME_CACHE_MAX) {
    let oldestKey: string | null = null;
    let oldestLastUsedAt = Number.POSITIVE_INFINITY;
    for (const [key, cached] of platformRemoteRuntimeCache) {
      if (cached.lastUsedAt < oldestLastUsedAt) {
        oldestKey = key;
        oldestLastUsedAt = cached.lastUsedAt;
      }
    }
    if (!oldestKey) return;
    const cached = platformRemoteRuntimeCache.get(oldestKey);
    if (cached) {
      cleanupCachedRemoteRuntime(cached);
      platformRemoteRuntimeCache.delete(oldestKey);
    }
  }
}

function getCachedPlatformRemoteRuntime(
  platformId: PlatformId,
  state: PlatformPackageState,
): CachedPlatformRemoteRuntime {
  const key = buildRemoteRuntimeCacheKey(platformId, state);
  const existing = platformRemoteRuntimeCache.get(key);
  if (existing) {
    existing.lastUsedAt = Date.now();
    return existing;
  }

  const cached: CachedPlatformRemoteRuntime = {
    key,
    platformId,
    entryPromise: getPlatformPackageUiEntry(platformId),
    remotePromise: undefined as unknown as Promise<PlatformRemoteModule>,
    moduleUrl: null,
    styleElement: null,
    lastUsedAt: Date.now(),
  };
  cached.remotePromise = cached.entryPromise
    .then(async (entry) => {
      if (entry.protocol !== 'react-remote-esm-v1') {
        throw new Error(`Unsupported platform UI protocol: ${entry.protocol}`);
      }
      if (!entry.exports.includes('mount')) {
        throw new Error('Platform remote UI is missing mount export');
      }
      cached.moduleUrl = buildRemoteModuleUrl(entry.source);
      const remote = await import(/* @vite-ignore */ cached.moduleUrl) as PlatformRemoteModule;
      if (typeof remote.mount !== 'function') {
        throw new Error('Platform remote UI mount export is not a function');
      }
      return remote;
    })
    .catch((error) => {
      cleanupCachedRemoteRuntime(cached);
      platformRemoteRuntimeCache.delete(key);
      throw error;
    });

  platformRemoteRuntimeCache.set(key, cached);
  evictStaleRemoteRuntimeCache();
  return cached;
}

function getPlatformRootClass(platformId: PlatformId): string {
  return `${platformId}-platform-ui-root`;
}

function getTabsSlotRootId(tabsSlotId: string, platformId: PlatformId): string {
  return `${tabsSlotId}__${platformId.replace(/[^a-zA-Z0-9_-]/g, '-')}-remote-root`;
}

function installRemoteStyle(
  cached: CachedPlatformRemoteRuntime,
  version: string,
  style: string | null | undefined,
): HTMLStyleElement | null {
  const { platformId } = cached;
  document.head.querySelectorAll('style[data-platform-remote-style]').forEach((element) => {
    const styleElement = element as HTMLStyleElement;
    if (
      styleElement.dataset.platformRemoteStyle === platformId
      && styleElement.dataset.platformRemoteCacheKey !== cached.key
    ) {
      element.remove();
    }
  });

  if (!style) return null;
  const element = cached.styleElement ?? document.createElement('style');
  element.dataset.platformRemoteStyle = platformId;
  element.dataset.platformRemoteVersion = version;
  element.dataset.platformRemoteCacheKey = cached.key;
  if (!cached.styleElement || element.textContent !== style) {
    element.textContent = style;
  }
  if (!element.isConnected) {
    document.head.appendChild(element);
  }
  cached.styleElement = element;
  return element;
}

function detachRemoteStyle(cached: CachedPlatformRemoteRuntime | null): void {
  cached?.styleElement?.remove();
}

function resolveCleanup(
  remote: PlatformRemoteModule,
  container: HTMLElement,
  mounted: void | (() => void) | { unmount?: () => void },
): () => void {
  if (typeof mounted === 'function') {
    return mounted;
  }
  if (mounted && typeof mounted === 'object' && typeof mounted.unmount === 'function') {
    return () => mounted.unmount?.();
  }
  if (typeof remote.unmount === 'function') {
    return () => remote.unmount?.(container);
  }
  return () => {
    container.replaceChildren();
  };
}

function stableStringify(value: unknown): string {
  if (value === undefined) return '';
  if (value === null || typeof value !== 'object') return JSON.stringify(value);
  if (Array.isArray(value)) {
    return `[${value.map((item) => stableStringify(item)).join(',')}]`;
  }
  const objectValue = value as Record<string, unknown>;
  return `{${Object.keys(objectValue)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${stableStringify(objectValue[key])}`)
    .join(',')}}`;
}

export function PlatformRuntimePageHost({
  platformId,
  state,
  className,
  tabsSlotId,
  runtimeParams,
}: PlatformRuntimePageHostProps) {
  const { t, i18n } = useTranslation();
  const containerRef = useRef<HTMLDivElement | null>(null);
  const cleanupRef = useRef<(() => void) | null>(null);
  const stateRef = useRef(state);
  const runtimeParamsRef = useRef(runtimeParams);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const locale = i18n.language || 'zh-cn';
  const runtimeCacheKey = buildRemoteRuntimeCacheKey(platformId, state);
  const runtimeParamsKey = stableStringify(runtimeParams);

  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  useEffect(() => {
    runtimeParamsRef.current = runtimeParams;
  }, [runtimeParams]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let cancelled = false;
    let cachedRuntime: CachedPlatformRemoteRuntime | null = null;
    let tabsSlotElement: HTMLElement | null = null;
    let tabsSlotRootElement: HTMLElement | null = null;
    const platformRootClass = getPlatformRootClass(platformId);

    const cleanupInjectedAssets = () => {
      detachRemoteStyle(cachedRuntime);
    };

    const cleanupTabsSlot = () => {
      if (tabsSlotRootElement?.parentElement) {
        tabsSlotRootElement.remove();
      }
      tabsSlotElement = null;
      tabsSlotRootElement = null;
    };

    const cleanupMounted = () => {
      if (cleanupRef.current) {
        try {
          cleanupRef.current();
        } catch (cleanupError) {
          console.warn('Platform remote cleanup failed:', cleanupError);
        }
        cleanupRef.current = null;
      }
    };

    setLoading(true);
    setError(null);
    cleanupMounted();
    container.replaceChildren();
    tabsSlotElement = tabsSlotId ? document.getElementById(tabsSlotId) : null;
    if (tabsSlotElement) {
      tabsSlotElement.replaceChildren();
      tabsSlotRootElement = document.createElement('span');
      tabsSlotRootElement.id = getTabsSlotRootId(tabsSlotId!, platformId);
      tabsSlotRootElement.className = `platform-remote-tabs-root ${platformRootClass}`;
      tabsSlotRootElement.dataset.platformRemoteTabsRoot = platformId;
      tabsSlotElement.appendChild(tabsSlotRootElement);
    }

    cachedRuntime = getCachedPlatformRemoteRuntime(platformId, stateRef.current);
    cachedRuntime.lastUsedAt = Date.now();

    void Promise.all([cachedRuntime.entryPromise, cachedRuntime.remotePromise])
      .then(async ([entry, remote]: [PlatformPackageUiEntry, PlatformRemoteModule]) => {
        if (cancelled) return;

        installRemoteStyle(cachedRuntime, entry.version, entry.style);
        if (cancelled) {
          cleanupInjectedAssets();
          return;
        }

        const mount = remote.mount;
        if (typeof mount !== 'function') {
          throw new Error('Platform remote UI mount export is not a function');
        }

        const hostApi: PlatformRemoteHostApi = {
          platformId,
          packageVersion: entry.version,
          locale,
          theme: document.documentElement.dataset.theme || 'light',
          state: stateRef.current,
          tabsSlotId: tabsSlotRootElement?.id ?? tabsSlotId,
          runtimeParams: runtimeParamsRef.current,
        };
        const mounted = await mount(container, hostApi);
        if (cancelled) {
          resolveCleanup(remote, container, mounted)();
          return;
        }
        cleanupRef.current = resolveCleanup(remote, container, mounted);
      })
      .catch((loadError) => {
        if (cancelled) return;
        cleanupMounted();
        cleanupInjectedAssets();
        cleanupTabsSlot();
        container.replaceChildren();
        const message = loadError instanceof Error ? loadError.message : String(loadError);
        setError(message);
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
      cleanupMounted();
      cleanupInjectedAssets();
      cleanupTabsSlot();
    };
  }, [
    locale,
    platformId,
    runtimeCacheKey,
    tabsSlotId,
    runtimeParamsKey,
  ]);

  return (
    <div className={`platform-runtime-page-host ${className || ''}`.trim()}>
      {loading ? (
        <div className="platform-runtime-placeholder">
          {t('common.loading', '加载中...')}
        </div>
      ) : null}
      {error ? (
        <div className="platform-runtime-placeholder is-error">
          <strong>{t('platformLayout.packageError', '状态异常')}</strong>
          <span>{error}</span>
        </div>
      ) : null}
      <div
        ref={containerRef}
        className={`platform-runtime-remote-root ${loading || error ? 'is-hidden' : ''}`}
      />
    </div>
  );
}
