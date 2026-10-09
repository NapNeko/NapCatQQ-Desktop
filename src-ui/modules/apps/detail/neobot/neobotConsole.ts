// NeoBot 上游管理端点与桌面端覆盖范围。

export type ConsoleFeatureState = 'desktop' | 'partial' | 'consoleOnly';

export interface ConsoleFeature {
    name: string;
    desc: string;
    state: ConsoleFeatureState;
    /** 面板端点前缀，用于对账 */
    endpoints: readonly string[];
}

export interface ConsoleFeatureGroup {
    id: string;
    title: string;
    items: readonly ConsoleFeature[];
}

export const CONSOLE_STATE_LABEL: Readonly<Record<ConsoleFeatureState, string>> = {
    desktop: '桌面端已有',
    partial: '部分',
    consoleOnly: '仅控制台',
};

export const NEOBOT_CONSOLE_FEATURES: readonly ConsoleFeatureGroup[] = [
    {
        id: 'runtime',
        title: '运行',
        items: [
            {
                name: '概览与统计',
                desc: '消息数与延迟曲线、API 调用量、活跃用户、用量与计费。',
                state: 'desktop',
                endpoints: [
                    '/api/overview',
                    '/api/stats/*',
                    '/api/series/*',
                    '/api/config/billing',
                ],
            },
            {
                name: '服务与任务',
                desc: '后台服务、任务清单与定时任务创建、编辑、启停和删除。',
                state: 'desktop',
                endpoints: ['/api/services', '/api/tasks', '/api/scheduled-tasks'],
            },
            {
                name: '日志',
                desc: '面板里的实时日志，可按调试 / 信息 / 警告 / 错误过滤；桌面端「日志」页看的是同一份输出。',
                state: 'desktop',
                endpoints: ['/api/logs'],
            },
            {
                name: '电源',
                desc: '进入待机 / 恢复运行 / 软重启运行 / 重启进程 / 关闭进程。',
                state: 'desktop',
                endpoints: ['/api/admin/power', '/api/admin/standby', '/api/admin/shutdown'],
            },
        ],
    },
    {
        id: 'ai',
        title: '对话与 AI',
        items: [
            {
                name: 'Bot 与连接',
                desc: 'Bot 清单、单个 Bot 详情与 OneBot 连接状态。',
                state: 'desktop',
                endpoints: ['/api/bots', '/api/bot/detail'],
            },
            {
                name: '模型库',
                desc: '模型清单、按用途指派、连通性测试、从供应商拉取模型列表。',
                state: 'desktop',
                endpoints: [
                    '/api/config/models',
                    '/api/config/models/assignments',
                    '/api/config/models/test',
                ],
            },
            {
                name: '提示词',
                desc: '查看、预览、保存、重置各阶段提示词。',
                state: 'desktop',
                endpoints: ['/api/prompts', '/api/prompts/preview', '/api/analysis/prompts'],
            },
            {
                name: '对话流',
                desc: '最近请求、完整提示词历史、跟随最新、记录对比与清空。',
                state: 'desktop',
                endpoints: ['/api/chat-flows', '/api/chat-flows/detail'],
            },
        ],
    },
    {
        id: 'data',
        title: '数据',
        items: [
            {
                name: '记忆与归档',
                desc: '会话归档的浏览与编辑、摘要生成、快照查看。',
                state: 'desktop',
                endpoints: ['/api/archives', '/api/archives/item', '/api/archives/summarize'],
            },
        ],
    },
    {
        id: 'extend',
        title: '扩展',
        items: [
            {
                name: '插件',
                desc: '贴一个 GitHub 仓库地址安装第三方插件，启停、重载、查更新、卸载，以及插件自身的配置。',
                state: 'desktop',
                endpoints: ['/api/plugins', '/api/plugins/install', '/api/plugins/probe'],
            },
        ],
    },
    {
        id: 'config',
        title: '配置',
        items: [
            {
                name: '本体配置',
                desc: '按分区树编辑 config.toml，带校验与热重载。',
                state: 'desktop',
                endpoints: ['/api/config', '/api/config/validate', '/api/config/reload'],
            },
            {
                name: '环境变量与平台密钥',
                desc: '模型与平台的密钥、地址；面板读取时脱敏显示。',
                state: 'desktop',
                endpoints: ['/api/config/env', '/api/config/env/platform'],
            },
        ],
    },
];

/** 功能地图里一共有多少条，以及各状态多少条（页面顶部给一句汇总） */
export function consoleFeatureStats(
    groups: readonly ConsoleFeatureGroup[] = NEOBOT_CONSOLE_FEATURES,
): { total: number; byState: Record<ConsoleFeatureState, number> } {
    const byState: Record<ConsoleFeatureState, number> = { desktop: 0, partial: 0, consoleOnly: 0 };
    let total = 0;
    for (const g of groups) {
        for (const item of g.items) {
            total += 1;
            byState[item.state] += 1;
        }
    }
    return { total, byState };
}
