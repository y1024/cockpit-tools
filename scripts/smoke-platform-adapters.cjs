#!/usr/bin/env node

const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.resolve(__dirname, '..');
const INDEX_PATH = path.join(ROOT, 'platform-packages', 'index.json');
const DATA_DIR = path.join(ROOT, '.aitasks', 'platform-smoke-data');
const DEFAULT_TIMEOUT_MS = 12000;
const args = new Set(process.argv.slice(2));
const allowEnvironmentBlocked = args.has('--allow-environment-blocked');
const contractOnly = args.has('--contract-only');
const REPORT_PATH = path.join(
  ROOT,
  '.aitasks',
  contractOnly ? 'platform-adapter-contract-report.json' : 'platform-adapter-smoke-report.json',
);

const REQUIRED_METHODS = [
  'health.check',
  'accounts.list',
  'accounts.current',
];

const PREFERRED_METHODS = [
  'health.check',
  'accounts.list',
  'accounts.current',
  'modelProviders.load',
  'localAccess.getState',
  'wakeup.getOverview',
  'wakeup.getState',
  'instances.list',
];

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function adapterEntryForCurrentOs(adapter) {
  if (process.platform === 'darwin') return adapter.macosEntry || adapter.entry;
  if (process.platform === 'win32') return adapter.windowsEntry || adapter.entry;
  return adapter.linuxEntry || adapter.entry;
}

function isEnvironmentBlockedMessage(message) {
  return (
    /(EPERM|EACCES|PermissionDenied|Operation not permitted)/i.test(message)
    && /(bind|listen|server|127\.0\.0\.1|localhost)/i.test(message)
  );
}

function environmentBlockedFailures(failures) {
  return failures.length > 0 && failures.every(isEnvironmentBlockedMessage);
}

function waitForStartup(child, platformId) {
  return new Promise((resolve, reject) => {
    let stdout = '';
    let stderr = '';
    const timer = setTimeout(() => {
      cleanup();
      reject(new Error(`adapter 启动超时: ${platformId}; stderr=${stderr.slice(-500)}`));
    }, DEFAULT_TIMEOUT_MS);

    const onStdout = (chunk) => {
      stdout += chunk.toString();
      const line = stdout.split(/\r?\n/).find((item) => item.trim().startsWith('{'));
      if (!line) return;
      try {
        const info = JSON.parse(line);
        cleanup();
        resolve(info);
      } catch {
        // Continue reading until a complete JSON line is available.
      }
    };
    const onStderr = (chunk) => {
      stderr += chunk.toString();
    };
    const onExit = (code, signal) => {
      cleanup();
      reject(new Error(`adapter 启动前退出: ${platformId}; code=${code}; signal=${signal}; stderr=${stderr.slice(-500)}`));
    };
    const cleanup = () => {
      clearTimeout(timer);
      child.stdout.off('data', onStdout);
      child.stderr.off('data', onStderr);
      child.off('exit', onExit);
    };

    child.stdout.on('data', onStdout);
    child.stderr.on('data', onStderr);
    child.on('exit', onExit);
  });
}

async function rpc(info, method, payload = null) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), DEFAULT_TIMEOUT_MS);
  try {
    const response = await fetch(`http://${info.host}:${info.port}/rpc`, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${info.token}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ method, payload }),
      signal: controller.signal,
    });
    const text = await response.text();
    let json = null;
    try {
      json = JSON.parse(text);
    } catch {
      throw new Error(`${method}: 响应不是 JSON: ${text.slice(0, 200)}`);
    }
    if (!response.ok || !json.ok) {
      throw new Error(`${method}: ${json?.error?.message || response.statusText || `HTTP ${response.status}`}`);
    }
    return json.data;
  } finally {
    clearTimeout(timer);
  }
}

async function shutdown(child, info) {
  if (!info) {
    child.kill('SIGTERM');
    return;
  }
  try {
    await rpc(info, 'adapter.shutdown');
  } catch {
    child.kill('SIGTERM');
  }
}

