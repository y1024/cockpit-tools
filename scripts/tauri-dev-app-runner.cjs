#!/usr/bin/env node

const { spawn, spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const APP_NAME = 'Cockpit Tools Dev';
const BUNDLE_ID = 'com.jlcodes.cockpit-tools.dev';
const EXECUTABLE_NAME = 'cockpit-tools-dev';
const CARGO_BIN_NAME = 'cockpit-tools';

const repoRoot = path.resolve(__dirname, '..');
const appRoot = path.join(repoRoot, 'target', 'dev-app', `${APP_NAME}.app`);
const contentsDir = path.join(appRoot, 'Contents');
const macosDir = path.join(contentsDir, 'MacOS');
const resourcesDir = path.join(contentsDir, 'Resources');
const appExecutablePath = path.join(macosDir, EXECUTABLE_NAME);

function fail(message) {
  console.error(`[tauri-dev-app-runner] ${message}`);
  process.exit(1);
}

function findBinaryPath(args) {
  for (const arg of args) {
    const candidate = path.resolve(repoRoot, arg);
    if (!fs.existsSync(candidate)) {
      continue;
    }
    const stat = fs.statSync(candidate);
    if (stat.isFile()) {
      return candidate;
    }
  }
  return null;
}

function splitCargoRunArgs(args) {
  const separatorIndex = args.indexOf('--');
  if (separatorIndex < 0) {
    return { cargoArgs: args, appArgs: [] };
  }
  return {
    cargoArgs: args.slice(0, separatorIndex),
    appArgs: args.slice(separatorIndex + 1),
  };
}

function valueAfter(args, longName, shortName) {
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === longName || arg === shortName) {
      return args[index + 1] ?? null;
    }
    if (arg.startsWith(`${longName}=`)) {
      return arg.slice(longName.length + 1);
    }
  }
  return null;
}

function resolveBuiltBinaryPath(cargoArgs) {
  const profile = cargoArgs.includes('--release') ? 'release' : 'debug';
  const target = valueAfter(cargoArgs, '--target', '-t');
  return target
    ? path.join(repoRoot, 'target', target, profile, CARGO_BIN_NAME)
    : path.join(repoRoot, 'target', profile, CARGO_BIN_NAME);
}

function buildCargoRunTarget(args) {
  if (args[0] !== 'run') {
    return null;
  }

  const { cargoArgs, appArgs } = splitCargoRunArgs(args);
  const buildArgs = ['build', ...cargoArgs.slice(1)];
  const result = spawnSync('cargo', buildArgs, {
    cwd: path.join(repoRoot, 'src-tauri'),
    env: process.env,
    stdio: 'inherit',
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }

  const binaryPath = resolveBuiltBinaryPath(cargoArgs);
  if (!fs.existsSync(binaryPath)) {
    fail(`cargo build succeeded, but binary was not found at ${binaryPath}`);
  }

  return { binaryPath, appArgs };
}

function writeFileIfChanged(filePath, content) {
  if (fs.existsSync(filePath) && fs.readFileSync(filePath, 'utf8') === content) {
    return;
  }
  fs.writeFileSync(filePath, content);
}

function escapeXml(value) {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
}

function copyIfChanged(source, target) {
  if (fs.existsSync(target)) {
    const sourceStat = fs.statSync(source);
    const targetStat = fs.statSync(target);
    if (
      sourceStat.size === targetStat.size &&
      Math.trunc(sourceStat.mtimeMs) === Math.trunc(targetStat.mtimeMs)
    ) {
      return;
    }
  }
  fs.copyFileSync(source, target);
  fs.chmodSync(target, 0o755);
}

function removeExisting(target) {
  if (!fs.existsSync(target)) {
    return;
  }
  const stat = fs.lstatSync(target);
  if (stat.isDirectory() && !stat.isSymbolicLink()) {
    fs.rmSync(target, { recursive: true, force: true });
  } else {
    fs.unlinkSync(target);
  }
}

function symlinkOrCopy(source, target) {
  if (!fs.existsSync(source)) {
    return;
  }
  if (fs.existsSync(target)) {
    const stat = fs.lstatSync(target);
    if (stat.isSymbolicLink() && fs.readlinkSync(target) === source) {
      return;
    }
    removeExisting(target);
  }
  try {
    fs.symlinkSync(source, target);
  } catch {
    const stat = fs.statSync(source);
    if (stat.isDirectory()) {
      fs.cpSync(source, target, { recursive: true });
    } else {
      fs.copyFileSync(source, target);
      fs.chmodSync(target, 0o755);
    }
  }
}

function createInfoPlist() {
  const appName = escapeXml(APP_NAME);
  const bundleId = escapeXml(BUNDLE_ID);
  const executableName = escapeXml(EXECUTABLE_NAME);
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>${executableName}</string>
  <key>CFBundleIdentifier</key>
  <string>${bundleId}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${appName}</string>
  <key>CFBundleDisplayName</key>
  <string>${appName}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.0.0-dev</string>
  <key>CFBundleVersion</key>
  <string>0.0.0-dev</string>
  <key>CFBundleIconFile</key>
  <string>icon</string>
  <key>LSMinimumSystemVersion</key>
  <string>10.13</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
`;
}

function prepareAppBundle(binaryPath) {
  fs.mkdirSync(macosDir, { recursive: true });
  fs.mkdirSync(resourcesDir, { recursive: true });

  writeFileIfChanged(path.join(contentsDir, 'Info.plist'), createInfoPlist());
  writeFileIfChanged(path.join(contentsDir, 'PkgInfo'), 'APPL????');

  copyIfChanged(binaryPath, appExecutablePath);

  const iconSource = path.join(repoRoot, 'src-tauri', 'icons', 'icon.icns');
  if (fs.existsSync(iconSource)) {
    fs.copyFileSync(iconSource, path.join(resourcesDir, 'icon.icns'));
  }

  const debugDir = path.dirname(binaryPath);
  symlinkOrCopy(path.join(repoRoot, 'platform-packages'), path.join(resourcesDir, 'platform-packages'));
  symlinkOrCopy(path.join(debugDir, 'native-menu-icons'), path.join(resourcesDir, 'native-menu-icons'));
  symlinkOrCopy(path.join(debugDir, 'scripts'), path.join(resourcesDir, 'scripts'));
  symlinkOrCopy(path.join(debugDir, 'cockpit-cliproxy'), path.join(macosDir, 'cockpit-cliproxy'));
}

const args = process.argv.slice(2);
const cargoRunTarget = buildCargoRunTarget(args);
const binaryPath = cargoRunTarget?.binaryPath ?? findBinaryPath(args);
if (!binaryPath) {
  fail(`cannot find Tauri binary in runner args: ${args.join(' ')}`);
}

const binaryArgIndex = args.findIndex((arg) => path.resolve(repoRoot, arg) === binaryPath);
const appArgs = cargoRunTarget?.appArgs ?? (binaryArgIndex >= 0 ? args.slice(binaryArgIndex + 1) : []);

prepareAppBundle(binaryPath);

const child = spawn(appExecutablePath, appArgs, {
  cwd: repoRoot,
  env: process.env,
  stdio: 'inherit',
});

function forwardSignal(signal) {
  if (!child.killed) {
    child.kill(signal);
  }
}

process.on('SIGINT', () => forwardSignal('SIGINT'));
process.on('SIGTERM', () => forwardSignal('SIGTERM'));

child.on('exit', (code, signal) => {
  if (signal) {
    process.exit(1);
    return;
  }
  process.exit(code ?? 0);
});
