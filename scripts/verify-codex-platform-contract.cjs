#!/usr/bin/env node

const fs = require('node:fs');
const net = require('node:net');
const path = require('node:path');
const { spawn } = require('node:child_process');

const ROOT = path.resolve(__dirname, '..');
const MANIFEST_PATH = path.join(ROOT, 'platform-packages', 'codex', 'manifest.json');
const ADAPTER_SOURCE_PATH = path.join(ROOT, 'crates', 'cockpit-codex-adapter', 'src', 'main.rs');
const FACADE_SOURCE_PATHS = [
  path.join(ROOT, 'src-tauri', 'src', 'commands', 'codex.rs'),
  path.join(ROOT, 'src-tauri', 'src', 'commands', 'codex_instance.rs'),
  path.join(ROOT, 'src-tauri', 'src', 'modules', 'platform_adapter.rs'),
];
const MODEL_PROVIDER_SERVICE_PATH = path.join(
  ROOT,
  'src',
  'services',
  'codexModelProviderService.ts',
);
const UI_TIMEOUT_CHECKS = [
  {
    name: 'model provider list',
    filePath: path.join(ROOT, 'src', 'components', 'codex', 'CodexModelProviderManager.tsx'),
    patterns: ['CODEX_PROVIDER_LOAD_TIMEOUT_MS', 'codex.modelProviders.loadTimeout', 'withTimeout('],
  },
  {
    name: 'wakeup overview',
    filePath: path.join(ROOT, 'src', 'stores', 'useCodexWakeupStore.ts'),
    patterns: ['CODEX_WAKEUP_LOAD_TIMEOUT_MS', 'withTimeout('],
  },
  {
    name: 'wakeup timeout message',
    filePath: path.join(ROOT, 'src', 'components', 'codex', 'CodexWakeupContent.tsx'),
    patterns: ['codex.wakeup.loadTimeout'],
  },
  {
    name: 'instance list',
    filePath: path.join(ROOT, 'src', 'stores', 'createInstanceStore.ts'),
    patterns: ['INSTANCE_LIST_TIMEOUT_MS', 'INSTANCE_LIST_TIMEOUT_ERROR', 'withTimeout('],
  },
];
const MACOS_ADAPTER_PATH = path.join(
  ROOT,
  'platform-packages',
  'codex',
  'adapter',
  'macos',
  'cockpit-codex-adapter',
);
const MACOS_CLIPROXY_PATH = path.join(
  ROOT,
  'platform-packages',
  'codex',
  'adapter',
  'macos',
  'cockpit-cliproxy',
);

const args = new Set(process.argv.slice(2));
const runSmoke = args.has('--smoke');
const issues = [];

const CRITICAL_METHODS = [
  'modelProviders.load',
  'localAccess.getState',
  'localAccess.saveAccounts',
  'wakeup.getOverview',
  'wakeup.getState',
  'instances.list',
];

const SMOKE_METHODS = [
  'health.check',
  'modelProviders.load',
  'localAccess.getState',
  'wakeup.getOverview',
  'wakeup.getState',
  'instances.list',
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

function isExecutable(filePath) {
  try {
    return (fs.statSync(filePath).mode & 0o111) !== 0;
  } catch {
    return false;
  }
}

function fail(message) {
  issues.push(message);
}

function uniqueSorted(values) {
  return [...new Set(values)].sort((a, b) => a.localeCompare(b));
}

function extractAdapterMethods(source) {
  const methods = [];
  const regex = /"([^"]+)"\s*=>/g;
  let match;
  while ((match = regex.exec(source))) {
    methods.push(match[1]);
  }
  return uniqueSorted(methods);
}

function extractFacadeMethods(source) {
  const methods = [];
  const regex =
    /platform_adapter::call_codex(?:_value|_with_timeout)?(?:::<[^>]+>)?\(\s*"([^"]+)"/g;
  let match;
  while ((match = regex.exec(source))) {
    methods.push(match[1]);
  }
  return uniqueSorted(methods);
}

function verifyUiTimeoutCoverage() {
  for (const check of UI_TIMEOUT_CHECKS) {
    const source = readText(check.filePath);
    for (const pattern of check.patterns) {
      if (!source.includes(pattern)) {
        fail(`ui timeout missing for ${check.name}: ${relative(check.filePath)} lacks ${pattern}`);
      }
    }
  }
}

