// src-ui 分层门禁：规则权威是 docs/context/frontend.md §2，这里只是它的机器化投影。
// 分层违规 = error 直接挡；已登记偏差（frontend.md「现存偏差」清单）按文件降级 warn，
// 逐个消解后删对应 overrides。quality sweep 记录见 .claude/plan/ui-quality-sweep.md。
import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';
import globals from 'globals';

const UI = 'src-ui';

// 只允许出现在 core/ipc/transport.ts 的能力 import
const TAURI_PACKAGES = ['@tauri-apps/*'];

const LAYER_MSG =
    '分层违规：见 docs/context/frontend.md §2。modules/shared/app 只能 import hooks/*、core/domain/*、core/ipc/types、core/ipc/generated/**、shared/*。';

const uiFiles = [`${UI}/**/*.ts`, `${UI}/**/*.tsx`];

const layerBan = (patterns, extra = []) => [
    {
        group: [...patterns, ...extra],
        message: LAYER_MSG,
    },
];

export default tseslint.config(
    {
        ignores: [
            'node_modules/**',
            'dist/**',
            `${UI}/core/ipc/generated/**`,
            `${UI}/vite.config.ts`,
            `${UI}/vitest.config.ts`,
            'src-tauri/**',
            'crates/**',
            'scripts/**',
            '.references/**',
            '.claude/**',
            '.kiro/**',
            '.codex/**',
            'docs/**',
        ],
    },
    js.configs.recommended,
    ...tseslint.configs.recommended,
    {
        files: uiFiles,
        languageOptions: {
            ecmaVersion: 2022,
            sourceType: 'module',
            globals: { ...globals.browser },
        },
        plugins: {
            'react-hooks': reactHooks,
            'react-refresh': reactRefresh,
        },
        rules: {
            ...reactHooks.configs.recommended.rules,
            // react-hooks v6 的 React Compiler 家族规则（set-state-in-effect / refs /
            // immutability / purity / preserve-manual-memoization / globals）：本仓库未启用
            // React Compiler，这些规则对现有代码是 300+ 处口径外报告，不进门禁；
            // 是否逐条采纳属功能迭代时的事。rules-of-hooks 保持 error。
            'react-hooks/set-state-in-effect': 'off',
            'react-hooks/refs': 'off',
            'react-hooks/immutability': 'off',
            'react-hooks/purity': 'off',
            'react-hooks/preserve-manual-memoization': 'off',
            'react-hooks/globals': 'off',
            'react-hooks/exhaustive-deps': 'warn',
            'react-refresh/only-export-components': ['warn', { allowConstantExport: true }],
            '@typescript-eslint/no-unused-vars': 'off', // tsc noUnusedLocals/Parameters 已管
            '@typescript-eslint/no-explicit-any': 'off', // 存量噪音大，暂不进门禁
            'no-empty': ['error', { allowEmptyCatch: true }], // .catch(() => {}) 另有审计，不在 lint 误伤
        },
    },
    // vitest / node 上下文
    {
        files: [`${UI}/**/*.test.ts`, `${UI}/**/*.test.tsx`, `${UI}/**/*.mock.ts`],
        languageOptions: { globals: { ...globals.browser, ...globals.node } },
    },
    // transport：唯一允许碰 @tauri-apps 与 mock 兜底的地方
    {
        files: [`${UI}/core/ipc/transport.ts`],
        rules: { 'no-restricted-imports': 'off' },
    },
    // modules / shared / app：可推倒层，禁直连 IPC 底层与 services（error = 门禁）
    {
        files: [`${UI}/modules/**`, `${UI}/shared/**`, `${UI}/app/**`],
        rules: {
            'no-restricted-imports': [
                'error',
                {
                    patterns: layerBan(
                        [
                            `${UI}/core/services/*`,
                            '../core/services/*',
                            '../../core/services/*',
                            '../../../core/services/*',
                            '../../../../core/services/*',
                            '**/core/services/*',
                            '**/core/ipc/transport',
                            '**/core/ipc/mock/*',
                            '@/core/services/*',
                            '@/core/ipc/transport',
                            '@/core/ipc/mock/*',
                        ],
                        TAURI_PACKAGES,
                    ),
                },
            ],
        },
    },
    // hooks：只许 services/domain，禁 @tauri-apps、禁反向伸 modules
    // （error = 门禁；settings-draft 双向纠缠属已登记偏差，见文末 override）
    {
        files: [`${UI}/hooks/**`],
        rules: {
            'no-restricted-imports': [
                'error',
                {
                    patterns: layerBan(
                        [
                            '**/core/ipc/transport',
                            '**/core/ipc/mock/*',
                            '**/modules/**',
                            '@/modules/**',
                            '../../modules/**',
                            '../modules/**',
                        ],
                        TAURI_PACKAGES,
                    ),
                },
            ],
        },
    },
    // hooks 对 transport 的 shell 依赖（openExternalUrl/pickDirectory/onFileDragDrop）
    // 是文件对话框与外链打开的薄封装，frontend.md「现存偏差」登记；逐文件收敛前 warn 可见。
    {
        files: [
            `${UI}/hooks/apps/useAppInstances.ts`,
            `${UI}/hooks/useOpenExternal.ts`,
            `${UI}/hooks/usePickDirectory.ts`,
            `${UI}/hooks/webui/useOpenSnowlumaNovnc.ts`,
            `${UI}/hooks/webui/useOpenWebui.ts`,
            `${UI}/hooks/ui/useTauriFileDrop.ts`,
        ],
        rules: { 'no-restricted-imports': 'warn' },
    },
    // useBackendSettings <-> settings-draft 双向纠缠：settings 草稿体系重构前维持 warn
    // （frontend.md「现存偏差」登记；settings-draft 也反向 import hooks/* store）。
    {
        files: [`${UI}/hooks/preferences/useBackendSettings.ts`],
        rules: { 'no-restricted-imports': 'warn' },
    },
    // bot/settings 直连 services 的存量文件（frontend.md「现存偏差」白名单）：
    // 逐文件拆分是独立重构，本门禁对它们保持 warn 可见，新增文件按 error 拦。
    {
        files: [
            `${UI}/modules/bot/config/BotConfigPage.next.tsx`,
            `${UI}/modules/bot/dialogs/ImportRemoteBotsDialog.tsx`,
            `${UI}/modules/bot/list/BotListPage.next.tsx`,
            `${UI}/modules/settings/DataRootMigrateDialog.tsx`,
            `${UI}/modules/settings/settings-draft.ts`,
            `${UI}/modules/settings/tabs/NcdWatchRemoteSection.tsx`,
            `${UI}/modules/settings/tabs/NotificationsTab.tsx`,
            `${UI}/modules/settings/tabs/WindowTab.tsx`,
        ],
        rules: { 'no-restricted-imports': 'warn' },
    },
    // core/domain：零运行时依赖
    {
        files: [`${UI}/core/domain/**`],
        rules: {
            'no-restricted-imports': [
                'error',
                {
                    patterns: [
                        {
                            group: [
                                'react',
                                'react-*',
                                '@react*/*',
                                '@tanstack/*',
                                '@tauri-apps/*',
                                '**/core/services/*',
                                '@/core/services/*',
                                '**/core/ipc/transport',
                                '**/hooks/**',
                                '**/modules/**',
                                '**/shared/**',
                            ],
                            message:
                                'core/domain 零运行时依赖：只放纯函数 + 类型 + reducer + 文案表，可 import core/ipc/types 与 generated（frontend.md §2）。',
                        },
                    ],
                },
            ],
        },
    },
    // core/services：禁 @tauri-apps 直连（transport 除外的那份豁免只给了 transport.ts；
    // desktop.service.ts 的动态 import 属已登记偏差，动态 import 不受本规则约束）
    {
        files: [`${UI}/core/services/**`],
        rules: {
            'no-restricted-imports': [
                'error',
                {
                    patterns: [
                        { group: TAURI_PACKAGES, message: 'Tauri API 只许 core/ipc/transport.ts import。' },
                        {
                            group: ['**/modules/**', '**/hooks/**', '@/modules/**'],
                            message: 'services 层不得依赖 UI 层。',
                        },
                    ],
                },
            ],
        },
    },
    // 测试/mock 允许 import services 做 vi.mock 断言、import 任意被测层
    {
        files: [`${UI}/**/*.test.ts`, `${UI}/**/*.test.tsx`, `${UI}/**/*.mock.ts`],
        rules: { 'no-restricted-imports': 'off' },
    },
    {
        files: [`${UI}/main.tsx`, `${UI}/chat-main.ts`],
        languageOptions: { globals: { ...globals.browser } },
    },
);
