import { sveltekit } from '@sveltejs/kit/vite';
import UnoCSS from 'unocss/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [UnoCSS(), sveltekit()],
  server: {
    proxy: {
      '/api': process.env.DEV_HUB_URL ?? 'http://127.0.0.1:8080',
      '/health': process.env.DEV_HUB_URL ?? 'http://127.0.0.1:8080'
    }
  }
});
