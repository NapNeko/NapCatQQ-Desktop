// 关于页「鸣谢」精选依赖清单。
// 只列用户可感知的主要运行时库，不扫全量 lockfile；增删依赖时按需改这里。

export interface CreditItem {
    /** 包名 / 项目名 */
    name: string;
    /** 一句话用途（用户向） */
    role: string;
    /** SPDX 或常见许可证简称 */
    license: string;
    /** 项目主页；有则右侧可打开 */
    url?: string;
}

export interface CreditGroup {
    title: string;
    items: readonly CreditItem[];
}

/** 关于页鸣谢分组（前端 / 桌面壳 / 后端）。 */
export const APP_CREDIT_GROUPS: readonly CreditGroup[] = [
    {
        title: '前端',
        items: [
            {
                name: 'React',
                role: '界面框架',
                license: 'MIT',
                url: 'https://react.dev/',
            },
            {
                name: 'Vite',
                role: '前端构建',
                license: 'MIT',
                url: 'https://vite.dev/',
            },
            {
                name: 'Tailwind CSS',
                role: '样式系统',
                license: 'MIT',
                url: 'https://tailwindcss.com/',
            },
            {
                name: 'Radix UI',
                role: '无障碍基础组件',
                license: 'MIT',
                url: 'https://www.radix-ui.com/',
            },
            {
                name: 'TanStack Query',
                role: '服务端状态与缓存',
                license: 'MIT',
                url: 'https://tanstack.com/query',
            },
            {
                name: 'GSAP',
                role: '动效',
                license: 'Standard',
                url: 'https://gsap.com/',
            },
            {
                name: 'Lucide',
                role: '图标',
                license: 'ISC',
                url: 'https://lucide.dev/',
            },
            {
                name: 'Plus Jakarta Sans / Inter / JetBrains Mono',
                role: '界面字体（Fontsource）',
                license: 'OFL',
                url: 'https://fontsource.org/',
            },
        ],
    },
    {
        title: '桌面壳',
        items: [
            {
                name: 'Tauri',
                role: '桌面壳与 IPC',
                license: 'Apache-2.0 / MIT',
                url: 'https://tauri.app/',
            },
            {
                name: 'Tokio',
                role: '异步运行时',
                license: 'MIT',
                url: 'https://tokio.rs/',
            },
            {
                name: 'serde',
                role: '序列化',
                license: 'Apache-2.0 / MIT',
                url: 'https://serde.rs/',
            },
        ],
    },
    {
        title: '后端与工具',
        items: [
            {
                name: 'tracing',
                role: '结构化日志',
                license: 'MIT',
                url: 'https://github.com/tokio-rs/tracing',
            },
            {
                name: 'thiserror',
                role: '错误类型',
                license: 'Apache-2.0 / MIT',
                url: 'https://docs.rs/thiserror',
            },
            {
                name: 'sysinfo',
                role: '本机进程与资源信息',
                license: 'MIT',
                url: 'https://docs.rs/sysinfo',
            },
            {
                name: 'ts-rs',
                role: 'Rust → TypeScript 类型导出',
                license: 'MIT',
                url: 'https://github.com/Aleph-Alpha/ts-rs',
            },
            {
                name: 'zip',
                role: '配置导入导出压缩',
                license: 'MIT',
                url: 'https://docs.rs/zip',
            },
        ],
    },
];
