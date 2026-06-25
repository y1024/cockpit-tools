#!/usr/bin/env node

const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.resolve(__dirname, '..');
const INDEX_PATH = path.join(ROOT, 'platform-packages', 'index.json');
const PAGES_DIR = path.join(ROOT, 'src', 'pages');
const PLATFORM_UI_DIR = path.join(ROOT, 'src', 'platform-ui');
const REPORT_PATH = path.join(ROOT, '.aitasks', 'platform-ui-smoke-report.json');
const TOOLBAR_PATH = path.join(ROOT, 'src', 'components', 'PlatformPackageToolbar.tsx');
const UNAVAILABLE_PAGE_PATH = path.join(ROOT, 'src', 'components', 'PlatformPackageUnavailablePage.tsx');
const RUNTIME_HOST_PATH = path.join(ROOT, 'src', 'components', 'platform', 'PlatformRuntimePageHost.tsx');
const LOG_VIEWER_PATH = path.join(ROOT, 'src', 'components', 'LogViewerModal.tsx');
const LOG_SERVICE_PATH = path.join(ROOT, 'src', 'services', 'logService.ts');
const LOG_COMMAND_PATH = path.join(ROOT, 'src-tauri', 'src', 'commands', 'logs.rs');
const LOGGER_PATH = path.join(ROOT, 'src-tauri', 'src', 'modules', 'logger.rs');

const PAGE_REQUIRED_SNIPPETS = [
  'PlatformPackageToolbar',
  'PlatformPackageUnavailablePage',
  'PlatformRuntimePageHost',
  'usePlatformPackageStore',
  'platformPackage.runtimeReady',
  '<PlatformRuntimePageHost',
  '<PlatformPackageUnavailablePage',
  '<PlatformPackageToolbar',
];

const TOOLBAR_REQUIRED_SNIPPETS = [
  'handleCheckUpdate',
  'showUpdateDialog',
  "confirmAction('uninstall')",
  'platformLayout.packageCheckUpdate',
  'platformLayout.packageUninstall',
  'platformLayout.packageChangelog',
  'update_notification.skipThisVersion',
  'update_notification.updateNow',
  'packageMode === \'hotUpdate\'',
];

const UNAVAILABLE_REQUIRED_SNIPPETS = [
  'installPackage',
  'PlatformPackageUnavailablePage',
  'packageInstallNotReady',
  'runtimeReady',
];

const RUNTIME_HOST_REQUIRED_SNIPPETS = [
  'react-remote-esm-v1',
  'getPlatformPackageUiEntry',
  'Platform remote UI mount export is not a function',
  'Platform remote UI is missing mount export',
  'platformRemoteRuntimeCache',
  'data-platform-remote-style',
];

const LOG_REQUIRED_SNIPPETS = [
  {
    filePath: LOG_VIEWER_PATH,
    snippets: ['available_files', 'log_file_display_name', 'getLogSnapshot'],
  },
  {
    filePath: LOG_SERVICE_PATH,
    snippets: ['log_file_display_name', 'available_files', "invoke('logs_get_snapshot'"],
  },
  {
    filePath: LOG_COMMAND_PATH,
    snippets: ['display_log_file_name', 'list_managed_log_files', 'available_files'],
  },
  {
    filePath: LOGGER_PATH,
    snippets: [
      'platform_log_file_prefix',
      'platform_id_from_log_message',
      'migrate_legacy_log_file_names',
      'platform-{}-{}.log',
      'app-2026-06-24.log',
      'platform-zed-2026-06-24.log',
    ],
  },
];

function readText(filePath) {
  return fs.readFileSync(filePath, 'utf8');
}

function readJson(filePath) {
  return JSON.parse(readText(filePath));
}

function relative(filePath) {
  return path.relative(ROOT, filePath);
}

function assertIncludes(issues, label, source, snippet) {
  if (!source.includes(snippet)) {
    issues.push(`${label}: missing ${snippet}`);
  }
}

function findPageForPackage(packageId) {
  if (packageId === 'antigravity' || packageId === 'antigravity_ide') {
    return path.join(PAGES_DIR, 'AntigravitySuitePage.tsx');
  }

  const candidates = fs.readdirSync(PAGES_DIR)
    .filter((name) => name.endsWith('AccountsPage.tsx') && name !== 'AccountsPage.tsx')
    .map((name) => path.join(PAGES_DIR, name));
  return candidates.find((filePath) => {
    const source = readText(filePath);
    return source.includes(`'${packageId}'`) || source.includes(`"${packageId}"`);
  }) || null;
}

function hasRemoteExport(source, exportName) {
  const escaped = exportName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp(
    `export\\s+(?:async\\s+)?function\\s+${escaped}\\b|export\\s+(?:const|let|var)\\s+${escaped}\\b`,
  ).test(source) || new RegExp(
    `export\\s*\\{[\\s\\S]*?(?:\\b${escaped}\\b|\\bas\\s+${escaped}\\b)[\\s\\S]*?\\}`,
  ).test(source);
}

function verifyPageShell(pkg) {
  const issues = [];
  const pagePath = findPageForPackage(pkg.id);
  if (!pagePath) {
    issues.push('missing account page shell');
    return { ok: false, pagePath: null, issues };
  }

  const source = readText(pagePath);
  for (const snippet of PAGE_REQUIRED_SNIPPETS) {
    assertIncludes(issues, `${pkg.id}: ${relative(pagePath)}`, source, snippet);
  }

  if (pkg.id === 'antigravity' || pkg.id === 'antigravity_ide') {
    assertIncludes(issues, `${pkg.id}: ${relative(pagePath)}`, source, 'platformId');
  } else {
    assertIncludes(issues, `${pkg.id}: ${relative(pagePath)}`, source, pkg.id);
  }

  return {
    ok: issues.length === 0,
    pagePath: relative(pagePath),
    issues,
  };
}

