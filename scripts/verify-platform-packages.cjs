#!/usr/bin/env node

const { execFileSync } = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.resolve(__dirname, '..');
const PACKAGE_JSON_PATH = path.join(ROOT, 'package.json');
const CARGO_TOML_PATH = path.join(ROOT, 'Cargo.toml');
const INDEX_PATH = path.join(ROOT, 'platform-packages', 'index.json');
const DIST_DIR = path.join(ROOT, 'platform-packages', 'dist');
const PLATFORM_UI_DIR = path.join(ROOT, 'src', 'platform-ui');
const BUILD_PLATFORM_UI_SCRIPT_PATH = path.join(ROOT, 'scripts', 'build-platform-ui.cjs');
const PACKAGE_PLATFORM_SCRIPT_PATH = path.join(ROOT, 'scripts', 'package-platform-package.cjs');
const PACKAGE_INDEX_SCRIPT_PATH = path.join(ROOT, 'scripts', 'build-platform-package-index.cjs');
const PLATFORM_PACKAGES_WORKFLOW_PATH = path.join(ROOT, '.github', 'workflows', 'platform-packages.yml');
const STORE_PATH = path.join(ROOT, 'src', 'stores', 'usePlatformPackageStore.ts');
const PAGES_DIR = path.join(ROOT, 'src', 'pages');
const TOOLBAR_PATH = path.join(ROOT, 'src', 'components', 'PlatformPackageToolbar.tsx');
const SERVICE_PATH = path.join(ROOT, 'src', 'services', 'platformPackageService.ts');
const COMMANDS_PATH = path.join(ROOT, 'src-tauri', 'src', 'commands', 'platform_package.rs');
const APP_PATH = path.join(ROOT, 'src', 'App.tsx');
const DASHBOARD_PATH = path.join(ROOT, 'src', 'pages', 'DashboardPage.tsx');
const FLOATING_CARD_PATH = path.join(ROOT, 'src', 'pages', 'FloatingCardWindow.tsx');
const AUTO_REFRESH_PATH = path.join(ROOT, 'src', 'hooks', 'useAutoRefresh.ts');
const ACCOUNT_TRANSFER_PATH = path.join(ROOT, 'src', 'services', 'accountTransferService.ts');
const DATA_TRANSFER_PATH = path.join(ROOT, 'src', 'services', 'dataTransferService.ts');
const SIDE_NAV_PATH = path.join(ROOT, 'src', 'components', 'layout', 'SideNav.tsx');
const PLATFORM_LAYOUT_MODAL_PATH = path.join(ROOT, 'src', 'components', 'PlatformLayoutModal.tsx');
const TRAY_PATH = path.join(ROOT, 'src-tauri', 'src', 'modules', 'tray.rs');
const MACOS_NATIVE_MENU_PATH = path.join(ROOT, 'src-tauri', 'src', 'modules', 'macos_native_menu.rs');
const PROVIDER_TOKEN_KEEPER_PATH = path.join(ROOT, 'src-tauri', 'src', 'modules', 'provider_token_keeper.rs');
const WEB_REPORT_PATH = path.join(ROOT, 'src-tauri', 'src', 'modules', 'web_report.rs');
const PROVIDER_CURRENT_PATH = path.join(ROOT, 'src-tauri', 'src', 'commands', 'provider_current.rs');
const TAURI_SRC_DIR = path.join(ROOT, 'src-tauri', 'src');
const TAURI_MODULES_MOD_PATH = path.join(ROOT, 'src-tauri', 'src', 'modules', 'mod.rs');

const EXPECTED_PLATFORM_PACKAGES = new Map([
  ['antigravity', 'sidecarAdapter'],
  ['antigravity_ide', 'sidecarAdapter'],
  ['claude_manager', 'sidecarAdapter'],
  ['zed', 'sidecarAdapter'],
  ['kiro', 'sidecarAdapter'],
  ['github-copilot', 'sidecarAdapter'],
  ['windsurf', 'sidecarAdapter'],
  ['cursor', 'sidecarAdapter'],
  ['gemini', 'sidecarAdapter'],
  ['trae', 'sidecarAdapter'],
  ['qoder', 'sidecarAdapter'],
  ['codebuddy', 'sidecarAdapter'],
  ['codebuddy_cn', 'sidecarAdapter'],
  ['workbuddy', 'sidecarAdapter'],
  ['codex', 'sidecarAdapter'],
]);

const PLATFORM_CONTENT_COMPONENTS = new Map([
  ['antigravity', 'AntigravityRemoteContent'],
  ['antigravity_ide', 'AntigravityRemoteContent'],
  ['claude_manager', 'ClaudeAccountsContent'],
  ['zed', 'ZedAccountsContent'],
  ['kiro', 'KiroAccountsContent'],
  ['github-copilot', 'GitHubCopilotAccountsContent'],
  ['windsurf', 'WindsurfAccountsContent'],
  ['cursor', 'CursorAccountsContent'],
  ['gemini', 'GeminiAccountsContent'],
  ['trae', 'TraeAccountsContent'],
  ['qoder', 'QoderAccountsContent'],
  ['codebuddy', 'CodebuddyAccountsContent'],
  ['codebuddy_cn', 'CodebuddyCnAccountsContent'],
  ['workbuddy', 'WorkbuddyAccountsContent'],
  ['codex', 'CodexAccountsContent'],
]);

const PLATFORM_RUST_MODULE_PREFIXES = new Map([
  ['antigravity', ['account', 'antigravity']],
  ['antigravity_ide', ['account', 'antigravity']],
  ['claude_manager', ['claude']],
  ['zed', ['zed']],
  ['kiro', ['kiro']],
  ['github-copilot', ['github_copilot']],
  ['windsurf', ['windsurf']],
  ['cursor', ['cursor']],
  ['gemini', ['gemini']],
  ['trae', ['trae']],
  ['qoder', ['qoder']],
  ['codebuddy', ['codebuddy']],
  ['codebuddy_cn', ['codebuddy_cn']],
  ['workbuddy', ['workbuddy']],
  ['codex', ['codex']],
]);

const cliArgs = process.argv.slice(2).map((value) => value.trim()).filter(Boolean);
const strictFullHotUpdate = cliArgs.includes('--strict-full-hot-update');
const requestedIds = new Set(cliArgs.filter((value) => !value.startsWith('--')));
const issues = [];
const rows = [];
const strictNativeBoundaryDetails = [];