async function smokePackage(pkg) {
  const adapter = pkg.adapter;
  if (!adapter) {
    return {
      id: pkg.id,
      ok: false,
      failures: ['缺少 adapter 声明'],
      contract: {
        ok: false,
        requiredMethods: REQUIRED_METHODS,
        declaredMethods: [],
        missingMethods: REQUIRED_METHODS,
      },
      methods: [],
    };
  }

  const entry = adapterEntryForCurrentOs(adapter);
  const adapterPath = path.join(ROOT, 'platform-packages', pkg.id, entry || '');
  const failures = [];
  const methodResults = [];
  const declaredMethods = adapter.methods || [];
  const available = new Set(declaredMethods);
  const missingRequiredMethods = REQUIRED_METHODS.filter((method) => !available.has(method));
  const contract = {
    ok: missingRequiredMethods.length === 0,
    requiredMethods: REQUIRED_METHODS,
    declaredMethods,
    missingMethods: missingRequiredMethods,
  };
  if (missingRequiredMethods.length > 0) {
    failures.push(`缺少基础 adapter 方法: ${missingRequiredMethods.join(', ')}`);
  }

  if (!entry || !fs.existsSync(adapterPath)) {
    failures.push(`adapter 文件不存在: ${entry || '<empty>'}`);
    return {
      id: pkg.id,
      ok: false,
      contract,
      failures,
      methods: [],
    };
  }

  if (contractOnly || failures.length > 0) {
    return {
      id: pkg.id,
      ok: failures.length === 0,
      contract,
      failures,
      methods: REQUIRED_METHODS
        .filter((method) => available.has(method))
        .map((method) => ({ method, ok: true, declared: true, skipped: 'contract-only' })),
    };
  }

  const env = {
    ...process.env,
    COCKPIT_TOOLS_PROFILE: 'dev',
    COCKPIT_TOOLS_DATA_DIR: DATA_DIR,
    VITE_COCKPIT_TOOLS_PROFILE: 'dev',
    COCKPIT_HOST_EVENT_URL: '',
    COCKPIT_HOST_EVENT_TOKEN: '',
  };
  const child = spawn(adapterPath, [], {
    cwd: ROOT,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  let info = null;
  try {
    info = await waitForStartup(child, pkg.id);
    if (!info || info.protocol !== 'http-json-v1' || !info.port || !info.token) {
      throw new Error(`adapter 启动信息无效: ${JSON.stringify(info)}`);
    }

    for (const method of PREFERRED_METHODS.filter((item) => available.has(item))) {
      try {
        const startedAt = Date.now();
        await rpc(info, method);
        methodResults.push({ method, ok: true, elapsedMs: Date.now() - startedAt });
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        methodResults.push({ method, ok: false, error: message });
        failures.push(message);
      }
    }
  } catch (error) {
    failures.push(error instanceof Error ? error.message : String(error));
  } finally {
    await shutdown(child, info);
  }

  return {
    id: pkg.id,
    ok: failures.length === 0,
    environmentBlocked: environmentBlockedFailures(failures),
    contract,
    failures,
    methods: methodResults,
  };
}

async function main() {
  fs.mkdirSync(DATA_DIR, { recursive: true });
  fs.mkdirSync(path.dirname(REPORT_PATH), { recursive: true });

  const index = readJson(INDEX_PATH);
  const packages = index.packages || [];
  const results = [];
  for (const pkg of packages) {
    const result = await smokePackage(pkg);
    results.push(result);
    const mark = result.ok ? 'OK' : result.environmentBlocked ? 'BLOCKED' : 'FAIL';
    const methodSummary = result.methods
      .map((item) => {
        if (!item.ok) return `${item.method}:ERR`;
        if (item.declared) return `${item.method}:declared`;
        return `${item.method}:${item.elapsedMs}ms`;
      })
      .join(', ');
    console.log(`${mark} ${pkg.id}${methodSummary ? ` ${methodSummary}` : ''}`);
    for (const failure of result.failures) {
      console.log(`  - ${failure}`);
    }
  }

  const report = {
    generatedAt: new Date().toISOString(),
    dataDir: DATA_DIR,
    allowEnvironmentBlocked,
    contractOnly,
    results,
  };
  fs.writeFileSync(REPORT_PATH, `${JSON.stringify(report, null, 2)}\n`);

  const failed = results.filter((item) => !item.ok);
  if (failed.length > 0) {
    const blocked = failed.filter((item) => item.environmentBlocked);
    if (allowEnvironmentBlocked && blocked.length === failed.length) {
      console.warn(
        `Platform adapter smoke environment-blocked: ${blocked.length}/${results.length}; report=${REPORT_PATH}`,
      );
      process.exit(0);
    }
    console.error(`Platform adapter smoke failed: ${failed.length}/${results.length}`);
    process.exit(1);
  }
  console.log(`Platform adapter smoke passed: ${results.length}/${results.length}`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack || error.message : String(error));
  process.exit(1);
});
