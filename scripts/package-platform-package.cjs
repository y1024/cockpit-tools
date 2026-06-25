#!/usr/bin/env node

const { execFileSync } = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.resolve(__dirname, '..');
const INDEX_PATH = path.join(ROOT, 'platform-packages', 'index.json');
const DEFAULT_DIST_DIR = path.join(ROOT, 'platform-packages', 'dist');

function fail(message) {
  console.error(message);
  process.exit(1);
}

function usage() {
  console.log(`Usage:
  node scripts/package-platform-package.cjs --platform <id> [options]

Options:
  --os <macos|windows|linux>          Target platform OS. Defaults to current OS.
  --arch <aarch64|x86_64>             Target platform arch. Defaults to current arch.
  --adapter-bin-dir <path>            Directory containing built adapter binary.
  --dist-dir <path>                   Output directory. Defaults to platform-packages/dist.
  --filename-template <legacy|os-arch> Zip name mode. Defaults to legacy.
  --metadata-out <path>               Write artifact metadata JSON.
  --download-url <url>                Override metadata downloadUrl.
  --update-index                      Update platform-packages/index.json for this artifact.
`);
}

function parseArgs(argv) {
  const args = {
    distDir: DEFAULT_DIST_DIR,
    filenameTemplate: 'legacy',
    updateIndex: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--help' || arg === '-h') {
      usage();
      process.exit(0);
    }
    if (arg === '--update-index') {
      args.updateIndex = true;
      continue;
    }
    const next = argv[index + 1];
    if (!next || next.startsWith('--')) {
      fail(`Missing value for ${arg}`);
    }
    index += 1;
    if (arg === '--platform') args.platformId = next;
    else if (arg === '--os') args.os = normalizeOs(next);
    else if (arg === '--arch') args.arch = normalizeArch(next);
    else if (arg === '--adapter-bin-dir') args.adapterBinDir = path.resolve(ROOT, next);
    else if (arg === '--dist-dir') args.distDir = path.resolve(ROOT, next);
    else if (arg === '--filename-template') args.filenameTemplate = next;
    else if (arg === '--metadata-out') args.metadataOut = path.resolve(ROOT, next);
    else if (arg === '--download-url') args.downloadUrl = next;
    else fail(`Unknown argument: ${arg}`);
  }

  if (!args.platformId) fail('Missing --platform <id>');
  args.os = normalizeOs(args.os || process.platform);
  args.arch = normalizeArch(args.arch || process.arch);
  if (!['legacy', 'os-arch'].includes(args.filenameTemplate)) {
    fail('--filename-template must be legacy or os-arch');
  }
  return args;
}

function normalizeOs(value) {
  if (value === 'darwin') return 'macos';
  if (value === 'win32') return 'windows';
  if (value === 'macos' || value === 'windows' || value === 'linux') return value;
  fail(`Unsupported OS: ${value}`);
}

function normalizeArch(value) {
  if (value === 'arm64') return 'aarch64';
  if (value === 'x64') return 'x86_64';
  if (value === 'aarch64' || value === 'x86_64') return value;
  fail(`Unsupported arch: ${value}`);
}

function readJson(filePath, label) {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch (error) {
    fail(`${label}: failed to read JSON: ${error.message}`);
  }
}

function safeRelativePath(relativePath, label) {
  if (!relativePath || typeof relativePath !== 'string') {
    fail(`${label}: path is required`);
  }
  if (path.isAbsolute(relativePath) || relativePath.includes('\0')) {
    fail(`${label}: unsafe path ${relativePath}`);
  }
  const normalized = path.normalize(relativePath);
  if (normalized === '..' || normalized.startsWith(`..${path.sep}`)) {
    fail(`${label}: unsafe path ${relativePath}`);
  }
  return normalized;
}