function fail(message) {
  issues.push(message);
}

function readJson(filePath, label) {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch (error) {
    fail(`${label}: failed to read JSON: ${error.message}`);
    return null;
  }
}

function readText(filePath, label) {
  try {
    return fs.readFileSync(filePath, 'utf8');
  } catch (error) {
    fail(`${label}: failed to read file: ${error.message}`);
    return '';
  }
}

function relative(filePath) {
  return path.relative(ROOT, filePath);
}

function sha256(filePath) {
  return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

function jsonStable(value) {
  return JSON.stringify(value ?? null);
}

function assertEqual(label, actual, expected) {
  if (actual !== expected) {
    fail(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

function assertJsonEqual(label, actual, expected) {
  if (jsonStable(actual) !== jsonStable(expected)) {
    fail(`${label}: JSON mismatch`);
  }
}

function assertNonEmptyArray(label, value) {
  if (!Array.isArray(value) || value.length === 0) {
    fail(`${label}: expected a non-empty array`);
  }
}

function assertIncludes(label, source, expected) {
  if (!source.includes(expected)) {
    fail(`${label}: missing ${expected}`);
  }
}

function assertIncludesAny(label, source, expectedValues) {
  if (!expectedValues.some((expected) => source.includes(expected))) {
    fail(`${label}: missing one of [${expectedValues.join(', ')}]`);
  }
}

function assertSetEqual(label, actual, expected) {
  const missing = [...expected].filter((value) => !actual.has(value));
  const extra = [...actual].filter((value) => !expected.has(value));
  if (missing.length > 0 || extra.length > 0) {
    fail(`${label}: missing [${missing.join(', ')}], extra [${extra.join(', ')}]`);
  }
}

function platformStringLiterals(platformId) {
  return [`'${platformId}'`, `"${platformId}"`];
}

function assertCanOpenPlatformCall(label, source, platformId) {
  assertIncludesAny(
    label,
    source,
    platformStringLiterals(platformId).map((literal) => `canOpenPlatform(${literal})`),
  );
}

function assertRustPackageGate(label, source, platformId) {
  assertIncludesAny(label, source, [
    `is_platform_package_runtime_ready("${platformId}")`,
    `is_platform_package_installed("${platformId}")`,
  ]);
}

function isAntigravitySuitePackage(packageId) {
  return packageId === 'antigravity' || packageId === 'antigravity_ide';
}

function listFilesRecursive(dir, predicate) {
  const files = [];
  if (!fs.existsSync(dir)) return files;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...listFilesRecursive(fullPath, predicate));
    } else if (!predicate || predicate(fullPath)) {
      files.push(fullPath);
    }
  }
  return files;
}

function lineNumberAt(source, index) {
  return source.slice(0, index).split(/\r?\n/).length;
}

function platformRustPrefixes(indexPackages) {
  const prefixes = new Set();
  for (const pkg of indexPackages) {
    for (const prefix of PLATFORM_RUST_MODULE_PREFIXES.get(pkg.id) ?? []) {
      prefixes.add(prefix);
    }
  }
  return [...prefixes].sort((left, right) => right.length - left.length);
}

function nativeBoundaryDomain(command) {
  const value = String(command || '');
  if (/wakeup/i.test(value)) return 'wakeup';
  if (/session|thread|trash/i.test(value)) return 'sessions';
  if (/oauth|login|verification/i.test(value)) return 'oauth-login';
  if (/gateway|api_service|local_access|provider|model/i.test(value)) return 'gateway-provider';
  if (/instance|launch|runtime/i.test(value)) return 'instances-runtime';
  if (/quota|credit|subscription|referral|invite/i.test(value)) return 'quota-billing';
  if (/switch|profile|config|app_speed|current/i.test(value)) return 'switch-config';
  if (/import|export|batch|file|index_path/i.test(value)) return 'import-export';
  if (/account|tag|note|plan|token|api_key/i.test(value)) return 'accounts';
  return 'other';
}

function recordStrictNativeBoundaryDetails(packageId, nativeBoundaries) {
  if (!strictFullHotUpdate || nativeBoundaries.length === 0) return;
  const grouped = new Map();
  for (const boundary of nativeBoundaries) {
    const domain = nativeBoundaryDomain(boundary);
    const values = grouped.get(domain) ?? [];
    values.push(boundary);
    grouped.set(domain, values);
  }
  strictNativeBoundaryDetails.push({
    packageId,
    grouped: [...grouped.entries()]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([domain, values]) => ({ domain, values: values.sort() })),
  });
}

function sourceInitializer(source, constName, openChar, closeChar) {
  const anchor = `const ${constName}`;
  const start = source.indexOf(anchor);
  if (start < 0) return null;
  return assignmentInitializer(source, constName, start, openChar, closeChar);
}

function assignmentInitializer(source, name, start, openChar, closeChar) {
  const equals = source.indexOf('=', start);
  if (equals < 0) return null;
  const open = source.indexOf(openChar, equals);
  if (open < 0) return null;

  let depth = 0;
  let inString = null;
  let escaped = false;
  for (let index = open; index < source.length; index += 1) {
    const char = source[index];
    if (inString) {
      if (escaped) {
        escaped = false;
      } else if (char === '\\') {
        escaped = true;
      } else if (char === inString) {
        inString = null;
      }
      continue;
    }
    if (char === '\'' || char === '"' || char === '`') {
      inString = char;
      continue;
    }
    if (char === openChar) depth += 1;
    if (char === closeChar) {
      depth -= 1;
      if (depth === 0) {
        return source.slice(open + 1, index);
      }
    }
  }
  return null;
}

function extractStringSetInitializer(source, constName) {
  const initializer = sourceInitializer(source, constName, '[', ']');
  if (initializer == null) {
    fail(`src/stores/usePlatformPackageStore.ts: missing ${constName} initializer`);
    return new Set();
  }

  const values = new Set();
  const pattern = /['"]([^'"]+)['"]/g;
  let match;
  while ((match = pattern.exec(initializer))) {
    values.add(match[1]);
  }
  return values;
}