function verifyModelProviderLoadDoesNotAwaitMigrationSave() {
  const source = readText(MODEL_PROVIDER_SERVICE_PATH);
  if (source.includes('await saveProvidersToDisk(loaded)')) {
    fail(
      `model provider first load blocks on migration save: ${relative(
        MODEL_PROVIDER_SERVICE_PATH,
      )}`,
    );
  }
  if (!source.includes('void saveProvidersToDisk(loaded).catch')) {
    fail(
      `model provider migration save is not fire-and-forget: ${relative(
        MODEL_PROVIDER_SERVICE_PATH,
      )}`,
    );
  }
}

function verifyCodexRuntimeHelpers() {
  if (!fs.existsSync(MACOS_CLIPROXY_PATH)) {
    fail(`codex API service helper missing: ${relative(MACOS_CLIPROXY_PATH)}`);
    return;
  }
  if (!isExecutable(MACOS_CLIPROXY_PATH)) {
    fail(`codex API service helper is not executable: ${relative(MACOS_CLIPROXY_PATH)}`);
  }
}

function shapeOf(value) {
  if (Array.isArray(value)) return `array(${value.length})`;
  if (value === null) return 'null';
  return typeof value;
}

function once(emitter, event) {
  return new Promise((resolve) => emitter.once(event, resolve));
}

async function readBootstrap(child) {
  let buffer = '';
  const startedAt = Date.now();
  return new Promise((resolve, reject) => {
    const timer = setInterval(() => {
      if (Date.now() - startedAt > 10_000) {
        clearInterval(timer);
        reject(new Error('adapter bootstrap timed out'));
      }
    }, 100);
    child.stdout.on('data', (chunk) => {
      buffer += chunk.toString('utf8');
      const lineBreak = buffer.indexOf('\n');
      if (lineBreak === -1) return;
      clearInterval(timer);
      const line = buffer.slice(0, lineBreak).trim();
      try {
        resolve(JSON.parse(line));
      } catch (error) {
        reject(new Error(`adapter bootstrap is not JSON: ${line || error.message}`));
      }
    });
    child.once('exit', (code, signal) => {
      clearInterval(timer);
      reject(new Error(`adapter exited before bootstrap: code=${code} signal=${signal}`));
    });
    child.once('error', (error) => {
      clearInterval(timer);
      reject(error);
    });
  });
}

async function callRpc(endpoint, method, payload = {}) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 10_000);
  const startedAt = Date.now();
  try {
    const response = await fetch(`http://${endpoint.host}:${endpoint.port}/rpc`, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${endpoint.token}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ method, payload }),
      signal: controller.signal,
    });
    const body = await response.json();
    if (!response.ok || !body.ok) {
      throw new Error(body?.error?.message || `HTTP ${response.status}`);
    }
    return {
      elapsedMs: Date.now() - startedAt,
      shape: shapeOf(body.data),
      data: body.data,
    };
  } finally {
    clearTimeout(timer);
  }
}

function openSlowAuthorizedRequest(endpoint) {
  return new Promise((resolve, reject) => {
    const socket = net.createConnection(
      { host: endpoint.host, port: endpoint.port },
      () => {
        socket.write(
          [
            'POST /rpc HTTP/1.1',
            `Host: ${endpoint.host}:${endpoint.port}`,
            `Authorization: Bearer ${endpoint.token}`,
            'Content-Type: application/json',
            'Content-Length: 1048576',
            'Connection: keep-alive',
            '',
            '',
          ].join('\r\n'),
        );
        resolve(socket);
      },
    );
    socket.setTimeout(10_000, () => {
      socket.destroy(new Error('slow request socket timed out'));
    });
    socket.once('error', reject);
  });
}

async function verifyAdapterConcurrency(endpoint) {
  const slowSocket = await openSlowAuthorizedRequest(endpoint);
  try {
    const result = await callRpc(endpoint, 'modelProviders.load');
    return {
      method: 'concurrency.fastReadDuringSlowRequest',
      ...result,
    };
  } finally {
    slowSocket.destroy();
  }
}

