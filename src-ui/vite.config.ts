import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { resolve } from 'path';

// Tauri 生产环境用自定义协议加载前端；base 必须相对路径，
// 否则 index.html 写成 /assets/...，WebView 里脚本 404，表现为「前端崩溃」。
const isTauriWindows =
  process.env.TAURI_ENV_PLATFORM === 'windows' ||
  process.env.TAURI_PLATFORM === 'windows';

// 配置位于 src-ui/；仓库根为 monorepo 工作区（package.json / Cargo / dist）。
const repoRoot = resolve(__dirname, '..');

export default defineConfig({
  // 相对 base：生产 asset 协议 + dev 都可用
  base: './',
  root: __dirname,
  plugins: [react(), tailwindcss()],
  define: {
    // dev 启动即拉全量组件 detect（含远端 SSH）会拖慢首屏；需要测组件页时设 VITE_SKIP_COMPONENTS_WARMUP=0
    'import.meta.env.VITE_SKIP_COMPONENTS_WARMUP': JSON.stringify(
      process.env.VITE_SKIP_COMPONENTS_WARMUP ?? '1',
    ),
  },
  clearScreen: false,
  envPrefix: ['VITE_', 'TAURI_'],
  // .env 仍放在仓库根，与既有本地开发习惯一致。
  envDir: repoRoot,
  server: {
    port: 1420,
    strictPort: true,
    hmr: { overlay: true },
    fs: {
      allow: [repoRoot],
    },
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
      '@': resolve(__dirname),
    },
  },
  build: {
    // 产物仍输出到仓库根 dist，供 src-tauri frontendDist 使用。
    outDir: resolve(repoRoot, 'dist'),
    emptyOutDir: true,
    target: isTauriWindows ? 'chrome105' : 'esnext',
    minify: process.env.TAURI_ENV_DEBUG === 'true' ? false : 'esbuild',
    sourcemap: process.env.TAURI_ENV_DEBUG === 'true',
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
          return undefined;
        },
      },
    },
  },
});
