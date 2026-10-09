// NeoBot 自带 WebUI 面板的假回包：按真机 /api/* 的路径逐条备料，
// 其余路径如实报「未模拟」，免得预览里看着像通了、真机却是空的。
import type { AppPanelResult } from '../../types';
import { withMockDelay } from '../bootstrap.mock';
import { mockNeoBotWorkspace } from './neobot-workspace';

/** 预览里「记住的面板密码」：只记有没有，不存明文 */
const panelPasswords = new Set<string>();

export const neobotPanelApi = {
    panelCall: async (
        _id: string,
        method: string,
        path: string,
        _body?: unknown,
    ): Promise<AppPanelResult | null> => {
        const ok = (data: unknown) => withMockDelay({ kind: 'ok' as const, data });
        const workspace = mockNeoBotWorkspace(_id, method, path, _body);
        if (workspace) return withMockDelay(workspace);
        if (method === 'POST') {
            // 插件的几个动作：预览里只要「面板收下了」就够，列表不变也没关系
            const panelAction =
                path === '/api/deploy/onebot-token' ||
                path === '/api/plugins/install' ||
                /^\/api\/plugins\/[^/]+\/(toggle|reload|uninstall)$/.test(path);
            if (panelAction) {
                return withMockDelay({
                    kind: 'ok' as const,
                    data: { ok: true, message: '预览：已提交' },
                });
            }
            return withMockDelay({
                kind: 'failed' as const,
                message: '预览未模拟 ' + method + ' ' + path,
            });
        }
        if (method !== 'GET') {
            return withMockDelay({
                kind: 'failed' as const,
                message: '预览未模拟 ' + method + ' ' + path,
            });
        }
        switch (path) {
            case '/api/deploy/status':
                return ok({
                    ok: true,
                    ready: false,
                    revision: 'rev-1',
                    env_revision: 'env-1',
                    steps: [
                        {
                            key: 'bot_identity',
                            label: '机器人身份',
                            required: true,
                            done: true,
                            hint: '机器人 QQ 号与昵称都是必填',
                        },
                        {
                            key: 'persona',
                            label: '人设',
                            required: true,
                            done: false,
                            hint: '写清你是谁、怎么说话',
                        },
                        {
                            key: 'platform_key',
                            label: '平台密钥',
                            required: true,
                            done: false,
                            hint: '至少填 DeepSeek_APIKey',
                        },
                        {
                            key: 'onebot',
                            label: 'OneBot 连接',
                            required: true,
                            done: false,
                            hint: '配好监听端口与 access token',
                        },
                        {
                            key: 'admin',
                            label: '超级管理员（选填）',
                            required: false,
                            done: false,
                            hint: '接收余额不足等通知',
                        },
                    ],
                    onebot: {
                        host: '0.0.0.0',
                        port: 8080,
                        url_local: 'ws://127.0.0.1:8080/onebot/v11/ws',
                        url_lan: 'ws://192.168.1.10:8080/onebot/v11/ws',
                        token: '',
                        token_enabled: false,
                        path_hint: '/onebot',
                        warning: '监听 0.0.0.0 但没有 access token：局域网内任何人都能连上来',
                    },
                    defaults: {
                        bot_account: '0',
                        bot_nick_name: 'NeoBot',
                        bot_data: '',
                    },
                    values: {
                        bot_account: '10001',
                        bot_nick_name: 'Luna',
                        bot_data: '',
                        alias_name: [],
                        admin_accounts: [],
                        group_chat_chance: 0.3,
                    },
                });
            case '/api/auth/status':
                // 预览里当作「还没有面板密码」：正好能看凭据卡片给的引导文案
                return ok({
                    ok: true,
                    configured: false,
                    setup_required: true,
                    setup_allowed: true,
                    loopback: true,
                    version: '1.2.3',
                });
            case '/api/overview':
                return ok({
                    ok: true,
                    online: true,
                    app_name: 'NeoBot',
                    app_version: '1.2.3',
                    bot_nickname: 'Luna',
                    bot_user_id: 10001,
                    avatar_url: '',
                    uptime_seconds: 8130,
                    today_messages: 128,
                    total_messages: 20461,
                    plugins_loaded: 6,
                    plugins_total: 7,
                    plugins_error: 1,
                    latency_ms: 42,
                    python_version: '3.13.5',
                    hostname: 'DESKTOP-LUNA',
                    standby: false,
                    notices: [
                        {
                            level: 'warning',
                            text: '尚未配置超级管理员账号，可能影响部分命令使用',
                            hint: '在「配置管理 → 本体配置 → chat」里填 admin_accounts（QQ 号列表）',
                        },
                    ],
                });
            case '/api/plugins':
                return ok({
                    ok: true,
                    manage_enabled: true,
                    console_plugin: 'dashboard',
                    items: [
                        {
                            id: 'dashboard',
                            name: 'dashboard',
                            version: '1.2.3',
                            status: '运行中',
                            state: 'running',
                            enabled: true,
                            description: '网页面板本体',
                            official: true,
                            manageable: false,
                            tags: ['official', 'web'],
                            python_dependencies: [],
                            missing_python_dependencies: [],
                            config_path: '',
                        },
                        {
                            id: 'demo-plugin',
                            name: 'demo-plugin',
                            version: '0.3.1',
                            status: '已加载',
                            state: 'loaded',
                            enabled: true,
                            description: '示例第三方插件',
                            author: 'someone',
                            official: false,
                            manageable: true,
                            repo: 'https://github.com/someone/demo-plugin',
                            tags: ['demo'],
                            python_dependencies: ['httpx'],
                            missing_python_dependencies: ['httpx'],
                            config_path: 'D:/ncd/apps/neobot/plugins_data/demo-plugin/config.toml',
                        },
                    ],
                });
            case '/api/config/models':
                return ok({
                    ok: true,
                    can_manage: true,
                    platforms: {
                        DeepSeek: {
                            name: 'DeepSeek',
                            url: 'https://api.deepseek.com',
                            has_key: true,
                        },
                    },
                    assignments: { chat: 'deepseek-chat', vision: 'deepseek-vl' },
                    library: [
                        {
                            model_ref: 'deepseek-chat',
                            display_name: 'DeepSeek 对话',
                            provider: 'DeepSeek',
                            model_name: 'deepseek-chat',
                            model_type: 'chat',
                            type_label: '对话',
                        },
                        {
                            model_ref: 'deepseek-vl',
                            display_name: 'DeepSeek 视觉',
                            provider: 'DeepSeek',
                            model_name: 'deepseek-vl',
                            model_type: 'vision',
                            type_label: '视觉',
                        },
                        {
                            model_ref: 'silicon-chat',
                            display_name: 'SiliconFlow 对话',
                            provider: 'SiliconFlow',
                            model_name: 'Qwen/Qwen2.5-7B-Instruct',
                            model_type: 'chat',
                            type_label: '对话',
                        },
                    ],
                });
            case '/api/prompts':
                return ok({
                    ok: true,
                    editable: true,
                    sections: [
                        {
                            name: 'reply',
                            keys: [
                                {
                                    path: 'system',
                                    label: '系统提示词',
                                    kind: 'template',
                                    value: '你是 {nickname}，一个群里的普通成员。',
                                    default: '你是 {nickname}。',
                                    custom: '你是 {nickname}，一个群里的普通成员。',
                                    overridden: true,
                                    placeholders: ['nickname'],
                                },
                            ],
                        },
                        {
                            name: 'memory',
                            keys: [
                                {
                                    path: 'summary_hint',
                                    label: '摘要提示词',
                                    kind: 'scalar',
                                    value: '把要点压到 200 字以内。',
                                    default: '',
                                    custom: null,
                                    overridden: false,
                                    placeholders: [],
                                },
                            ],
                        },
                    ],
                });
            case '/api/archives':
                return ok({
                    ok: true,
                    can_manage: true,
                    summarize_available: true,
                    min_target_chars: 200,
                    max_target_chars: 2000,
                    items: [
                        { table_name: 'group_10001', count: 412, over_limit: 0, max_chars: 2000 },
                        { table_name: 'private_20002', count: 88, over_limit: 3, max_chars: 2000 },
                    ],
                });
            default:
                return withMockDelay({
                    kind: 'failed' as const,
                    message: '预览未模拟 ' + method + ' ' + path,
                });
        }
    },

    setPanelPassword: async (id: string, password: string): Promise<void> => {
        if (password.trim()) panelPasswords.add(id);
        else panelPasswords.delete(id);
    },

    panelPasswordSet: async (id: string): Promise<boolean> => withMockDelay(panelPasswords.has(id)),
};
