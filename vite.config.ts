import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Rust builds must not trigger unrelated WebView reloads while editing a test note.
    watch: { ignored: ['**/target/**', '**/src-tauri/**', '**/artifacts/**'] },
  },
  build: { target: ['es2022', 'chrome105', 'safari15'] },
});
