// AstrBot 详情侧栏的分组，也是各页中文名的唯一来源：「去『模型』页」这类跳转文案从这里取，
// 改名只改这一处。原始文件、日志由外壳追加到「实例」组末尾。

import type { FrameworkNavGroup } from '../frameworkUi';

export const ASTRBOT_NAV: readonly FrameworkNavGroup[] = [
    { id: 'home', items: [{ value: 'overview', label: '概览' }] },
    {
        id: 'ai',
        label: 'AI',
        items: [
            { value: 'models', label: '模型' },
            { value: 'persona', label: '人格' },
            { value: 'kb', label: '知识库' },
            { value: 'subagent', label: '子代理' },
        ],
    },
    {
        id: 'message',
        label: '消息',
        items: [
            { value: 'talk', label: '回复设置' },
            { value: 'rules', label: '会话规则' },
        ],
    },
    { id: 'extend', label: '扩展', items: [{ value: 'plugins', label: '插件' }] },
    { id: 'instance', label: '实例', items: [{ value: 'connections', label: '连接与账号' }] },
];

export const ASTRBOT_TAB_LABEL: Readonly<Record<string, string>> = Object.fromEntries(
    ASTRBOT_NAV.flatMap((g) => g.items).map((t) => [t.value, t.label]),
);