function verifyRemoteUi(pkg) {
  const issues = [];
  const sourceRemotePath = path.join(PLATFORM_UI_DIR, pkg.id, 'remote.tsx');
  const sourceStylePath = path.join(PLATFORM_UI_DIR, pkg.id, 'style.css');
  const packageRoot = path.join(ROOT, 'platform-packages', pkg.id);
  const manifestPath = path.join(packageRoot, 'manifest.json');

  if (!fs.existsSync(sourceRemotePath)) {
    issues.push(`missing source remote ${relative(sourceRemotePath)}`);
  } else {
    const source = readText(sourceRemotePath);
    if (!hasRemoteExport(source, 'mount')) {
      issues.push(`${relative(sourceRemotePath)} missing mount export`);
    }
    if (!hasRemoteExport(source, 'unmount')) {
      issues.push(`${relative(sourceRemotePath)} missing unmount export`);
    }
  }
  if (!fs.existsSync(sourceStylePath)) {
    issues.push(`missing source style ${relative(sourceStylePath)}`);
  }

  if (!fs.existsSync(manifestPath)) {
    issues.push(`missing manifest ${relative(manifestPath)}`);
    return { ok: false, issues };
  }

  const manifest = readJson(manifestPath);
  if (manifest.ui?.protocol !== 'react-remote-esm-v1') {
    issues.push('manifest ui.protocol must be react-remote-esm-v1');
  }
  const entryPath = path.join(packageRoot, manifest.ui?.entry || '');
  const stylePath = path.join(packageRoot, manifest.ui?.style || '');
  if (!fs.existsSync(entryPath)) {
    issues.push(`missing packaged remote ${relative(entryPath)}`);
  } else {
    const entrySource = readText(entryPath);
    if (!hasRemoteExport(entrySource, 'mount')) {
      issues.push(`${relative(entryPath)} missing mount export`);
    }
    if (/\bprocess\s*\.\s*env\b/.test(entrySource)) {
      issues.push(`${relative(entryPath)} contains process.env`);
    }
  }
  if (!fs.existsSync(stylePath)) {
    issues.push(`missing packaged style ${relative(stylePath)}`);
  }

  return {
    ok: issues.length === 0,
    sourceRemotePath: relative(sourceRemotePath),
    entryPath: relative(entryPath),
    issues,
  };
}

function verifySharedHostContracts() {
  const checks = [];
  const issues = [];
  for (const item of [
    { name: 'toolbar', filePath: TOOLBAR_PATH, snippets: TOOLBAR_REQUIRED_SNIPPETS },
    { name: 'unavailablePage', filePath: UNAVAILABLE_PAGE_PATH, snippets: UNAVAILABLE_REQUIRED_SNIPPETS },
    { name: 'runtimeHost', filePath: RUNTIME_HOST_PATH, snippets: RUNTIME_HOST_REQUIRED_SNIPPETS },
    ...LOG_REQUIRED_SNIPPETS.map((item) => ({ ...item, name: relative(item.filePath) })),
  ]) {
    const source = readText(item.filePath);
    const itemIssues = [];
    for (const snippet of item.snippets) {
      assertIncludes(itemIssues, relative(item.filePath), source, snippet);
    }
    checks.push({
      name: item.name,
      filePath: relative(item.filePath),
      ok: itemIssues.length === 0,
      issues: itemIssues,
    });
    issues.push(...itemIssues);
  }
  return { ok: issues.length === 0, checks, issues };
}

function main() {
  fs.mkdirSync(path.dirname(REPORT_PATH), { recursive: true });
  const index = readJson(INDEX_PATH);
  const packages = index.packages || [];
  const packageResults = packages.map((pkg) => ({
    id: pkg.id,
    pageShell: verifyPageShell(pkg),
    remoteUi: verifyRemoteUi(pkg),
  }));
  const sharedHost = verifySharedHostContracts();

  const failedPackages = packageResults.filter((item) => !item.pageShell.ok || !item.remoteUi.ok);
  const report = {
    generatedAt: new Date().toISOString(),
    packageCount: packages.length,
    sharedHost,
    packages: packageResults,
    summary: {
      ok: failedPackages.length === 0 && sharedHost.ok,
      failedPackages: failedPackages.map((item) => item.id),
      sharedHostFailed: !sharedHost.ok,
    },
  };
  fs.writeFileSync(REPORT_PATH, `${JSON.stringify(report, null, 2)}\n`);

  for (const item of packageResults) {
    const ok = item.pageShell.ok && item.remoteUi.ok;
    console.log(`${ok ? 'OK' : 'FAIL'} ${item.id} page=${item.pageShell.pagePath || '-'} remote=${item.remoteUi.entryPath || '-'}`);
    for (const issue of [...item.pageShell.issues, ...item.remoteUi.issues]) {
      console.log(`  - ${issue}`);
    }
  }

  if (!sharedHost.ok) {
    console.log('FAIL shared host contracts');
    for (const issue of sharedHost.issues) {
      console.log(`  - ${issue}`);
    }
  } else {
    console.log('OK shared host contracts');
  }

  if (!report.summary.ok) {
    console.error(`Platform UI contract smoke failed; report=${REPORT_PATH}`);
    process.exit(1);
  }
  console.log(`Platform UI contract smoke passed: ${packages.length}/${packages.length}; report=${REPORT_PATH}`);
}

main();
