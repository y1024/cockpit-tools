#!/usr/bin/env node

const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.resolve(__dirname, '..');
const VERSION_RE = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/u;

function fail(message) {
  console.error(`[prepare-test-channel-version] ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const args = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--help' || arg === '-h') {
      console.log(`Usage:
  node scripts/prepare-test-channel-version.cjs [--version <semver>]

If --version is omitted, COCKPIT_TEST_CHANNEL_VERSION is used.
Empty version means no-op.`);
      process.exit(0);
    }
    if (arg !== '--version') fail(`Unknown argument: ${arg}`);
    const next = argv[index + 1];
    if (!next || next.startsWith('--')) fail('Missing value for --version');
    args.version = next;
    index += 1;
  }
  return args;
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function writeJson(filePath, data) {
  fs.writeFileSync(filePath, `${JSON.stringify(data, null, 2)}\n`);
}

function updatePackageJson(version) {
  const filePath = path.join(ROOT, 'package.json');
  const pkg = readJson(filePath);
  pkg.version = version;
  writeJson(filePath, pkg);
}

function updatePackageLock(version) {
  const filePath = path.join(ROOT, 'package-lock.json');
  if (!fs.existsSync(filePath)) return;

  const lock = readJson(filePath);
  lock.version = version;
  if (lock.packages && lock.packages['']) {
    lock.packages[''].version = version;
  }
  writeJson(filePath, lock);
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const version = String(args.version || process.env.COCKPIT_TEST_CHANNEL_VERSION || '').trim();
  if (!version) {
    console.log('[prepare-test-channel-version] no test version override, skipped');
    return;
  }
  if (!VERSION_RE.test(version)) {
    fail(`Invalid semver: ${version}`);
  }

  updatePackageJson(version);
  updatePackageLock(version);
  console.log(`[prepare-test-channel-version] prepared test version ${version}`);
}

main();
