import { create } from 'zustand';
import type { PlatformId } from '../types/platform';
import type { PlatformPackageState } from '../types/platformPackage';
import {
  checkPlatformPackageUpdate,
  installPlatformPackage,
  listPlatformPackages,
  uninstallPlatformPackage,
  updatePlatformPackage,
} from '../services/platformPackageService';

const RUNTIME_MANAGED_PLATFORM_IDS = new Set<PlatformId>([
  'antigravity',
  'antigravity_ide',
  'claude_manager',
  'zed',
  'kiro',
  'github-copilot',
  'windsurf',
  'cursor',
  'gemini',
  'trae',
  'qoder',
  'codebuddy',
  'codebuddy_cn',
  'workbuddy',
  'codex',
]);

const EMPTY_CONTRIBUTIONS: PlatformPackageState['contributions'] = {
  platforms: [],
  dataPaths: [],
  localStorageKeys: [],
  nativeBoundaries: [],
};

const DEFAULT_PLATFORM_PACKAGES: PlatformPackageState[] = [
  {
    platformId: 'antigravity',
    packageMode: 'hotUpdate',
    installKind: 'sidecarAdapter',
    installStatus: 'notInstalled',
    runtimeReady: false,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: EMPTY_CONTRIBUTIONS,
    changelog: [],
  },
  {
    platformId: 'antigravity_ide',
    packageMode: 'hotUpdate',
    installKind: 'sidecarAdapter',
    installStatus: 'notInstalled',
    runtimeReady: false,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: EMPTY_CONTRIBUTIONS,
    changelog: [],
  },
  {
    platformId: 'claude_manager',
    packageMode: 'hotUpdate',
    installKind: 'sidecarAdapter',
    installStatus: 'notInstalled',
    runtimeReady: false,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: EMPTY_CONTRIBUTIONS,
    changelog: [],
  },
  {
    platformId: 'zed',
    packageMode: 'hotUpdate',
    installKind: 'sidecarAdapter',
    installStatus: 'notInstalled',
    runtimeReady: false,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: EMPTY_CONTRIBUTIONS,
    changelog: [],
  },
  {
    platformId: 'kiro',
    packageMode: 'hotUpdate',
    installKind: 'sidecarAdapter',
    installStatus: 'notInstalled',
    runtimeReady: false,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: EMPTY_CONTRIBUTIONS,
    changelog: [],
  },
  {
    platformId: 'github-copilot',
    packageMode: 'hotUpdate',
    installKind: 'sidecarAdapter',
    installStatus: 'notInstalled',
    runtimeReady: false,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: EMPTY_CONTRIBUTIONS,
    changelog: [],
  },
  {
    platformId: 'windsurf',
    packageMode: 'hotUpdate',
    installKind: 'sidecarAdapter',
    installStatus: 'notInstalled',
    runtimeReady: false,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: EMPTY_CONTRIBUTIONS,
    changelog: [],
  },
  {
    platformId: 'cursor',
    packageMode: 'hotUpdate',
    installKind: 'sidecarAdapter',
    installStatus: 'notInstalled',
    runtimeReady: false,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: EMPTY_CONTRIBUTIONS,
    changelog: [],
  },
  {
    platformId: 'gemini',
    packageMode: 'hotUpdate',
    installKind: 'sidecarAdapter',
    installStatus: 'notInstalled',
    runtimeReady: false,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: EMPTY_CONTRIBUTIONS,
    changelog: [],
  },
  {
    platformId: 'trae',
    packageMode: 'hotUpdate',
    installKind: 'sidecarAdapter',
    installStatus: 'notInstalled',
    runtimeReady: false,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: EMPTY_CONTRIBUTIONS,
    changelog: [],
  },
  {
    platformId: 'qoder',
    packageMode: 'hotUpdate',
    installKind: 'sidecarAdapter',
    installStatus: 'notInstalled',
    runtimeReady: false,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: EMPTY_CONTRIBUTIONS,
    changelog: [],
  },
  {
    platformId: 'codebuddy',
    packageMode: 'hotUpdate',
    installKind: 'sidecarAdapter',
    installStatus: 'notInstalled',
    runtimeReady: false,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: EMPTY_CONTRIBUTIONS,
    changelog: [],
  },
  {
    platformId: 'codebuddy_cn',
    packageMode: 'hotUpdate',
    installKind: 'sidecarAdapter',
    installStatus: 'notInstalled',
    runtimeReady: false,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: EMPTY_CONTRIBUTIONS,
    changelog: [],
  },
  {
    platformId: 'workbuddy',
    packageMode: 'hotUpdate',
    installKind: 'sidecarAdapter',
    installStatus: 'notInstalled',
    runtimeReady: false,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: EMPTY_CONTRIBUTIONS,
    changelog: [],
  },
  {
    platformId: 'codex',
    packageMode: 'hotUpdate',
    installKind: 'sidecarAdapter',
    installStatus: 'notInstalled',
    runtimeReady: false,
    installedVersion: null,
    latestVersion: null,
    downloadSizeBytes: null,
    installedSizeBytes: null,
    lastCheckedAt: null,
    errorMessage: null,
    entry: null,
    adapter: null,
    ui: null,
    capabilities: [],
    contributions: EMPTY_CONTRIBUTIONS,
    changelog: [],
  },
];

