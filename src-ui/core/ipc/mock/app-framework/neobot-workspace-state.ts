import type { PanelObject } from '../../../domain/apps/neobotWorkspace';

const scalar = (name: string, value: unknown, label = name) => ({
    name,
    label,
    path: [name],
    kind: 'scalar',
    type: typeof value === 'number' ? 'float' : typeof value === 'boolean' ? 'bool' : 'str',
    value,
    default: value,
});

export function createNeoBotWorkspaceState() {
    const config = {
        chat: {
            bot_account: '10001',
            bot_nick_name: 'Luna',
            bot_data: '你是群里的一位普通成员。',
            group_chat_chance: 0.3,
            admin_accounts: [],
        },
        adapter: { mode: 'onebot', reverse_ws_port: 8080 },
    };
    const schema = [
        {
            name: 'chat',
            label: '聊天与人设',
            path: ['chat'],
            kind: 'group',
            fields: [
                scalar('bot_account', '10001', '机器人 QQ'),
                scalar('bot_nick_name', 'Luna', '昵称'),
                scalar('bot_data', '你是群里的一位普通成员。', '人设'),
                scalar('group_chat_chance', 0.3, '群聊概率'),
                { name: 'admin_accounts', label: '超级管理员 QQ', kind: 'list', default: [] },
            ],
        },
        {
            name: 'adapter',
            label: '连接',
            path: ['adapter'],
            kind: 'group',
            fields: [scalar('mode', 'onebot', '协议'), scalar('reverse_ws_port', 8080, '监听端口')],
        },
    ];
    const entrySchema = [
        scalar('display_name', '', '显示名'),
        scalar('provider', 'DeepSeek', '供应商'),
        scalar('model_name', 'deepseek-chat', '模型名'),
        scalar('model_type', 'chat', '类型'),
        scalar('use_system_proxy', false, '使用系统代理'),
        { name: 'settings', label: '模型参数', kind: 'dict', default: {} },
        scalar('billing_script', '', '计费脚本'),
        { name: 'billing_config', label: '计费参数', kind: 'dict', default: {} },
    ];
    return {
        revision: 1,
        config,
        schema,
        source: '[chat]\nbot_account = "10001"\nbot_nick_name = "Luna"\nbot_data = "你是群里的一位普通成员。"\n',
        envRevision: 1,
        env: [
            {
                key: 'DeepSeek_URL',
                value: 'https://api.deepseek.com',
                has_value: true,
                in_file: true,
                sensitive: false,
            },
            { key: 'DeepSeek_APIKey', value: '', has_value: true, in_file: true, sensitive: true },
        ] as PanelObject[],
        platforms: [
            { name: 'DeepSeek', url: 'https://api.deepseek.com', has_key: true },
        ] as PanelObject[],
        library: [
            {
                model_ref: 'deepseek-chat',
                display_name: 'DeepSeek 对话',
                provider: 'DeepSeek',
                model_name: 'deepseek-chat',
                model_type: 'chat',
                api_key_configured: true,
                entry: {
                    display_name: 'DeepSeek 对话',
                    model_ref: 'deepseek-chat',
                    provider: 'DeepSeek',
                    model_name: 'deepseek-chat',
                    model_type: 'chat',
                    use_system_proxy: false,
                    settings: {},
                },
            },
        ] as PanelObject[],
        assignments: { chat_model: 'deepseek-chat', creator_image_models: [] } as PanelObject,
        entrySchema,
        prompts: [
            {
                name: 'reply',
                keys: [
                    {
                        path: 'system',
                        label: '系统提示词',
                        kind: 'template',
                        value: '你是 {nickname}。',
                        default: '你是 {nickname}。',
                        overridden: false,
                        placeholders: ['nickname'],
                    },
                ],
            },
        ] as PanelObject[],
        plugins: [
            {
                id: 'dashboard',
                name: 'dashboard',
                version: '1.2.4a1',
                official: true,
                enabled: true,
                manageable: false,
                status: 'loaded',
                hot_reload: false,
            },
            {
                id: 'demo',
                name: 'demo',
                version: '0.3.1',
                official: false,
                enabled: true,
                manageable: true,
                status: 'loaded',
                hot_reload: true,
                description: '预览插件',
            },
        ] as PanelObject[],
        pluginConfigs: new Map<string, PanelObject>(),
        proxy: { mode: 'system', host: '', port: 7890 } as PanelObject,
        archives: [
            {
                table_name: 'group_10001',
                key: 'overview',
                value: '群成员喜欢讨论开源项目和桌面工具。',
                tags: ['群聊'],
                version: 1,
                editable: true,
                total_chars: 21,
            },
        ] as PanelObject[],
        tasks: [
            {
                task_id: 'task-1',
                title: '每日提醒',
                detail: '提醒群成员休息',
                recurrence: 'daily',
                enabled: true,
                state: 'active',
                start_at_local: '2026-10-09T20:00',
                end_at_local: '2026-10-09T20:10',
                next_run: '2026-10-09 20:00',
                bindings: [{ kind: 'group', id: '10001' }],
                one_shot_notification: true,
            },
        ] as PanelObject[],
        standby: false,
        connectOnebot: true,
        historyCleared: false,
        configured: false,
    };
}

export type NeoBotWorkspaceState = ReturnType<typeof createNeoBotWorkspaceState>;
const states = new Map<string, NeoBotWorkspaceState>();
export const neoBotWorkspaceState = (id: string) => {
    let state = states.get(id);
    if (!state) {
        state = createNeoBotWorkspaceState();
        states.set(id, state);
    }
    return state;
};
