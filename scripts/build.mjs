// Local installers: `pnpm build:win`, `pnpm build:mac`, `pnpm build:linux` (or `pnpm build` for this machine).
// Tauri cannot cross-build, so each one has to run on its own OS; pushing a tag makes GitHub build all three
// (.github/workflows/release.yml). Updater signing is skipped here (it needs the release key), so these
// builds are for testing, not for publishing.
import { spawnSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';

const HOST = { win32: 'win', darwin: 'mac', linux: 'linux' }[process.platform];
const target = process.argv[2] ?? HOST;
const BUNDLES = { win: ['nsis'], mac: ['app', 'dmg'], linux: ['deb', 'appimage'] };

if (!BUNDLES[target]) {
  console.error('Usage: node scripts/build.mjs <win|mac|linux>');
  process.exit(1);
}
if (target !== HOST) {
  console.error(`A ${target} build has to run on ${target} (this is ${HOST}). Push a v* tag and GitHub builds all three.`);
  process.exit(1);
}

// mise's cargo shim is broken on some machines: the rustup one is the one to use.
const cargoBin = join(homedir(), '.cargo', 'bin');
const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === 'path') ?? 'PATH';
const env = { ...process.env, [pathKey]: [cargoBin, process.env[pathKey]].filter(Boolean).join(process.platform === 'win32' ? ';' : ':'), NODE_OPTIONS: '--max-old-space-size=6144' };
// A config file, not an inline JSON string: quoting that survives cmd.exe, PowerShell and sh alike is not worth it.
const configFile = join(mkdtempSync(join(tmpdir(), 'agentdeck-build-')), 'tauri.local.json');
writeFileSync(configFile, JSON.stringify({ bundle: { createUpdaterArtifacts: false } }));
const args = ['--filter', 'desktop', 'tauri', 'build', '--bundles', BUNDLES[target].join(','), '--config', configFile];
if (!process.env.TAURI_SIGNING_PRIVATE_KEY) console.log('No TAURI_SIGNING_PRIVATE_KEY: building without updater artifacts.');

const result = spawnSync('pnpm', args, { stdio: 'inherit', env, shell: process.platform === 'win32' });
if (result.status === 0) console.log(`\nInstallers: target/release/bundle/${BUNDLES[target][0]}`);
process.exit(result.status ?? 1);