interface PlatformPackageStoreState {
  packages: PlatformPackageState[];
  initialized: boolean;
  loading: boolean;
  error: string | null;
  refresh: () => Promise<PlatformPackageState[]>;
  checkUpdate: (platformId: PlatformId) => Promise<PlatformPackageState>;
  installPackage: (platformId: PlatformId) => Promise<PlatformPackageState>;
  updatePackage: (platformId: PlatformId) => Promise<PlatformPackageState>;
  uninstallPackage: (platformId: PlatformId) => Promise<PlatformPackageState>;
  getPackage: (platformId: PlatformId) => PlatformPackageState | null;
  isHotUpdatePlatform: (platformId: PlatformId) => boolean;
  canShowPlatformEntry: (platformId: PlatformId) => boolean;
  isInstallRequired: (platformId: PlatformId) => boolean;
  isRuntimeReady: (platformId: PlatformId) => boolean;
  canOpenPlatform: (platformId: PlatformId) => boolean;
}

function upsertPackage(
  packages: PlatformPackageState[],
  nextPackage: PlatformPackageState,
): PlatformPackageState[] {
  const index = packages.findIndex((item) => item.platformId === nextPackage.platformId);
  if (index < 0) {
    return [...packages, nextPackage];
  }
  const next = [...packages];
  next[index] = nextPackage;
  return next;
}

function packageForPlatform(
  packages: PlatformPackageState[],
  platformId: PlatformId,
): PlatformPackageState | null {
  return packages.find((item) => item.platformId === platformId) ?? null;
}

function mergeKnownPackages(packages: PlatformPackageState[]): PlatformPackageState[] {
  return DEFAULT_PLATFORM_PACKAGES.reduce(
    (next, defaultPackage) => (
      packageForPlatform(next, defaultPackage.platformId)
        ? next
        : [...next, defaultPackage]
    ),
    [...packages],
  );
}

function isPlatformRuntimeReady(
  packages: PlatformPackageState[],
  platformId: PlatformId,
): boolean {
  const runtimePackage = packageForPlatform(packages, platformId);
  if (!runtimePackage) {
    return !RUNTIME_MANAGED_PLATFORM_IDS.has(platformId);
  }
  if (runtimePackage.packageMode !== 'hotUpdate') {
    return true;
  }
  return runtimePackage.installStatus === 'installed'
    || runtimePackage.installStatus === 'updateAvailable'
      ? runtimePackage.runtimeReady
      : false;
}

function isRuntimeManagedPlatformId(platformId: PlatformId): boolean {
  return RUNTIME_MANAGED_PLATFORM_IDS.has(platformId);
}

function isHotUpdatePlatformPackage(
  packages: PlatformPackageState[],
  platformId: PlatformId,
): boolean {
  const runtimePackage = packageForPlatform(packages, platformId);
  return runtimePackage?.packageMode === 'hotUpdate';
}

function canShowPlatformEntry(
  packages: PlatformPackageState[],
  platformId: PlatformId,
): boolean {
  const runtimePackage = packageForPlatform(packages, platformId);
  if (!runtimePackage) {
    return !isRuntimeManagedPlatformId(platformId);
  }
  if (runtimePackage.packageMode === 'hotUpdate') {
    return true;
  }
  return true;
}