function extractDefaultPlatformPackages(source) {
  const initializer = sourceInitializer(source, 'DEFAULT_PLATFORM_PACKAGES', '[', ']');
  if (initializer == null) {
    fail('src/stores/usePlatformPackageStore.ts: missing DEFAULT_PLATFORM_PACKAGES initializer');
    return new Map();
  }

  const defaults = new Map();
  const pattern = /platformId:\s*['"]([^'"]+)['"][\s\S]*?packageMode:\s*['"]([^'"]+)['"][\s\S]*?installKind:\s*['"]([^'"]+)['"]/g;
  let match;
  while ((match = pattern.exec(initializer))) {
    defaults.set(match[1], {
      packageMode: match[2],
      installKind: match[3],
    });
  }
  return defaults;
}

function extractWorkspaceMembers() {
  const source = readText(CARGO_TOML_PATH, 'workspace Cargo.toml');
  const start = source.search(/^\s*members\s*=/m);
  const initializer = start < 0 ? null : assignmentInitializer(source, 'members', start, '[', ']');
  if (initializer == null) {
    fail('Cargo.toml: missing workspace members initializer');
    return new Set();
  }

  const members = new Set();
  const pattern = /['"]([^'"]+)['"]/g;
  let match;
  while ((match = pattern.exec(initializer))) {
    members.add(match[1]);
  }
  return members;
}

function cargoPackageName(crateTomlPath) {
  const source = readText(crateTomlPath, relative(crateTomlPath));
  const match = source.match(/^\s*name\s*=\s*['"]([^'"]+)['"]/m);
  return match?.[1] ?? null;
}

function expectedAdapterCrateName(packageId) {
  if (packageId === 'claude_manager') return 'cockpit-claude-adapter';
  return `cockpit-${packageId.replace(/_/g, '-')}-adapter`;
}

function isExecutable(filePath) {
  try {
    return (fs.statSync(filePath).mode & 0o111) !== 0;
  } catch {
    return false;
  }
}

function safeZipNameFromUrl(url) {
  const clean = String(url || '').split('?')[0].split('#')[0];
  const name = path.basename(clean);
  if (!name || name === '.' || name === '..' || !name.endsWith('.zip')) {
    return null;
  }
  return name;
}

function listZipEntries(zipPath) {
  try {
    return new Set(
      execFileSync('unzip', ['-Z1', zipPath], { encoding: 'utf8' })
        .split(/\r?\n/)
        .map((entry) => entry.trim())
        .filter(Boolean),
    );
  } catch (error) {
    fail(`${relative(zipPath)}: failed to list zip entries with unzip -Z1: ${error.message}`);
    return new Set();
  }
}

function readZipEntry(zipPath, entry) {
  try {
    return execFileSync('unzip', ['-p', zipPath, entry], { encoding: 'utf8' });
  } catch (error) {
    fail(`${relative(zipPath)}: failed to read zip entry ${entry}: ${error.message}`);
    return null;
  }
}

function assertZipJsonEntryEqual(zipPath, entry, expected, packageId) {
  const source = readZipEntry(zipPath, entry);
  if (source == null) return;
  try {
    const actual = JSON.parse(source);
    assertJsonEqual(`${packageId}: zip ${entry} vs source`, actual, expected);
  } catch (error) {
    fail(`${packageId}: zip ${entry} is not valid JSON: ${error.message}`);
  }
}

function assertZipHas(zipEntries, packageId, entry) {
  const normalized = String(entry || '').replace(/^\/+/, '');
  if (!normalized) return;
  if (!zipEntries.has(normalized)) {
    fail(`${packageId}: zip missing ${normalized}`);
  }
}

function adapterEntryForArtifact(adapter, artifact) {
  if (!adapter) return null;
  if (artifact.os === 'macos') return adapter.macosEntry || adapter.entry;
  if (artifact.os === 'windows') return adapter.windowsEntry || adapter.entry;
  if (artifact.os === 'linux') return adapter.linuxEntry || adapter.entry;
  return adapter.entry;
}

function verifySidecarAdapterPackage(packageId, manifest, artifacts, workspaceMembers) {
  const expectedCrate = expectedAdapterCrateName(packageId);
  const expectedMember = `crates/${expectedCrate}`;
  if (!workspaceMembers.has(expectedMember)) {
    fail(`${packageId}: workspace is missing sidecar adapter member ${expectedMember}`);
  }

  const crateTomlPath = path.join(ROOT, expectedMember, 'Cargo.toml');
  if (!fs.existsSync(crateTomlPath)) {
    fail(`${packageId}: missing sidecar adapter Cargo.toml at ${relative(crateTomlPath)}`);
  } else {
    assertEqual(`${packageId}: adapter crate package.name`, cargoPackageName(crateTomlPath), expectedCrate);
  }

  const adapter = manifest.adapter;
  if (!adapter) {
    return;
  }
  assertEqual(`${packageId}: adapter.protocol`, adapter.protocol, 'http-json-v1');
  assertNonEmptyArray(`${packageId}: adapter.methods`, adapter.methods);

  for (const artifact of artifacts) {
    const entry = adapterEntryForArtifact(adapter, artifact);
    if (!entry) {
      fail(`${packageId}: missing adapter entry for artifact os=${artifact.os || '<missing>'}`);
      continue;
    }
    const adapterPath = path.join(ROOT, 'platform-packages', packageId, entry);
    if (!fs.existsSync(adapterPath)) {
      fail(`${packageId}: package source is missing adapter ${entry}`);
      continue;
    }
    if ((artifact.os === 'macos' || artifact.os === 'linux') && !isExecutable(adapterPath)) {
      fail(`${packageId}: adapter ${entry} is not executable`);
    }
    if (artifact.os === 'windows' && !entry.endsWith('.exe')) {
      fail(`${packageId}: windows adapter entry should end with .exe`);
    }
  }
}

function verifyChangelog(packageId, manifest, indexPackage) {
  assertNonEmptyArray(`${packageId}: manifest.changelog`, manifest.changelog);
  assertNonEmptyArray(`${packageId}: index.changelog`, indexPackage.changelog);
  assertJsonEqual(`${packageId}: index.changelog vs manifest.changelog`, indexPackage.changelog ?? [], manifest.changelog ?? []);

  for (const [index, entry] of (manifest.changelog ?? []).entries()) {
    if (!entry || typeof entry !== 'object') {
      fail(`${packageId}: changelog[${index}] must be an object`);
      continue;
    }
    if (typeof entry.version !== 'string' || !entry.version.trim()) {
      fail(`${packageId}: changelog[${index}].version is required`);
    }
    assertNonEmptyArray(`${packageId}: changelog[${index}].notes`, entry.notes);
  }
}

