import { defineConfig } from 'astro/config';
import { satteri } from '@astrojs/markdown-satteri';
import docsMarkup from './scripts/rehype-docs.mjs';
import { prepareImages } from './scripts/prepare-images.mjs';

export default defineConfig({
  output: 'static',
  site: 'https://napneko.github.io',
  base: process.env.SITE_BASE || '/',
  devToolbar: { enabled: false },
  integrations: [{ name: 'local-image-assets', hooks: { 'astro:config:setup': () => prepareImages() } }],
  markdown: {
    processor: satteri({ hastPlugins: [docsMarkup()] }),
    shikiConfig: { themes: { light: 'github-light', dark: 'github-dark' } },
  },
});