function isPlatformPackageInstallRequired(
  packages: PlatformPackageState[],
  platformId: PlatformId,
): boolean {
  const runtimePackage = packageForPlatform(packages, platformId);
  if (!runtimePackage || runtimePackage.packageMode !== 'hotUpdate') {
    return false;
  }
  return !isPlatformRuntimeReady(packages, platformId);
}

export const usePlatformPackageStore = create<PlatformPackageStoreState>((set, get) => ({
  packages: DEFAULT_PLATFORM_PACKAGES,
  initialized: false,
  loading: false,
  error: null,

  refresh: async () => {
    set({ loading: true, error: null });
    try {
      const packages = mergeKnownPackages(await listPlatformPackages());
      set({ packages, loading: false, initialized: true });
      return packages;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      set({ loading: false, initialized: false, error: message });
      throw error;
    }
  },

  checkUpdate: async (platformId) => {
    const nextPackage = await checkPlatformPackageUpdate(platformId);
    set((state) => ({
      packages: upsertPackage(state.packages, nextPackage),
      initialized: true,
      error: null,
    }));
    return nextPackage;
  },

  installPackage: async (platformId) => {
    const nextPackage = await installPlatformPackage(platformId);
    set((state) => ({
      packages: upsertPackage(state.packages, nextPackage),
      initialized: true,
      error: null,
    }));
    return nextPackage;
  },

  updatePackage: async (platformId) => {
    const nextPackage = await updatePlatformPackage(platformId);
    set((state) => ({
      packages: upsertPackage(state.packages, nextPackage),
      initialized: true,
      error: null,
    }));
    return nextPackage;
  },

  uninstallPackage: async (platformId) => {
    const nextPackage = await uninstallPlatformPackage(platformId);
    set((state) => ({
      packages: upsertPackage(state.packages, nextPackage),
      initialized: true,
      error: null,
    }));
    return nextPackage;
  },

  getPackage: (platformId) => packageForPlatform(get().packages, platformId),

  isHotUpdatePlatform: (platformId) => {
    return isHotUpdatePlatformPackage(get().packages, platformId);
  },

  canShowPlatformEntry: (platformId) => {
    return canShowPlatformEntry(get().packages, platformId);
  },

  isInstallRequired: (platformId) => {
    return isPlatformPackageInstallRequired(get().packages, platformId);
  },

  isRuntimeReady: (platformId) => {
    const state = get();
    return isPlatformRuntimeReady(state.packages, platformId);
  },

  canOpenPlatform: (platformId) => {
    const state = get();
    return isPlatformRuntimeReady(state.packages, platformId);
  },
}));

export function canOpenPlatformFromPackages(
  packages: PlatformPackageState[],
  _initialized: boolean,
  platformId: PlatformId,
): boolean {
  return isPlatformRuntimeReady(packages, platformId);
}

export function getPlatformPackageFromPackages(
  packages: PlatformPackageState[],
  platformId: PlatformId,
): PlatformPackageState | null {
  return packageForPlatform(packages, platformId);
}

export function isRuntimeManagedPlatform(platformId: PlatformId): boolean {
  return isRuntimeManagedPlatformId(platformId);
}

export function isHotUpdatePlatformFromPackages(
  packages: PlatformPackageState[],
  platformId: PlatformId,
): boolean {
  return isHotUpdatePlatformPackage(packages, platformId);
}

export function canShowPlatformEntryFromPackages(
  packages: PlatformPackageState[],
  _initialized: boolean,
  platformId: PlatformId,
): boolean {
  return canShowPlatformEntry(packages, platformId);
}

export function isPlatformPackageInstallRequiredFromPackages(
  packages: PlatformPackageState[],
  _initialized: boolean,
  platformId: PlatformId,
): boolean {
  return isPlatformPackageInstallRequired(packages, platformId);
}

export function formatPlatformPackageSize(size?: number | null): string {
  if (size == null || !Number.isFinite(size) || size <= 0) {
    return '--';
  }
  if (size < 1024) {
    return `${Math.round(size)} B`;
  }
  const kb = size / 1024;
  if (kb < 1024) {
    return `${kb.toFixed(kb >= 100 ? 0 : 1)} KB`;
  }
  const mb = kb / 1024;
  if (mb < 1024) {
    return `${mb.toFixed(mb >= 100 ? 0 : 1)} MB`;
  }
  const gb = mb / 1024;
  return `${gb.toFixed(gb >= 100 ? 0 : 1)} GB`;
}
