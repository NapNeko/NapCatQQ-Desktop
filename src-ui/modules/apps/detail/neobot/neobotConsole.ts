// NeoBot 面板（Web 控制台）的**功能地图**：桌面端不做重复建设，而是把「完整控制台能干什么」
// 标出来，用户知道去哪。每一条都对应面板的真实端点（见 dashboard/server.py 的路由表），
// 端点是给人对账用的：面板升级后如果这条对不上，说明地图该更新了。
//
// 状态三态：
//   desktop      桌面端已有等价能力
//   partial      桌面端有相邻能力，但不等价（例如只能改配置原文、只能停止）
//   consoleOnly  只有控制台能做

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
                state: 'consoleOnly',
                endpoints: ['/api/overview', '/api/stats/*', '/api/series/*', '/api/config/billing'],
            },
            {
                name: '服务与任务',
                desc: '后台服务清单、定时任务及其触发。',
                state: 'consoleOnly',
                endpoints: ['/api/services', '/api/tasks', '/api/scheduled-tasks'],
            },
            {
                name: '日志',
                desc: '面板内的实时日志缓冲，可按级别过滤。',
                state: 'desktop',
                endpoints: ['/api/logs'],
            },
            {
                name: '电源',
                desc: '进入待机 / 恢复运行 / 软重启运行 / 重启进程 / 关闭进程。',
                state: 'partial',
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
                state: 'partial',
                endpoints: ['/api/bots', '/api/bot/detail'],
            },
            {
                name: '模型库',
                desc: '模型清单、按用途指派、连通性测试、从供应商拉取模型列表。',
                state: 'consoleOnly',
                endpoints: ['/api/config/models', '/api/config/models/assignments', '/api/config/models/test'],
            },
            {
                name: '提示词',
                desc: '查看、预览、保存、重置各阶段提示词。',
                state: 'consoleOnly',
                endpoints: ['/api/prompts', '/api/prompts/preview', '/api/analysis/prompts'],
            },
            {
                name: '对话流',
                desc: '全链路流程图与漂移检查（NeoBot 独有的可视化）。',
                state: 'consoleOnly',
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
                state: 'consoleOnly',
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
                state: 'consoleOnly',
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
                state: 'partial',
                endpoints: ['/api/config', '/api/config/validate', '/api/config/reload'],
            },
            {
                name: '环境变量与平台密钥',
                desc: '模型与平台的密钥、地址；面板读取时脱敏显示。',
                state: 'consoleOnly',
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
