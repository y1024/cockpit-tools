const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..');
const electronResourceDir = path.join(repoRoot, 'src-tauri', 'resources', 'electron');
const electronResourceGitkeep = path.join(electronResourceDir, '.gitkeep');

function pathExists(value) {
  try {
    return fs.existsSync(value);
  } catch {
    return false;
  }
}

function findElectronDist() {
  const directDist = path.join(repoRoot, 'node_modules', 'electron', 'dist');
  if (pathExists(directDist)) {
    return directDist;
  }

  const installScript = path.join(repoRoot, 'node_modules', 'electron', 'install.js');
  if (pathExists(installScript)) {
    const result = spawnSync(process.execPath, [installScript], {
      cwd: repoRoot,
      stdio: 'inherit',
      env: process.env,
    });
    if (result.status === 0 && pathExists(directDist)) {
      return directDist;
    }
  }

  try {
    const electronExecutable = require('electron');
    if (typeof electronExecutable === 'string' && pathExists(electronExecutable)) {
      let current = path.dirname(electronExecutable);
      while (current && current !== path.dirname(current)) {
        if (path.basename(current).toLowerCase() === 'dist') {
          return current;
        }
        current = path.dirname(current);
      }
    }
  } catch {
    // Ignore missing or incomplete electron package. The runtime error will explain the fix.
  }

  return null;
}

function prepareClaudeDesktopAuthElectron() {
  fs.mkdirSync(electronResourceDir, { recursive: true });
  const source = findElectronDist();
  if (!source) {
    fs.writeFileSync(electronResourceGitkeep, '\n');
    console.warn(
      '[prepare-tauri] Electron runtime not found. Claude Desktop login helper will be unavailable until npm install downloads electron.'
    );
    return;
  }

  fs.rmSync(electronResourceDir, { recursive: true, force: true });
  fs.mkdirSync(electronResourceDir, { recursive: true });
  fs.cpSync(source, electronResourceDir, {
    recursive: true,
    force: true,
    errorOnExist: false,
    verbatimSymlinks: true,
  });
  fs.writeFileSync(electronResourceGitkeep, '\n');
  console.log(`[prepare-tauri] Prepared Claude Desktop auth Electron runtime: ${source}`);
}

prepareClaudeDesktopAuthElectron();

if (process.platform !== 'win32') {
  process.exit(0);
}

const targetExe = path.join(repoRoot, 'target', 'debug', 'cockpit_tools.exe');
const escapedTarget = targetExe.replace(/'/g, "''").toLowerCase();

const script = `
$ErrorActionPreference = 'Stop'
$target = '${escapedTarget}'
$processes = Get-CimInstance Win32_Process -Filter "Name = 'cockpit_tools.exe'" |
  Where-Object { $_.ExecutablePath -and ($_.ExecutablePath.ToLowerInvariant() -eq $target) }
foreach ($process in $processes) {
  Stop-Process -Id $process.ProcessId -Force
  Write-Output ("Stopped stale Cockpit Tools debug process PID " + $process.ProcessId)
}
`;

const result = spawnSync(
  'powershell.exe',
  ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', script],
  {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  }
);

if (result.stdout) {
  process.stdout.write(result.stdout);
}

if (result.status !== 0) {
  if (result.stderr) {
    process.stderr.write(result.stderr);
  }
  process.exit(result.status ?? 1);
}
