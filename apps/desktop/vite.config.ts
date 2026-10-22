import { defineConfig } from 'vite';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig(async () => ({
  plugins: [tailwindcss(), sveltekit()],
  clearScreen: false,
  server: { host: '127.0.0.1', port: 1420, strictPort: true },
  // The UI library is linked from a folder that changes often: pre-bundle it afresh on every start, so a rebuilt copy is never
  // hidden by a bundle the browser cached under an unchanged address.
  optimizeDeps: { force: true },
  envPrefix: ['VITE_', 'TAURI_'],
}));
