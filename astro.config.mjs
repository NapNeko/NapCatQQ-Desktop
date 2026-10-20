import { defineConfig } from 'astro/config';

export default defineConfig({
  output: 'static',
  base: process.env.SITE_BASE || '/',
  devToolbar: { enabled: false },
});
