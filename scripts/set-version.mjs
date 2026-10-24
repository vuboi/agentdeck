// Sets the app version (tauri.conf.json and the Rust workspace) so the updater can compare it with the release.
// Usage: node scripts/set-version.mjs 0.1.0
import { readFileSync, writeFileSync } from 'node:fs';

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(version ?? '')) {
  console.error('Usage: node scripts/set-version.mjs <major.minor.patch>');
  process.exit(1);
}
const edit = (file, change) => writeFileSync(file, change(readFileSync(file, 'utf8')));
edit('apps/desktop/src-tauri/tauri.conf.json', (text) => text.replace(/"version": "[^"]*"/, `"version": "${version}"`));
edit('Cargo.toml', (text) => text.replace(/^version = "[^"]*"/m, `version = "${version}"`));
console.log(`Version set to ${version}`);
