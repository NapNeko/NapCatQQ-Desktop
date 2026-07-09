import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { resolve } from 'path';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  define: {
    // dev 启动即拉全量组件 detect（含远端 SSH）会拖慢首屏；需要测组件页时设 VITE_SKIP_COMPONENTS_WARMUP=0
    'import.meta.env.VITE_SKIP_COMPONENTS_WARMUP': JSON.stringify(
      process.env.VITE_SKIP_COMPONENTS_WARMUP ?? '1',
    ),
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // 改 UI 时尽量只热更模块，少触发整页 reload（仍可能因改 index/main 而全刷）
    hmr: { overlay: true },
    watch: {
      ignored: [
        '**/target/**',
        '**/.references/**',
        '**/.codex/**',
        '**/.claude/**',
      ],
    },
  },
  optimizeDeps: {
    // 预打包大依赖，缩短 dev 冷启动与 HMR 后重新拉依赖的时间
    include: [
      'react',
      'react-dom',
      'react/jsx-runtime',
      'react/jsx-dev-runtime',
      '@tanstack/react-query',
      'gsap',
      'gsap/CustomEase',
      'gsap/CustomBounce',
      'gsap/CustomWiggle',
      'lucide-react',
      'recharts',
    ],
  },
  resolve: {
    alias: {
      '@': resolve(__dirname, './src-ui'),
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'esnext',
    minify: false,
    sourcemap: true,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('node_modules')) {
            if (id.includes('recharts')) return 'vendor-recharts';
            if (id.includes('gsap')) return 'vendor-gsap';
            if (id.includes('@radix-ui')) return 'vendor-radix';
            if (id.includes('lucide-react')) return 'vendor-icons';
            return 'vendor';
          }
        },
      },
    },
  },
});
