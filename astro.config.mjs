import { defineConfig } from 'astro/config';
import { satteri } from '@astrojs/markdown-satteri';
import docsMarkup from './scripts/rehype-docs.mjs';

export default defineConfig({
  output: 'static',
  base: process.env.SITE_BASE || '/',
  devToolbar: { enabled: false },
  markdown: {
    processor: satteri({ hastPlugins: [docsMarkup()] }),
    shikiConfig: { themes: { light: 'github-light', dark: 'github-dark' } },
  },
});
