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
        exclude: ['**/node_modules/**', '**/dist/**', '**/.references/**', '**/target/**'],
        passWithNoTests: false,
        restoreMocks: true,
        clearMocks: true,
        // 24 核机器默认按核数铺满 jsdom 环境会互相挤：waitFor 类断言成片超时（100+ 假红，
        // 单文件跑全绿）。8 线程实测全绿且更快。机器再快也别按核数放满。
        poolOptions: { threads: { maxThreads: 8, minThreads: 1 } },
        coverage: {
            provider: 'v8',
            // 测试地板只盯纯逻辑与 hooks 层；组件页由 vitest 行为测试 + 冒烟兜底
            include: ['core/domain/**', 'hooks/**'],
            // 2026-10 实测 lines 65.5 / funcs 76.6 / branches 84.3，地板取实测 -5pp 防回退
            thresholds: { lines: 60, functions: 71, branches: 79 },
            reporter: ['text-summary', 'html'],
        },
    },
    resolve: {
        alias: [
            { find: '@', replacement: uiRoot },
            // vitest 不认 module 字段，@gsap/react 会走 UMD 包去 require 另一份 CJS 的 gsap，
            // useGSAP 的 context 就收不到业务那份 gsap 建的动画。指到它的 ESM 入口，和打包时一样只有一份 gsap
            { find: /^@gsap\/react$/, replacement: '@gsap/react/src/index.js' },
        ],
    },
});
