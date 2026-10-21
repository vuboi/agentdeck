import { homedir } from 'node:os';
import { join } from 'node:path';
import { spawn } from 'node:child_process';

const cargoBin = join(homedir(), '.cargo', 'bin');
const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === 'path') ?? 'PATH';
const separator = process.platform === 'win32' ? ';' : ':';
let spawnFailed = false;
const child = spawn('cargo', process.argv.slice(2), {
  stdio: 'inherit',
  env: {
    ...process.env,
    [pathKey]: [cargoBin, process.env[pathKey]].filter(Boolean).join(separator),
  },
});

child.on('error', (error) => {
  spawnFailed = true;
  console.error(`Failed to start cargo: ${error.message}`);
  process.exitCode = 1;
});
child.on('exit', (code, signal) => {
  if (!spawnFailed) process.exitCode = code ?? (signal ? 1 : 0);
});
