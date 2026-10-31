// One entry point for isolated native dev, browser dev, and a locally built release app.
import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:net';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const mode = process.argv[2];
const dryRun = process.argv.includes('--dry-run');
if (!['dev', 'web', 'build'].includes(mode)) throw new Error('Usage: pnpm dev | pnpm dev:web | pnpm run:build');
const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === 'path') ?? 'PATH';
const env = { ...process.env, [pathKey]: [join(homedir(), '.cargo', 'bin'), process.env[pathKey]].filter(Boolean).join(process.platform === 'win32' ? ';' : ':') };
if (process.platform === 'win32' && mode !== 'web') {
  // whisper-rs uses bindgen + CMake. Discover the standard Windows installs so pnpm dev/build works from an ordinary
  // terminal; explicit variables still win for custom LLVM or Visual Studio installations.
  const programFiles = process.env.ProgramFiles ?? 'C:\\Program Files';
  const llvm = join(programFiles, 'LLVM');
  if (existsSync(join(llvm, 'bin', 'libclang.dll'))) {
    env.LIBCLANG_PATH ??= join(llvm, 'bin');
    const clangRoot = join(llvm, 'lib', 'clang');
    const resources = existsSync(clangRoot) ? readdirSync(clangRoot, { withFileTypes: true }).filter((entry) => entry.isDirectory()).map((entry) => entry.name).sort().at(-1) : undefined;
    if (resources) env.BINDGEN_EXTRA_CLANG_ARGS ??= `-resource-dir="${join(clangRoot, resources)}"`;
  }
  const programFilesX86 = process.env['ProgramFiles(x86)'] ?? 'C:\\Program Files (x86)';
  const hasCpp = (folder) => existsSync(join(folder, 'VC', 'Tools', 'MSVC'));
  const vs18 = join(programFiles, 'Microsoft Visual Studio', '18', 'Community');
  const vs17 = join(programFilesX86, 'Microsoft Visual Studio', '2022', 'BuildTools');
  const visualStudio = [vs18, vs17].find(hasCpp);
  if (!process.env.VSCMD_VER && visualStudio) {
    const command = `call "${join(visualStudio, 'Common7', 'Tools', 'VsDevCmd.bat')}" -arch=x64 >nul && set`;
    const result = spawnSync('cmd.exe', ['/d', '/s', '/c', command], {
      encoding: 'utf8',
      windowsHide: true,
      windowsVerbatimArguments: true,
    });
    if (result.status === 0) {
      const developer = Object.fromEntries(result.stdout.split(/\r?\n/).flatMap((line) => {
        const separator = line.indexOf('=');
        return separator > 0 ? [[line.slice(0, separator), line.slice(separator + 1)]] : [];
      }));
      const developerPathKey = Object.keys(developer).find((key) => key.toLowerCase() === 'path');
      if (developerPathKey) env[pathKey] = [developer[developerPathKey], env[pathKey]].filter(Boolean).join(';');
      for (const [key, value] of Object.entries(developer)) {
        if (key.toLowerCase() !== 'path') env[key] ??= value;
      }
    }
  }
  if (!env.CMAKE_GENERATOR) {
    if (visualStudio === vs18) env.CMAKE_GENERATOR = 'Visual Studio 18 2026';
    else if (visualStudio === vs17) env.CMAKE_GENERATOR = 'Visual Studio 17 2022';
  }
}
if (process.platform === 'linux') {
  env.GDK_BACKEND ??= 'x11';
  env.WEBKIT_DISABLE_DMABUF_RENDERER ??= '1';
  env.WEBKIT_DISABLE_COMPOSITING_MODE ??= '1';
}
if (process.argv.includes('--fake-update')) env.VITE_FAKE_UPDATE = '1';

/** Probe without taking over or terminating the process already listening on a port. */
function available(port) {
  return new Promise((accept, reject) => {
    const server = createServer();
    server.once('error', (error) => error.code === 'EADDRINUSE' ? accept(false) : reject(error));
    server.listen(port, '127.0.0.1', () => server.close(() => accept(true)));
  });
}