function verifyPackageInfo(packageId, manifest, packageRoot) {
  const infoPath = path.join(packageRoot, 'assets', 'package-info.json');
  if (!fs.existsSync(infoPath)) {
    fail(`${packageId}: missing assets/package-info.json`);
    return;
  }
  const info = readJson(infoPath, `${packageId} package-info`);
  if (!info) return;

  const infoPlatformId = info.platformId ?? info.id;
  assertEqual(`${packageId}: package-info platform id`, infoPlatformId, packageId);
  if (info.version != null) {
    assertEqual(`${packageId}: package-info.version`, info.version, manifest.version);
  }
  if (info.packageMode != null) {
    assertEqual(`${packageId}: package-info.packageMode`, info.packageMode, manifest.packageMode);
  }
  if (info.installKind != null) {
    assertEqual(`${packageId}: package-info.installKind`, info.installKind, manifest.installKind);
  }
}

function assertRemoteExport(source, exportName, packageId) {
  const escaped = exportName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const direct = new RegExp(
    `export\\s+(?:async\\s+)?function\\s+${escaped}\\b|export\\s+(?:const|let|var)\\s+${escaped}\\b`,
  ).test(source);
  const named = new RegExp(
    `export\\s*\\{[\\s\\S]*?(?:\\b${escaped}\\b|\\bas\\s+${escaped}\\b)[\\s\\S]*?\\}`,
  ).test(source);
  if (!direct && !named) {
    fail(`${packageId}: ui/remoteEntry.js is missing ${exportName} export`);
  }
}

function assertBrowserRuntimeSource(source, packageId) {
  if (/\bprocess\s*\.\s*env\b/.test(source)) {
    fail(`${packageId}: ui/remoteEntry.js contains process.env`);
  }
}