async function verifyLocalAccessSaveNoop(endpoint, localAccessState) {
  const collection = localAccessState?.collection;
  if (!collection) {
    return {
      method: 'localAccess.saveAccounts(noop)',
      elapsedMs: 0,
      shape: 'skipped(no collection)',
    };
  }

  const accountIds = Array.isArray(collection.accountIds) ? collection.accountIds : [];
  const restrictFreeAccounts =
    typeof collection.restrictFreeAccounts === 'boolean' ? collection.restrictFreeAccounts : true;
  const result = await callRpc(endpoint, 'localAccess.saveAccounts', {
    accountIds,
    restrictFreeAccounts,
  });
  return {
    method: 'localAccess.saveAccounts(noop)',
    ...result,
  };
}

async function runAdapterSmoke() {
  if (!fs.existsSync(MACOS_ADAPTER_PATH)) {
    fail(`smoke: missing adapter executable ${relative(MACOS_ADAPTER_PATH)}`);
    return [];
  }
  const child = spawn(MACOS_ADAPTER_PATH, [], {
    cwd: ROOT,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  const stderrChunks = [];
  child.stderr.on('data', (chunk) => stderrChunks.push(chunk.toString('utf8')));
  const results = [];
  try {
    const bootstrap = await readBootstrap(child);
    if (!bootstrap.ok || bootstrap.protocol !== 'http-json-v1') {
      throw new Error(`unexpected bootstrap: ${JSON.stringify(bootstrap)}`);
    }
    let localAccessState = null;
    for (const method of SMOKE_METHODS) {
      const result = await callRpc(bootstrap, method);
      results.push({ method, ...result });
      if (method === 'localAccess.getState') {
        localAccessState = result.data;
      }
    }
    results.push(await verifyLocalAccessSaveNoop(bootstrap, localAccessState));
    results.push(await verifyAdapterConcurrency(bootstrap));
    await callRpc(bootstrap, 'adapter.shutdown').catch(() => null);
    await once(child, 'exit').catch(() => null);
  } catch (error) {
    fail(`smoke: ${error.message}`);
  } finally {
    if (!child.killed) {
      child.kill('SIGTERM');
    }
    const stderr = stderrChunks.join('').trim();
    if (stderr) {
      console.log(`adapter stderr:\n${stderr}`);
    }
  }
  return results;
}

async function main() {
  const manifest = readJson(MANIFEST_PATH);
  const manifestMethods = uniqueSorted(manifest?.adapter?.methods || []);
  const adapterMethods = extractAdapterMethods(readText(ADAPTER_SOURCE_PATH));
  const facadeMethods = uniqueSorted(
    FACADE_SOURCE_PATHS.flatMap((filePath) => extractFacadeMethods(readText(filePath))),
  );
  verifyUiTimeoutCoverage();
  verifyModelProviderLoadDoesNotAwaitMigrationSave();
  verifyCodexRuntimeHelpers();

  for (const method of CRITICAL_METHODS) {
    if (!manifestMethods.includes(method)) {
      fail(`critical method missing in manifest: ${method}`);
    }
    if (!adapterMethods.includes(method)) {
      fail(`critical method missing in adapter: ${method}`);
    }
  }

  for (const method of facadeMethods) {
    if (!manifestMethods.includes(method)) {
      fail(`facade calls method not declared by manifest: ${method}`);
    }
    if (!adapterMethods.includes(method)) {
      fail(`facade calls method not implemented by adapter: ${method}`);
    }
  }

  for (const method of manifestMethods) {
    if (!adapterMethods.includes(method)) {
      fail(`manifest declares method not implemented by adapter: ${method}`);
    }
  }

  console.log('Codex platform contract');
  console.log(`- manifest methods: ${manifestMethods.length}`);
  console.log(`- adapter methods: ${adapterMethods.length}`);
  console.log(`- facade methods: ${facadeMethods.length}`);

  if (runSmoke) {
    const results = await runAdapterSmoke();
    if (results.length > 0) {
      console.log('- smoke:');
      for (const row of results) {
        console.log(`  ${row.method}: ${row.elapsedMs}ms ${row.shape}`);
      }
    }
  } else {
    console.log('- smoke: skipped (pass --smoke to start the local adapter)');
  }

  if (issues.length > 0) {
    console.error('\nIssues:');
    for (const issue of issues) {
      console.error(`- ${issue}`);
    }
    process.exit(1);
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