let config;
if (mode !== 'build') {
  const defaults = JSON.parse(readFileSync(join(root, 'apps/desktop/src-tauri/tauri.dev.conf.json'), 'utf8'));
  const preferred = Number(process.env.VITE_DEV_PORT || new URL(defaults.build.devUrl).port);
  if (!Number.isInteger(preferred) || preferred < 1024 || preferred > 65515) throw new Error('VITE_DEV_PORT must be an integer between 1024 and 65515');
  let port = preferred;
  while (!(await available(port))) {
    if (++port > preferred + 20) throw new Error('No free development port in the requested range');
  }
  env.VITE_DEV_PORT = String(port);
  env.VITE_APP_CHANNEL = 'dev';
  env.AGENTDECK_DEV_ORIGIN = `http://127.0.0.1:${port}`;
  const base = JSON.parse(readFileSync(join(root, 'apps/desktop/src-tauri/tauri.conf.json'), 'utf8'));
  config = {
    ...defaults,
    build: { ...defaults.build, devUrl: env.AGENTDECK_DEV_ORIGIN },
    app: { security: { devCsp: base.app.security.devCsp.replace(/(https?:|wss?:)\/\/127\.0\.0\.1:\d+/g, (_, scheme) => `${scheme}//127.0.0.1:${port}`) } },
  };
  console.log(`AgentDeck ${mode === 'web' ? 'browser dev' : 'Dev'}: ${env.AGENTDECK_DEV_ORIGIN}${port !== preferred ? ` (${preferred} is busy)` : ''}`);
} else {
  config = JSON.parse(readFileSync(join(root, 'apps/desktop/src-tauri/tauri.local.conf.json'), 'utf8'));
  env.NODE_OPTIONS ??= '--max-old-space-size=6144';
  env.VITE_APP_CHANNEL = 'local';
  env.CARGO_TARGET_DIR = join(root, 'target', 'local');
  delete env.VITE_FAKE_UPDATE;
  delete env.AGENTDECK_DEV_ORIGIN;
  console.log('AgentDeck Local: build and run embedded production UI, without a Vite server');
}

if (dryRun) {
  console.log(JSON.stringify({ mode, config, devOrigin: env.AGENTDECK_DEV_ORIGIN ?? null, fakeUpdate: env.VITE_FAKE_UPDATE === '1', packageInstallers: false, targetDirectory: env.CARGO_TARGET_DIR ?? null, pnpmEntrypoint: process.env.npm_execpath ?? null }, null, 2));
  process.exit(0);
}

// pnpm supplies its own entry point to scripts. Use Node/native spawning so config paths with spaces need no shell.
const pnpm = process.env.npm_execpath;
if (!pnpm || !/\.(?:[cm]?js|exe)$/i.test(pnpm)) throw new Error('Launch this script through pnpm so its executable can be resolved safely');
const nativePnpm = /\.exe$/i.test(pnpm);
const runPnpm = (args) => new Promise((accept, reject) => {
  const child = spawn(nativePnpm ? pnpm : process.execPath, nativePnpm ? args : [pnpm, ...args], { cwd: root, env, stdio: 'inherit' });
  child.once('error', reject);
  child.once('exit', (code) => accept(code ?? 1));
});

let temporary;
try {
  let args;
  if (mode === 'web') {
    args = ['--filter', 'desktop', 'dev:web'];
  } else {
    temporary = mkdtempSync(join(tmpdir(), 'agentdeck-run-'));
    const file = join(temporary, 'tauri.json');
    writeFileSync(file, JSON.stringify(config));
    args = ['--filter', 'desktop', 'tauri', mode === 'dev' ? 'dev' : 'build', '--config', file];
    if (mode === 'build') args.push('--no-bundle');
  }
  const code = await runPnpm(args);
  if (code !== 0 || mode !== 'build') process.exitCode = code;
  else {
    // Separate target output avoids a file lock from another locally running release executable.
    const executable = join(root, 'target', 'local', 'release', process.platform === 'win32' ? 'agentdeck-desktop.exe' : 'agentdeck-desktop');
    const child = spawn(executable, [], { cwd: root, env, detached: true, stdio: 'ignore', windowsHide: true });
    await new Promise((accept, reject) => { child.once('spawn', accept); child.once('error', reject); });
    child.unref();
    console.log('AgentDeck Local launched');
  }
} finally {
  // Delete only the exact temporary folder created above, under the OS temporary directory.
  if (temporary && resolve(temporary).startsWith(`${resolve(tmpdir())}${sep}`)) rmSync(temporary, { recursive: true, force: true });
}
