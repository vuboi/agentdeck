// Backward-compatible entry point; all dev runs use the same isolated launcher.
process.argv = [process.argv[0], process.argv[1], 'dev', '--fake-update', ...process.argv.slice(2)];
await import('./run-desktop.mjs');
