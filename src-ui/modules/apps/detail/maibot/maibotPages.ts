// MaiBot 详情侧栏的分组和各配置页铺哪些小节。侧栏中文名、校验路径落到哪一页都从这里推，
// 挪一个小节只改这一处。原始文件、日志由外壳追加到「实例」组末尾。

import type { FrameworkNavGroup } from '../frameworkUi';

export const MAIBOT_NAV: readonly FrameworkNavGroup[] = [
    { id: 'home', items: [{ value: 'overview', label: '概览' }] },
    {
        id: 'ai',
        label: 'AI',
        items: [
            { value: 'models', label: '模型' },
            { value: 'persona', label: '人格' },
            { value: 'prompts', label: '提示词' },
            { value: 'memory', label: '记忆' },
            { value: 'learning', label: '表达学习' },
        ],
    },
    {
        id: 'message',
        label: '消息',
        items: [
            { value: 'chat', label: '聊天名单' },
            { value: 'talk', label: '回复设置' },
            { value: 'rules', label: '会话规则' },
        ],
    },
    {
        // 麦麦自己攒下的数据：改了就落库，要它在跑
        id: 'resource',
        label: '资源',
        items: [
            { value: 'emoji', label: '表情包' },
            { value: 'expressions', label: '表达方式' },
            { value: 'jargon', label: '黑话' },
            { value: 'persons', label: '人物' },
            { value: 'knowledge', label: '知识库' },
        ],
    },
    {
        id: 'extend',
        label: '扩展',
        items: [
            { value: 'plugins', label: '插件' },
            { value: 'mcp', label: 'MCP' },
        ],
    },
    {
        id: 'instance',
        label: '实例',
        items: [
            { value: 'connection', label: '连接' },
            { value: 'advanced', label: '高级' },
        ],
    },
];

export const MAIBOT_TAB_LABEL: Readonly<Record<string, string>> = Object.fromEntries(
    MAIBOT_NAV.flatMap((g) => g.items).map((t) => [t.value, t.label]),
);

export type SchemaSectionDef = {
    /** bot_config 里的小节路径 */
    path: readonly string[];
    title: string;
    description?: string;
    /** 只出这几个字段；不给就整节 */
    fields?: readonly string[];
};

export type SchemaPageDef = {
    sections: readonly SchemaSectionDef[];
    /** 这页不出的字段（形状键，见 SchemaForm 的 shapeKey） */
    skip?: readonly string[];
    advanced?: readonly string[];
    /**
     * 和这页相关的数据页（学到的表达、记忆图谱、提示词模板…）。给 tab 的跳桌面端那一页；
     * 还没搬进桌面端的给 path，运行中开麦麦 WebUI 的那一页
     */
    links?: readonly MaiBotPageLink[];
};

/** tab 跳桌面端那一页，view 是那页里的哪一块（知识库的导入 / 图谱）；path 开麦麦 WebUI */
export type MaiBotPageLink = { label: string; tab: string; view?: string } | { label: string; path: string };

// 分时段频率和按聊天的提示词是「按会话」的规则，归会话规则页；回复设置页只放全局的
const RULE_FIELDS = [
    'chat.reply_timing.enable_talk_value_rules',
    'chat.reply_timing.talk_value_rules',
    'chat.reply_style.chat_prompts',
];

export const MAIBOT_SCHEMA_PAGES: Readonly<Record<string, SchemaPageDef>> = {
    persona: {
        sections: [
            { path: ['bot'], title: '身份' },
            { path: ['personality'], title: '人格' },
        ],
        // 适配器连上后会上报真实账号，这三项只在没上报时兜底，平时不用碰
        advanced: ['bot.platform', 'bot.qq_account', 'bot.platforms'],
        links: [
            { label: '提示词模板', tab: 'prompts' },
            { label: '麦麦认识的人', tab: 'persons' },
        ],
    },
    talk: {
        sections: [
            { path: ['chat'], title: '聊天' },
            { path: ['response_post_process'], title: '回复后处理' },
            { path: ['chinese_typo'], title: '错别字' },
            { path: ['response_splitter'], title: '分句' },
            { path: ['visual'], title: '图片' },
            { path: ['emoji'], title: '表情包' },
            { path: ['voice'], title: '语音' },
        ],
        skip: RULE_FIELDS,
        links: [{ label: '表情包库', tab: 'emoji' }],
    },
    rules: {
        sections: [
            {
                path: ['chat', 'reply_timing'],
                title: '分时段发言频率',
                fields: ['enable_talk_value_rules', 'talk_value_rules'],
            },
            { path: ['chat', 'reply_style'], title: '按聊天追加的提示词', fields: ['chat_prompts'] },
            { path: ['keyword_reaction'], title: '关键词反应' },
            { path: ['message_receive'], title: '消息过滤' },
        ],
    },
    learning: {
        sections: [
            { path: ['expression'], title: '表达方式' },
            { path: ['jargon'], title: '黑话' },
        ],
        links: [
            { label: '学到的表达方式', tab: 'expressions' },
            { label: '学到的黑话', tab: 'jargon' },
        ],
    },
    memory: {
        sections: [{ path: ['a_memorix'], title: '长期记忆' }],
        links: [
            { label: '记忆图谱', tab: 'knowledge', view: 'graph' },
            { label: '导入知识', tab: 'knowledge', view: 'import' },
        ],
    },
    mcp: { sections: [{ path: ['mcp'], title: 'MCP' }] },
    advanced: {
        sections: [
            { path: ['experimental'], title: '实验性功能' },
            { path: ['plugin'], title: '插件权限' },
            { path: ['plugin_runtime'], title: '插件运行时' },
            { path: ['webui'], title: 'WebUI' },
            { path: ['maim_message'], title: '消息服务' },
            { path: ['log'], title: '日志' },
            { path: ['debug'], title: '调试' },
            { path: ['telemetry'], title: '统计上报' },
            { path: ['database'], title: '数据库' },
        ],
        skip: [
            // 两个端口在「连接」页改，要和实例端口一起同步
            'webui.port',
            'maim_message.ws_server_port',
            // 桌面端运行中改配置、看状态都走 WebUI，关掉它这些就全断了
            'webui.enabled',
            // 麦麦由桌面端在后台拉起，没有可输入的终端
            'debug.enable_console_input',
        ],
        links: [{ label: '学到的行为', path: '/resource/behavior' }],
    },
};

// 路径前缀 → 页，最长的前缀赢：chat 整节在回复设置，其中两个规则字段在会话规则
const PREFIXES: readonly (readonly [string, string])[] = Object.entries(MAIBOT_SCHEMA_PAGES)
    .flatMap(([tab, page]) =>
        page.sections.flatMap((s) =>
            (s.fields ?? ['']).map((f) => [['bot', ...s.path, f].filter(Boolean).join('/'), tab] as const),
        ),
    )
    .sort((a, b) => b[0].length - a[0].length);

/** 校验路径跳到能改它的那一页 */
export function maibotTabForIssue(path: string): string {
    if (path.startsWith('adapter/chat/')) return 'chat';
    if (path.startsWith('adapter/')) return 'connection';
    if (path.startsWith('models/')) return 'models';
    if (path === 'bot/webui/port' || path === 'bot/maim_message/ws_server_port') return 'connection';
    const hit = PREFIXES.find(([prefix]) => path === prefix || path.startsWith(`${prefix}/`));
    return hit?.[1] ?? 'advanced';
}
