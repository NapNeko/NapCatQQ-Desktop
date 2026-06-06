import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { resolve } from 'path';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src-ui/test/setup.ts'],
    include: ['src-ui/**/*.test.ts', 'src-ui/**/*.test.tsx'],
    passWithNoTests: false,
    restoreMocks: true,
    clearMocks: true,
  },
  resolve: {
    alias: {
      '@': resolve(__dirname, './src-ui'),
    },
  },
});
