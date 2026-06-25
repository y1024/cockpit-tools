import { PlatformId } from './platform';

export type PlatformPackageMode = 'bundled' | 'hotUpdate';
export type PlatformPackageInstallKind = 'coreNativeBoundary' | 'sidecarAdapter';

export type PlatformPackageInstallStatus =
  | 'notInstalled'
  | 'installed'
  | 'updateAvailable'
  | 'installing'
  | 'updating'
  | 'uninstalling'
  | 'error'
  | 'incompatible';

export interface PlatformPackageState {
  platformId: PlatformId;
  packageMode: PlatformPackageMode;
  installKind: PlatformPackageInstallKind;
  installStatus: PlatformPackageInstallStatus;
  runtimeReady: boolean;
  installedVersion?: string | null;
  latestVersion?: string | null;
  downloadSizeBytes?: number | null;
  installedSizeBytes?: number | null;
  lastCheckedAt?: number | null;
  errorMessage?: string | null;
  entry?: string | null;
  adapter?: PlatformPackageAdapter | null;
  ui?: PlatformPackageUi | null;
  capabilities: string[];
  contributions: PlatformPackageContributions;
  changelog: PlatformPackageChangelogEntry[];
}

export interface PlatformPackageChangelogEntry {
  version: string;
  date?: string | null;
  notes: string[];
  locales?: Record<string, PlatformPackageChangelogLocale> | null;
}

export interface PlatformPackageChangelogLocale {
  notes?: string[];
}

export interface PlatformPackageAdapter {
  protocol: string;
  entry: string;
  macosEntry?: string | null;
  windowsEntry?: string | null;
  linuxEntry?: string | null;
  methods: string[];
}

export interface PlatformPackageUi {
  protocol: string;
  entry: string;
  style?: string | null;
  exports?: string[];
  sandbox?: string | null;
}

export interface PlatformPackageUiEntry {
  platformId: PlatformId;
  version: string;
  protocol: string;
  entry: string;
  exports: string[];
  sandbox?: string | null;
  source: string;
  style?: string | null;
}

export interface PlatformPackagePlatformContribution {
  id: PlatformId;
  label: string;
  labelKey?: string | null;
  iconKey?: string | null;
  page: string;
}

export interface PlatformPackageContributions {
  platforms: PlatformPackagePlatformContribution[];
  dataPaths: string[];
  localStorageKeys: string[];
  nativeBoundaries: string[];
}
