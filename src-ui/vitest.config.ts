import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { resolve } from 'path';

// 配置在 src-ui/；pnpm 从仓库根调用时 cwd 仍是根，用 root 固定 UI 树。
const uiRoot = __dirname;

export default defineConfig({
  root: uiRoot,
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
    // 相对 root（src-ui），不要用仓库根相对路径或绝对 path resolve 做 glob。
    setupFiles: ['./test/setup.ts'],
    include: ['**/*.{test,spec}.{ts,tsx}'],
    exclude: [
      '**/node_modules/**',
      '**/dist/**',
      '**/.references/**',
      '**/target/**',
    ],
    passWithNoTests: false,
    restoreMocks: true,
    clearMocks: true,
  },
  resolve: {
    alias: {
      '@': uiRoot,
    },
  },
});
