import { defineConfig, loadEnv } from 'vite';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';

// The dev port comes from `.env.development` (VITE_DEV_PORT), so the dev build can run beside the installed app.
// The Tauri dev config (src-tauri/tauri.dev.conf.json) points its window at the same port.
export default defineConfig(async ({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const port = Number(env.VITE_DEV_PORT || 1420);
  return {
    plugins: [tailwindcss(), sveltekit()],
    clearScreen: false,
    server: { host: '127.0.0.1', port, strictPort: true },
    // The UI library is linked from a folder that changes often: pre-bundle it afresh on every start, so a rebuilt copy is never
    // hidden by a bundle the browser cached under an unchanged address.
    optimizeDeps: { force: true },
    envPrefix: ['VITE_', 'TAURI_'],
    // The highlighter worker (src/lib/highlight.worker.ts) loads shiki's grammars on demand: an ES module worker keeps those
    // as separate chunks (the default IIFE format cannot split code).
    worker: { format: 'es' as const },
  };
});