function sha256(filePath) {
  return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

function displayPath(filePath) {
  const relativePath = path.relative(ROOT, filePath);
  if (relativePath === '' || (!relativePath.startsWith('..') && !path.isAbsolute(relativePath))) {
    return relativePath;
  }
  return filePath;
}

function adapterEntryForOs(adapter, os) {
  if (!adapter) return null;
  if (os === 'macos') return adapter.macosEntry || adapter.entry;
  if (os === 'windows') return adapter.windowsEntry || adapter.entry;
  if (os === 'linux') return adapter.linuxEntry || adapter.entry;
  return adapter.entry;
}

function expectedAdapterCrateName(platformId) {
  if (platformId === 'claude_manager') return 'cockpit-claude-adapter';
  return `cockpit-${platformId.replace(/_/g, '-')}-adapter`;
}

function rustTargetFor(os, arch) {
  if (os === 'macos') return `${arch}-apple-darwin`;
  if (os === 'windows') return `${arch}-pc-windows-msvc`;
  if (os === 'linux') return `${arch}-unknown-linux-gnu`;
  fail(`Unsupported OS for Rust target: ${os}`);
}

function assertFile(filePath, label) {
  if (!fs.existsSync(filePath) || !fs.statSync(filePath).isFile()) {
    fail(`${label}: missing file ${path.relative(ROOT, filePath)}`);
  }
}

function assertDir(filePath, label) {
  if (!fs.existsSync(filePath) || !fs.statSync(filePath).isDirectory()) {
    fail(`${label}: missing directory ${path.relative(ROOT, filePath)}`);
  }
}

function copyAdapterIfAvailable(packageRoot, manifest, os, adapterBinDir) {
  if (!manifest.adapter) {
    if (manifest.installKind === 'sidecarAdapter') {
      fail(`${manifest.id}: sidecarAdapter package is missing adapter`);
    }
    return null;
  }

  const entry = safeRelativePath(adapterEntryForOs(manifest.adapter, os), `${manifest.id}: adapter entry`);
  const targetPath = path.join(packageRoot, entry);
  const targetBasename = path.basename(entry);

  if (adapterBinDir) {
    const sourcePath = path.join(adapterBinDir, targetBasename);
    if (!fs.existsSync(sourcePath)) {
      fail(`${manifest.id}: built adapter not found at ${path.relative(ROOT, sourcePath)}`);
    }
    fs.mkdirSync(path.dirname(targetPath), { recursive: true });
    fs.copyFileSync(sourcePath, targetPath);
  }

  assertFile(targetPath, `${manifest.id}: adapter`);
  if (os !== 'windows') {
    fs.chmodSync(targetPath, 0o755);
  }
  return entry;
}

function copyCodexRuntimeHelpers(packageRoot, manifest, os, arch) {
  if (manifest.id !== 'codex') return [];

  const adapterEntry = safeRelativePath(adapterEntryForOs(manifest.adapter, os), 'codex: adapter entry');
  const adapterDir = path.dirname(path.join(packageRoot, adapterEntry));
  const extension = os === 'windows' ? '.exe' : '';
  const sourcePath = path.join(
    ROOT,
    'sidecars',
    'cockpit-cliproxy',
    'bin',
    `cockpit-cliproxy-${rustTargetFor(os, arch)}${extension}`,
  );
  const targetPath = path.join(adapterDir, `cockpit-cliproxy${extension}`);

  assertFile(sourcePath, 'codex: cockpit-cliproxy helper');
  fs.mkdirSync(adapterDir, { recursive: true });
  fs.copyFileSync(sourcePath, targetPath);
  if (os !== 'windows') {
    fs.chmodSync(targetPath, 0o755);
  }
  return [path.relative(packageRoot, targetPath)];
}

function createZip(packageRoot, zipPath) {
  fs.rmSync(zipPath, { force: true });
  fs.mkdirSync(path.dirname(zipPath), { recursive: true });

  if (process.platform === 'win32') {
    execFileSync('powershell', [
      '-NoProfile',
      '-NonInteractive',
      '-Command',
      '$ErrorActionPreference = "Stop"; Compress-Archive -Path * -DestinationPath $env:ZIP_PATH -Force',
    ], {
      cwd: packageRoot,
      env: { ...process.env, ZIP_PATH: zipPath },
      stdio: 'inherit',
    });
    return;
  }

  execFileSync('zip', ['-qr', zipPath, '.', '-x', '**/.DS_Store'], {
    cwd: packageRoot,
    stdio: 'inherit',
  });
}

function zipNameFor(platformId, version, os, arch, template) {
  if (template === 'os-arch') return `${platformId}-${version}-${os}-${arch}.zip`;
  return `${platformId}-${version}.zip`;
}

function replaceZipName(downloadUrl, zipName) {
  if (!downloadUrl || typeof downloadUrl !== 'string') return `platform-packages/dist/${zipName}`;
  const clean = downloadUrl.split('?')[0].split('#')[0];
  const index = clean.lastIndexOf('/');
  if (index < 0) return zipName;
  return `${downloadUrl.slice(0, index + 1)}${zipName}`;
}

function updateIndex(index, platformId, os, arch, metadata, manifest) {
  const pkg = (index.packages || []).find((item) => item.id === platformId);
  if (!pkg) fail(`${platformId}: missing from platform-packages/index.json`);

  for (const key of [
    'platformId',
    'version',
    'apiVersion',
    'minCoreVersion',
    'displayName',
    'entry',
    'packageMode',
    'installKind',
    'adapter',
    'ui',
    'capabilities',
    'changelog',
    'contributions',
  ]) {
    if (Object.prototype.hasOwnProperty.call(manifest, key)) {
      pkg[key] = manifest[key];
    }
  }

  if (!Array.isArray(pkg.artifacts)) pkg.artifacts = [];
  const artifactIndex = pkg.artifacts.findIndex((artifact) => artifact.os === os && artifact.arch === arch);
  const artifact = {
    os,
    arch,
    downloadUrl: metadata.downloadUrl,
    downloadSizeBytes: metadata.downloadSizeBytes,
    sha256: metadata.sha256,
  };
  if (artifactIndex >= 0) pkg.artifacts[artifactIndex] = artifact;
  else pkg.artifacts.push(artifact);

  if (artifactIndex === 0 || (artifactIndex < 0 && pkg.artifacts.length === 1)) {
    pkg.downloadUrl = artifact.downloadUrl;
    pkg.downloadSizeBytes = artifact.downloadSizeBytes;
    pkg.sha256 = artifact.sha256;
  }
  fs.writeFileSync(INDEX_PATH, `${JSON.stringify(index, null, 2)}\n`);
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const index = readJson(INDEX_PATH, 'platform package index');
  const indexPackage = (index.packages || []).find((pkg) => pkg.id === args.platformId);
  if (!indexPackage) fail(`Unknown platform package: ${args.platformId}`);

  const packageRoot = path.join(ROOT, 'platform-packages', args.platformId);
  const manifestPath = path.join(packageRoot, 'manifest.json');
  const runtimePath = path.join(packageRoot, 'runtime', 'index.json');
  const infoPath = path.join(packageRoot, 'assets', 'package-info.json');
  assertDir(packageRoot, args.platformId);
  assertFile(manifestPath, `${args.platformId}: manifest`);
  assertFile(runtimePath, `${args.platformId}: runtime`);
  assertFile(infoPath, `${args.platformId}: package-info`);

  const manifest = readJson(manifestPath, `${args.platformId} manifest`);
  const runtime = readJson(runtimePath, `${args.platformId} runtime`);
  if (manifest.id !== args.platformId || runtime.packageId !== args.platformId) {
    fail(`${args.platformId}: manifest/runtime id mismatch`);
  }
  if (manifest.version !== indexPackage.version) {
    fail(`${args.platformId}: manifest version does not match index version`);
  }

  const ui = manifest.ui || {};
  if (ui.protocol !== 'react-remote-esm-v1') fail(`${args.platformId}: ui.protocol must be react-remote-esm-v1`);
  assertFile(path.join(packageRoot, safeRelativePath(ui.entry, `${args.platformId}: ui.entry`)), `${args.platformId}: UI entry`);
  if (ui.style) {
    assertFile(path.join(packageRoot, safeRelativePath(ui.style, `${args.platformId}: ui.style`)), `${args.platformId}: UI style`);
  }

  const adapterEntry = copyAdapterIfAvailable(packageRoot, manifest, args.os, args.adapterBinDir);
  const helperEntries = copyCodexRuntimeHelpers(packageRoot, manifest, args.os, args.arch);
  if (manifest.adapter) {
    const cratePath = path.join(ROOT, 'crates', expectedAdapterCrateName(args.platformId), 'Cargo.toml');
    assertFile(cratePath, `${args.platformId}: adapter crate`);
  }

  const zipName = zipNameFor(args.platformId, manifest.version, args.os, args.arch, args.filenameTemplate);
  const zipPath = path.join(args.distDir, zipName);
  createZip(packageRoot, zipPath);

  const size = fs.statSync(zipPath).size;
  const checksum = sha256(zipPath);
  const firstArtifact = (indexPackage.artifacts || [])[0] || {};
  const downloadUrl = args.downloadUrl || replaceZipName(firstArtifact.downloadUrl || indexPackage.downloadUrl, zipName);
  const metadata = {
    id: args.platformId,
    platformId: manifest.platformId,
    version: manifest.version,
    packageMode: manifest.packageMode,
    installKind: manifest.installKind,
    os: args.os,
    arch: args.arch,
    zipName,
    zipPath: displayPath(zipPath),
    downloadUrl,
    downloadSizeBytes: size,
    sha256: checksum,
    adapterEntry,
    helperEntries,
    uiEntry: ui.entry,
    uiStyle: ui.style || null,
  };

  if (args.updateIndex) updateIndex(index, args.platformId, args.os, args.arch, metadata, manifest);
  if (args.metadataOut) {
    fs.mkdirSync(path.dirname(args.metadataOut), { recursive: true });
    fs.writeFileSync(args.metadataOut, `${JSON.stringify(metadata, null, 2)}\n`);
  }
  console.log(JSON.stringify(metadata, null, 2));
}

main();
