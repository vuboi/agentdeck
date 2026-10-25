// `pnpm dev:update`: the desktop app in dev mode with a pretend update waiting, so the update dialog opens at start.
import { spawnSync } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';

const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === 'path') ?? 'PATH';
const env = { ...process.env, VITE_FAKE_UPDATE: '1', [pathKey]: [join(homedir(), '.cargo', 'bin'), process.env[pathKey]].filter(Boolean).join(process.platform === 'win32' ? ';' : ':') };
const result = spawnSync('pnpm', ['--filter', 'desktop', 'tauri', 'dev'], { stdio: 'inherit', env, shell: process.platform === 'win32' });
process.exit(result.status ?? 1);