function assertScopedRemoteStyle(source, packageId) {
  const rulePattern = /(^|})\s*([^{}]+)\{/g;
  const globalElementPattern = /(^|[\s>+~])(?:html|body)(?=$|[\s>+~.#:[{])/;
  const globalRootPattern = /(^|[\s>+~])(?:#root|:root)(?=$|[\s>+~.#:[{])/;
  const universalPattern = /(^|[\s>+~])\*(?=$|[\s>+~.#:[{])/;
  let match;
  while ((match = rulePattern.exec(source))) {
    const selectorText = match[2].trim();
    if (!selectorText || selectorText.startsWith('@')) continue;
    for (const selector of selectorText.split(',')) {
      const normalized = selector.trim();
      if (
        globalElementPattern.test(normalized)
        || globalRootPattern.test(normalized)
        || universalPattern.test(normalized)
      ) {
        fail(`${packageId}: ui/style.css contains global selector "${normalized}"`);
      }
    }
  }
}

function verifyPackage(indexPackage, workspaceMembers) {
  const packageId = indexPackage.id;
  const packageRoot = path.join(ROOT, 'platform-packages', packageId);
  const manifestPath = path.join(packageRoot, 'manifest.json');
  const runtimePath = path.join(packageRoot, 'runtime', 'index.json');

  if (!fs.existsSync(packageRoot)) fail(`${packageId}: missing package dir ${relative(packageRoot)}`);
  if (!fs.existsSync(manifestPath)) fail(`${packageId}: missing manifest.json`);
  if (!fs.existsSync(runtimePath)) fail(`${packageId}: missing runtime/index.json`);
  if (!fs.existsSync(manifestPath) || !fs.existsSync(runtimePath)) return;

  const manifest = readJson(manifestPath, `${packageId} manifest`);
  const runtime = readJson(runtimePath, `${packageId} runtime`);
  if (!manifest || !runtime) return;

  for (const key of [
    'id',
    'platformId',
    'version',
    'apiVersion',
    'minCoreVersion',
    'displayName',
    'entry',
    'packageMode',
    'installKind',
  ]) {
    assertEqual(`${packageId}: manifest.${key} vs index.${key}`, manifest[key], indexPackage[key]);
  }
  assertEqual(`${packageId}: runtime.packageId vs manifest.id`, runtime.packageId, manifest.id);
  assertEqual(`${packageId}: runtime.platformId vs manifest.platformId`, runtime.platformId, manifest.platformId);
  assertEqual(`${packageId}: runtime.apiVersion vs manifest.apiVersion`, runtime.apiVersion, manifest.apiVersion);

  assertJsonEqual(`${packageId}: runtime.adapter vs manifest.adapter`, runtime.adapter ?? null, manifest.adapter ?? null);
  assertJsonEqual(`${packageId}: runtime.ui vs manifest.ui`, runtime.ui ?? null, manifest.ui ?? null);
  assertJsonEqual(`${packageId}: runtime.capabilities vs manifest.capabilities`, runtime.capabilities ?? [], manifest.capabilities ?? []);
  assertJsonEqual(`${packageId}: runtime.contributions vs manifest.contributions`, runtime.contributions ?? {}, manifest.contributions ?? {});
  assertJsonEqual(`${packageId}: index.adapter vs manifest.adapter`, indexPackage.adapter ?? null, manifest.adapter ?? null);
  assertJsonEqual(`${packageId}: index.ui vs manifest.ui`, indexPackage.ui ?? null, manifest.ui ?? null);
  assertJsonEqual(`${packageId}: index.capabilities vs manifest.capabilities`, indexPackage.capabilities ?? [], manifest.capabilities ?? []);
  assertJsonEqual(`${packageId}: index.contributions vs manifest.contributions`, indexPackage.contributions ?? {}, manifest.contributions ?? {});
  verifyChangelog(packageId, manifest, indexPackage);
  verifyPackageInfo(packageId, manifest, packageRoot);

  const nativeBoundaries = manifest.contributions?.nativeBoundaries ?? [];
  if (manifest.installKind === 'sidecarAdapter' && nativeBoundaries.length > 0) {
    fail(`${packageId}: sidecarAdapter package must not keep nativeBoundaries`);
  }
  if (manifest.installKind === 'coreNativeBoundary' && nativeBoundaries.length === 0) {
    fail(`${packageId}: coreNativeBoundary package must declare nativeBoundaries`);
  }
  if (manifest.installKind === 'sidecarAdapter' && !manifest.adapter) {
    fail(`${packageId}: sidecarAdapter package is missing adapter`);
  }
  if (strictFullHotUpdate) {
    if (manifest.installKind !== 'sidecarAdapter') {
      fail(`${packageId}: strict full hot update requires installKind=sidecarAdapter, got ${manifest.installKind}`);
    }
    if (nativeBoundaries.length > 0) {
      fail(`${packageId}: strict full hot update requires empty nativeBoundaries, got ${nativeBoundaries.length}`);
      recordStrictNativeBoundaryDetails(packageId, nativeBoundaries);
    }
  }

  const ui = manifest.ui;
  if (!ui || ui.protocol !== 'react-remote-esm-v1') {
    fail(`${packageId}: ui.protocol must be react-remote-esm-v1`);
  } else {
    const entryPath = path.join(packageRoot, ui.entry || '');
    const stylePath = path.join(packageRoot, ui.style || '');
    if (!fs.existsSync(entryPath)) {
      fail(`${packageId}: missing UI entry ${ui.entry}`);
    } else {
      const source = fs.readFileSync(entryPath, 'utf8');
      assertRemoteExport(source, 'mount', packageId);
      assertBrowserRuntimeSource(source, packageId);
      for (const exportName of ui.exports ?? []) {
        assertRemoteExport(source, exportName, packageId);
      }
    }
    if (ui.style && !fs.existsSync(stylePath)) {
      fail(`${packageId}: missing UI style ${ui.style}`);
    } else if (ui.style) {
      assertScopedRemoteStyle(fs.readFileSync(stylePath, 'utf8'), packageId);
    }
  }

  const artifacts = indexPackage.artifacts ?? [];
  if (artifacts.length === 0) {
    fail(`${packageId}: missing artifacts[]`);
  }
  if (manifest.adapter) {
    verifySidecarAdapterPackage(packageId, manifest, artifacts, workspaceMembers);
  }

  const firstArtifactZipName = safeZipNameFromUrl(artifacts[0]?.downloadUrl || indexPackage.downloadUrl);
  const topZipPath = firstArtifactZipName ? path.join(DIST_DIR, firstArtifactZipName) : null;
  if (!firstArtifactZipName || !topZipPath || !fs.existsSync(topZipPath)) {
    fail(`${packageId}: missing top-level zip for ${firstArtifactZipName || '<invalid url>'}`);
  } else {
    const topZipSize = fs.statSync(topZipPath).size;
    const topZipSha = sha256(topZipPath);
    assertEqual(`${packageId}: downloadSizeBytes`, indexPackage.downloadSizeBytes, topZipSize);
    assertEqual(`${packageId}: sha256`, indexPackage.sha256, topZipSha);
  }

  for (const [artifactIndex, artifact] of artifacts.entries()) {
    const zipName = safeZipNameFromUrl(artifact.downloadUrl);
    if (!zipName) {
      fail(`${packageId}: artifact[${artifactIndex}] has invalid downloadUrl`);
      continue;
    }
    const zipPath = path.join(DIST_DIR, zipName);
    if (!fs.existsSync(zipPath)) {
      fail(`${packageId}: missing artifact zip ${zipName}`);
      continue;
    }
    const zipSize = fs.statSync(zipPath).size;
    const zipSha = sha256(zipPath);
    assertEqual(`${packageId}: artifact[${artifactIndex}].downloadSizeBytes`, artifact.downloadSizeBytes, zipSize);
    assertEqual(`${packageId}: artifact[${artifactIndex}].sha256`, artifact.sha256, zipSha);

    const zipEntries = listZipEntries(zipPath);
    assertZipHas(zipEntries, packageId, 'manifest.json');
    assertZipHas(zipEntries, packageId, 'runtime/index.json');
    assertZipHas(zipEntries, packageId, 'assets/package-info.json');
    assertZipJsonEntryEqual(zipPath, 'manifest.json', manifest, packageId);
    assertZipJsonEntryEqual(zipPath, 'runtime/index.json', runtime, packageId);
    if (ui) {
      assertZipHas(zipEntries, packageId, ui.entry);
      if (ui.style) assertZipHas(zipEntries, packageId, ui.style);
    }
    const adapterEntry = adapterEntryForArtifact(manifest.adapter, artifact);
    if (manifest.adapter) {
      assertZipHas(zipEntries, packageId, adapterEntry);
    }
  }

  rows.push({
    id: packageId,
    installKind: manifest.installKind,
    version: manifest.version,
    nativeBoundaries: nativeBoundaries.length,
    artifacts: artifacts.length,
    zip: firstArtifactZipName,
  });
}

function verifyHostPlatformPackageStore(indexPackages) {
  if (!fs.existsSync(STORE_PATH)) {
    fail(`missing host platform package store ${relative(STORE_PATH)}`);
    return;
  }
  const source = fs.readFileSync(STORE_PATH, 'utf8');
  const indexIds = new Set(indexPackages.map((pkg) => pkg.id));
  const runtimeManagedIds = extractStringSetInitializer(source, 'RUNTIME_MANAGED_PLATFORM_IDS');
  const defaultPackages = extractDefaultPlatformPackages(source);
  const defaultIds = new Set(defaultPackages.keys());

  assertSetEqual('RUNTIME_MANAGED_PLATFORM_IDS vs platform-packages/index.json', runtimeManagedIds, indexIds);
  assertSetEqual('DEFAULT_PLATFORM_PACKAGES vs platform-packages/index.json', defaultIds, indexIds);

  for (const pkg of indexPackages) {
    const defaultPackage = defaultPackages.get(pkg.id);
    if (!defaultPackage) continue;
    assertEqual(`${pkg.id}: DEFAULT_PLATFORM_PACKAGES.packageMode`, defaultPackage.packageMode, pkg.packageMode);
    assertEqual(`${pkg.id}: DEFAULT_PLATFORM_PACKAGES.installKind`, defaultPackage.installKind, pkg.installKind);
  }
}

function verifyExpectedPlatformPackageSet(indexPackages) {
  const actualIds = new Set(indexPackages.map((pkg) => pkg.id));
  assertSetEqual(
    'platform-packages/index.json expected platform package ids',
    actualIds,
    new Set(EXPECTED_PLATFORM_PACKAGES.keys()),
  );

  for (const pkg of indexPackages) {
    const expectedInstallKind = EXPECTED_PLATFORM_PACKAGES.get(pkg.id);
    if (!expectedInstallKind) continue;
    assertEqual(`${pkg.id}: packageMode`, pkg.packageMode, 'hotUpdate');
    assertEqual(`${pkg.id}: installKind`, pkg.installKind, expectedInstallKind);
  }
}

function listAccountPageShells() {
  if (!fs.existsSync(PAGES_DIR)) {
    fail(`missing pages dir ${relative(PAGES_DIR)}`);
    return [];
  }
  return fs.readdirSync(PAGES_DIR)
    .filter((name) => name.endsWith('AccountsPage.tsx') && name !== 'AccountsPage.tsx')
    .map((name) => ({
      name,
      path: path.join(PAGES_DIR, name),
      source: fs.readFileSync(path.join(PAGES_DIR, name), 'utf8'),
    }));
}

function verifyHostPlatformPages(indexPackages) {
  const pageShells = listAccountPageShells();
  for (const pkg of indexPackages) {
    if (isAntigravitySuitePackage(pkg.id)) {
      const pagePath = path.join(PAGES_DIR, 'AntigravitySuitePage.tsx');
      const source = readText(pagePath, relative(pagePath));
      const label = `${pkg.id}: ${relative(pagePath)}`;
      for (const required of [
        'PlatformPackageToolbar',
        'PlatformPackageUnavailablePage',
        'PlatformRuntimePageHost',
        'usePlatformPackageStore',
        'platformPackage.runtimeReady',
        '<PlatformRuntimePageHost',
        '<PlatformPackageUnavailablePage',
        '<PlatformPackageToolbar',
      ]) {
        if (!source.includes(required)) {
          fail(`${label} is missing ${required}`);
        }
      }
      continue;
    }

    const pages = pageShells.filter((page) => (
      page.source.includes(`'${pkg.id}'`) || page.source.includes(`"${pkg.id}"`)
    ));
    if (pages.length === 0) {
      fail(`${pkg.id}: no *AccountsPage.tsx shell references this platform id`);
      continue;
    }
    for (const page of pages) {
      const label = `${pkg.id}: ${relative(page.path)}`;
      for (const required of [
        'PlatformPackageToolbar',
        'PlatformPackageUnavailablePage',
        'PlatformRuntimePageHost',
        'usePlatformPackageStore',
        'platformPackage.runtimeReady',
        '<PlatformRuntimePageHost',
        '<PlatformPackageUnavailablePage',
        '<PlatformPackageToolbar',
      ]) {
        if (!page.source.includes(required)) {
          fail(`${label} is missing ${required}`);
        }
      }
    }
  }
}

function verifyRemoteUiSourceReuse(indexPackages) {
  for (const pkg of indexPackages) {
    if (isAntigravitySuitePackage(pkg.id)) {
      const remotePath = path.join(PLATFORM_UI_DIR, pkg.id, 'remote.tsx');
      const sharedPath = path.join(PLATFORM_UI_DIR, 'antigravity', 'shared.tsx');
      const remoteSource = readText(remotePath, relative(remotePath));
      const sharedSource = readText(sharedPath, relative(sharedPath));

      assertIncludes(relative(remotePath), remoteSource, 'mountAntigravityRemote');
      assertIncludes(relative(remotePath), remoteSource, 'unmountAntigravityRemote');
      assertIncludes(relative(remotePath), remoteSource, 'export async function mount');
      assertIncludes(relative(remotePath), remoteSource, 'export function unmount');

      for (const expected of [
        '../../pages/AccountsPage',
        '../../pages/InstancesPage',
        '../../pages/WakeupTasksPage',
        '../../pages/WakeupVerificationPage',
        'AntigravityRemoteContent',
        '<AccountsPage hideHeader',
        '<InstancesPage hideHeader',
        '<WakeupTasksPage hideHeader',
        '<WakeupVerificationPage hideHeader',
      ]) {
        assertIncludes(relative(sharedPath), sharedSource, expected);
      }
      continue;
    }

    const componentName = PLATFORM_CONTENT_COMPONENTS.get(pkg.id);
    if (!componentName) {
      fail(`${pkg.id}: missing expected remote content component mapping`);
      continue;
    }

    const remotePath = path.join(PLATFORM_UI_DIR, pkg.id, 'remote.tsx');
    if (!fs.existsSync(remotePath)) {
      fail(`${pkg.id}: missing remote source ${relative(remotePath)}`);
      continue;
    }

    const source = readText(remotePath, relative(remotePath));
    assertIncludes(relative(remotePath), source, `../../pages/${componentName}`);
    assertIncludes(relative(remotePath), source, `<${componentName}`);
    assertIncludes(relative(remotePath), source, 'export async function mount');
    assertIncludes(relative(remotePath), source, 'export function unmount');
  }
}

function verifyHostLifecycleControls() {
  const toolbar = readText(TOOLBAR_PATH, relative(TOOLBAR_PATH));
  const service = readText(SERVICE_PATH, relative(SERVICE_PATH));
  const commands = readText(COMMANDS_PATH, relative(COMMANDS_PATH));

  for (const expected of [
    'checkUpdate',
    'installPackage',
    'updatePackage',
    'uninstallPackage',
    'showChangelog',
    'showUpdateDialog',
    "confirmAction('install')",
    "confirmAction('uninstall')",
    'formatPlatformPackageSize',
    'packageChangelogTitle',
    'packageCheckUpdate',
    'packageDownload',
    'packageUpdate',
    'packageUninstall',
    'showModal',
  ]) {
    assertIncludes(relative(TOOLBAR_PATH), toolbar, expected);
  }

  for (const expected of [
    "invoke('list_platform_packages')",
    "invoke('check_platform_package_update'",
    "invoke('install_platform_package'",
    "invoke('update_platform_package'",
    "invoke('uninstall_platform_package'",
    "invoke('get_platform_package_ui_entry'",
  ]) {
    assertIncludes(relative(SERVICE_PATH), service, expected);
  }

  for (const expected of [
    'pub fn list_platform_packages',
    'pub fn check_platform_package_update',
    'pub fn install_platform_package',
    'pub fn update_platform_package',
    'pub fn uninstall_platform_package',
    'pub fn get_platform_package_ui_entry',
    'platform_package::list_platform_packages',
    'platform_package::check_platform_package_update',
    'platform_package::install_platform_package',
    'platform_package::update_platform_package',
    'platform_package::uninstall_platform_package',
    'platform_package::get_platform_package_ui_entry',
  ]) {
    assertIncludes(relative(COMMANDS_PATH), commands, expected);
  }
}

function verifyPackagingTooling() {
  const packageJson = readJson(PACKAGE_JSON_PATH, relative(PACKAGE_JSON_PATH));
  if (packageJson) {
    const scripts = packageJson.scripts ?? {};
    assertEqual('package.json scripts.build:platform-ui', scripts['build:platform-ui'], 'node scripts/build-platform-ui.cjs');
    assertEqual('package.json scripts.package:platform', scripts['package:platform'], 'node scripts/package-platform-package.cjs');
    assertEqual('package.json scripts.package:platform-index', scripts['package:platform-index'], 'node scripts/build-platform-package-index.cjs');
    assertEqual('package.json scripts.verify:platform-packages', scripts['verify:platform-packages'], 'node scripts/verify-platform-packages.cjs');
    assertEqual(
      'package.json scripts.audit:platform-full-hot-update',
      scripts['audit:platform-full-hot-update'],
      'node scripts/verify-platform-packages.cjs --strict-full-hot-update',
    );
  }

  for (const scriptPath of [
    BUILD_PLATFORM_UI_SCRIPT_PATH,
    PACKAGE_PLATFORM_SCRIPT_PATH,
    PACKAGE_INDEX_SCRIPT_PATH,
  ]) {
    if (!fs.existsSync(scriptPath)) {
      fail(`missing platform package tooling script ${relative(scriptPath)}`);
    }
  }

  const packageScript = readText(PACKAGE_PLATFORM_SCRIPT_PATH, relative(PACKAGE_PLATFORM_SCRIPT_PATH));
  for (const expected of [
    '--platform <id>',
    '--os <macos|windows|linux>',
    '--arch <aarch64|x86_64>',
    '--adapter-bin-dir <path>',
    '--metadata-out <path>',
    '--update-index',
  ]) {
    assertIncludes(relative(PACKAGE_PLATFORM_SCRIPT_PATH), packageScript, expected);
  }

  const packageIndexScript = readText(PACKAGE_INDEX_SCRIPT_PATH, relative(PACKAGE_INDEX_SCRIPT_PATH));
  for (const expected of [
    '--metadata-dir <path>',
    '--download-base-url <url>',
    '--require-os-arch <list>',
    '--verify-zip-dir <path>',
    'macos/aarch64',
    'macos/x86_64',
    'linux/x86_64',
    'linux/aarch64',
    'windows/x86_64',
  ]) {
    assertIncludes(relative(PACKAGE_INDEX_SCRIPT_PATH), packageIndexScript, expected);
  }

  const workflow = readText(PLATFORM_PACKAGES_WORKFLOW_PATH, relative(PLATFORM_PACKAGES_WORKFLOW_PATH));
  for (const expected of [
    'name: Platform Packages',
    'npm run verify:platform-packages',
    'scripts/build-platform-ui.cjs',
    'scripts/package-platform-package.cjs',
    'scripts/build-platform-package-index.cjs',
    'macos-aarch64',
    'macos-x86_64',
    'linux-x86_64',
    'linux-aarch64',
    'windows-x86_64',
    'actions/upload-artifact',
    'actions/download-artifact',
    'package:platform-index',
    '--require-os-arch',
    '--verify-zip-dir',
  ]) {
    assertIncludes(relative(PLATFORM_PACKAGES_WORKFLOW_PATH), workflow, expected);
  }
}

function verifyHostHiddenEntryGates(indexPackages) {
  const platformIds = indexPackages.map((pkg) => pkg.id);

  const app = readText(APP_PATH, relative(APP_PATH));
  const dashboard = readText(DASHBOARD_PATH, relative(DASHBOARD_PATH));
  const autoRefresh = readText(AUTO_REFRESH_PATH, relative(AUTO_REFRESH_PATH));
  const accountTransfer = readText(ACCOUNT_TRANSFER_PATH, relative(ACCOUNT_TRANSFER_PATH));
  const dataTransfer = readText(DATA_TRANSFER_PATH, relative(DATA_TRANSFER_PATH));
  const sideNav = readText(SIDE_NAV_PATH, relative(SIDE_NAV_PATH));
  const platformLayoutModal = readText(PLATFORM_LAYOUT_MODAL_PATH, relative(PLATFORM_LAYOUT_MODAL_PATH));
  const floatingCard = readText(FLOATING_CARD_PATH, relative(FLOATING_CARD_PATH));
  const tray = readText(TRAY_PATH, relative(TRAY_PATH));
  const macosNativeMenu = readText(MACOS_NATIVE_MENU_PATH, relative(MACOS_NATIVE_MENU_PATH));
  const providerTokenKeeper = readText(PROVIDER_TOKEN_KEEPER_PATH, relative(PROVIDER_TOKEN_KEEPER_PATH));
  const webReport = readText(WEB_REPORT_PATH, relative(WEB_REPORT_PATH));
  const providerCurrent = readText(PROVIDER_CURRENT_PATH, relative(PROVIDER_CURRENT_PATH));

  for (const required of [
    'canOpenPlatformFromPackages',
    'canShowPlatformEntryFromPackages',
    'getPlatformPackageShortStatus',
    'usePlatformPackageStore',
  ]) {
    assertIncludes(relative(DASHBOARD_PATH), dashboard, required);
  }

  for (const platformId of platformIds) {
    assertCanOpenPlatformCall(relative(APP_PATH), app, platformId);
    assertCanOpenPlatformCall(relative(DASHBOARD_PATH), dashboard, platformId);
    assertCanOpenPlatformCall(relative(AUTO_REFRESH_PATH), autoRefresh, platformId);
  }

  for (const required of [
    'canShowPlatformEntryFromPackages',
    'getPlatformPackageShortStatus',
    'usePlatformPackageStore',
  ]) {
    assertIncludes(relative(SIDE_NAV_PATH), sideNav, required);
  }

  for (const required of [
    'usePlatformPackageStore',
    'isPackageUnavailable',
    'runtimeReady',
    'refreshPlatformPackages',
  ]) {
    assertIncludes(relative(PLATFORM_LAYOUT_MODAL_PATH), platformLayoutModal, required);
  }

  for (const required of [
    'isRuntimeManagedPlatform',
    'canUseAccountTransferPlatform',
    'usePlatformPackageStore.getState().canOpenPlatform(platform)',
  ]) {
    assertIncludes(relative(ACCOUNT_TRANSFER_PATH), accountTransfer, required);
  }

  assertIncludes(relative(DATA_TRANSFER_PATH), dataTransfer, 'canUseAccountTransferPlatform');
  assertIncludes(relative(DATA_TRANSFER_PATH), dataTransfer, 'canUseAntigravitySeriesTransfer');
  assertIncludes(relative(DATA_TRANSFER_PATH), dataTransfer, "'antigravity'");
  assertIncludes(relative(DATA_TRANSFER_PATH), dataTransfer, "'antigravity_ide'");
  assertIncludes(relative(DATA_TRANSFER_PATH), dataTransfer, "'codex'");

  for (const platformId of [
    'antigravity',
    'antigravity_ide',
    'codex',
    'claude_manager',
    'codebuddy',
    'codebuddy_cn',
    'qoder',
    'workbuddy',
  ]) {
    assertCanOpenPlatformCall(relative(FLOATING_CARD_PATH), floatingCard, platformId);
  }

  for (const required of [
    'pub(crate) fn runtime_ready(self) -> bool',
    'is_antigravity_series_runtime_ready()',
    'is_platform_package_runtime_ready(self.as_str())',
  ]) {
    assertIncludes(relative(TRAY_PATH), tray, required);
  }

  for (const required of [
    'let runtime_ready = platform.runtime_ready()',
    'if runtime_ready {',
    'if !platform.runtime_ready()',
  ]) {
    assertIncludes(relative(MACOS_NATIVE_MENU_PATH), macosNativeMenu, required);
  }

  for (const platformId of [
    'codex',
    'cursor',
    'gemini',
    'github-copilot',
    'windsurf',
    'codebuddy',
    'codebuddy_cn',
    'workbuddy',
    'trae',
  ]) {
    assertRustPackageGate(relative(PROVIDER_TOKEN_KEEPER_PATH), providerTokenKeeper, platformId);
  }

  for (const platformId of [
    'antigravity',
    'antigravity_ide',
    'codex',
    'windsurf',
    'cursor',
    'gemini',
    'codebuddy',
    'codebuddy_cn',
    'qoder',
    'trae',
    'workbuddy',
  ]) {
    if (isAntigravitySuitePackage(platformId)) {
      assertIncludes(relative(WEB_REPORT_PATH), webReport, 'is_antigravity_series_runtime_ready()');
    } else {
      assertRustPackageGate(relative(WEB_REPORT_PATH), webReport, platformId);
    }
  }

  for (const platformId of [
    'codebuddy',
    'codebuddy_cn',
    'qoder',
    'trae',
    'workbuddy',
  ]) {
    assertRustPackageGate(relative(PROVIDER_CURRENT_PATH), providerCurrent, platformId);
  }
}

function verifyStrictHostPlatformBusinessResiduals(indexPackages) {
  if (!strictFullHotUpdate) return;

  const prefixes = platformRustPrefixes(indexPackages);
  if (prefixes.length === 0) return;

  const moduleDeclarationPattern = new RegExp(
    `^\\s*pub\\s+mod\\s+(${prefixes.map((prefix) => prefix.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')})(?:_[A-Za-z0-9_]+)?\\s*;`,
    'gm',
  );
  const moduleSource = readText(TAURI_MODULES_MOD_PATH, relative(TAURI_MODULES_MOD_PATH));
  let declarationMatch;
  while ((declarationMatch = moduleDeclarationPattern.exec(moduleSource))) {
    fail(
      `${relative(TAURI_MODULES_MOD_PATH)}:${lineNumberAt(moduleSource, declarationMatch.index)}: strict full hot update forbids compiling platform business module "${declarationMatch[0].trim()}" into Core Shell`,
    );
  }

  const directModulePattern = new RegExp(
    `(?:crate::)?modules::(${prefixes.map((prefix) => prefix.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')})(?:_[A-Za-z0-9_]+)?::`,
    'g',
  );
  const hostFiles = listFilesRecursive(TAURI_SRC_DIR, (filePath) => {
    if (!filePath.endsWith('.rs')) return false;
    const rel = relative(filePath);
    if (rel === relative(TAURI_MODULES_MOD_PATH)) return false;
    if (/^src-tauri\/src\/modules\/(codebuddy|codebuddy_cn|codex|cursor|gemini|github_copilot|kiro|qoder|trae|windsurf|workbuddy|claude|zed)(?:_[^/]*)?\.rs$/.test(rel)) {
      return false;
    }
    return true;
  });

  for (const filePath of hostFiles) {
    const source = readText(filePath, relative(filePath));
    let match;
    while ((match = directModulePattern.exec(source))) {
      fail(
        `${relative(filePath)}:${lineNumberAt(source, match.index)}: strict full hot update forbids direct platform business call "${match[0]}"; use platform_adapter or move the logic into the package adapter`,
      );
    }
  }
}

function main() {
  const index = readJson(INDEX_PATH, 'platform package index');
  if (!index) process.exit(1);
  const packages = (index.packages ?? []).filter((pkg) => requestedIds.size === 0 || requestedIds.has(pkg.id));
  if (requestedIds.size > 0) {
    for (const id of requestedIds) {
      if (!packages.some((pkg) => pkg.id === id)) {
        fail(`requested package not found in index: ${id}`);
      }
    }
  }
  if (requestedIds.size === 0) {
    verifyExpectedPlatformPackageSet(packages);
    verifyPackagingTooling();
    verifyHostPlatformPackageStore(packages);
    verifyHostPlatformPages(packages);
    verifyRemoteUiSourceReuse(packages);
    verifyHostLifecycleControls();
    verifyHostHiddenEntryGates(packages);
    verifyStrictHostPlatformBusinessResiduals(packages);
  }

  const workspaceMembers = extractWorkspaceMembers();
  for (const pkg of packages) {
    verifyPackage(pkg, workspaceMembers);
  }

  console.table(rows);
  if (strictNativeBoundaryDetails.length > 0) {
    console.error('\nStrict full hot update native boundary details:');
    for (const detail of strictNativeBoundaryDetails) {
      console.error(`\n${detail.packageId}:`);
      for (const group of detail.grouped) {
        console.error(`  ${group.domain} (${group.values.length})`);
        for (const value of group.values) {
          console.error(`    - ${value}`);
        }
      }
    }
  }
  if (issues.length > 0) {
    console.error('\nPlatform package verification failed:');
    for (const issue of issues) {
      console.error(`- ${issue}`);
    }
    process.exit(1);
  }
  console.log(`Verified ${rows.length} platform package${rows.length === 1 ? '' : 's'}.`);
}

main();
