import type { AppPanelResult } from '../../types';
import { record, records, text } from '../../../domain/apps/neobotWorkspace';
import { neoBotWorkspaceState } from './neobot-workspace-state';

export function mockNeoBotWorkspace(
    id: string,
    method: string,
    path: string,
    body?: unknown,
): AppPanelResult | undefined {
    const state = neoBotWorkspaceState(id);
    const url = new URL(path, 'http://preview.local');
    const route = url.pathname;
    const req = record(body);
    const ok = (data: unknown = {}) => ({
        kind: 'ok' as const,
        data: { ok: true, ...record(data) },
    });
    const fail = (message: string, status = 400, extra = {}) => ({
        kind: 'failed' as const,
        status,
        message,
        data: { ok: false, error: message, ...extra },
    });
    const document = () => ({
        revision: String(state.revision),
        config: structuredClone(state.config),
        schema: state.schema,
        source: state.source,
        form_supported: true,
        source_available: true,
        can_manage: true,
    });
    const env = () => ({
        revision: String(state.envRevision),
        items: structuredClone(state.env),
        platforms: state.platforms,
    });
    const models = () => ({
        revision: String(state.revision),
        library: structuredClone(state.library),
        assignments: {
            roles: state.assignments,
            creator_image_models: state.assignments.creator_image_models,
        },
        roles_meta: [
            { role: 'chat_model', label: '主对话', required: true },
            { role: 'creator_image_models', label: '生图', multi: true },
        ],
        entry_schema: state.entrySchema,
        provider_options: state.platforms.map((p) => p.name),
        platforms: state.platforms,
        can_manage: true,
    });
    if (route === '/api/auth/status')
        return ok({
            configured: state.configured,
            setup_allowed: !state.configured,
            loopback: true,
            version: '1.2.4a1',
        });
    if (route === '/api/auth/setup' && method === 'POST') {
        state.configured = true;
        return ok({ message: '预览：密码已设置' });
    }
    if (route === '/api/config' && method === 'GET') return ok(document());
    if (route === '/api/config' && method === 'POST') {
        if (String(req.revision) !== String(state.revision))
            return fail('配置已被其他会话修改', 409);
        if (req.mode === 'toml') state.source = text(req.source);
        else state.config = record(req.config) as typeof state.config;
        state.revision += 1;
        return ok({ ...document(), message: '配置已保存', applied: req.reload === true });
    }
    if (route === '/api/config/validate') return ok({ errors: [] });
    if (route === '/api/config/reload') return ok({ message: '配置已重载', applied: true });
    if (route === '/api/config/env' && method === 'GET') return ok(env());
    if (route === '/api/config/env' && method === 'POST') {
        if (String(req.revision) !== String(state.envRevision)) return fail('环境变量已变化', 409);
        state.env = state.env.filter(
            (i) => !Array.isArray(req.deletes) || !req.deletes.includes(i.key),
        );
        for (const [key, value] of Object.entries(record(req.updates))) {
            const item = state.env.find((i) => i.key === key);
            if (item?.sensitive && !value) continue;
            const next = {
                key,
                value: /key|token|secret/i.test(key) ? '' : text(value),
                sensitive: /key|token|secret/i.test(key),
                has_value: !!value,
                in_file: true,
            };
            if (item) Object.assign(item, next);
            else state.env.push(next);
        }
        state.envRevision += 1;
        return ok({ ...env(), message: '环境变量已保存', needs_restart: true });
    }
    if (route === '/api/config/env/platform') {
        if (String(req.revision) !== String(state.envRevision)) return fail('环境变量已变化', 409);
        const previous = state.platforms.find((p) => p.name === req.name);
        state.platforms = [
            ...state.platforms.filter((p) => p.name !== req.name),
            { name: req.name, url: req.url, has_key: !!req.api_key || previous?.has_key === true },
        ];
        state.envRevision += 1;
        return ok({ ...env(), message: '供应商已保存' });
    }
    if (route === '/api/config/models' && method === 'GET') return ok(models());
    if (route === '/api/config/models/test')
        return ok({
            reachable: true,
            authorized: true,
            model_found: true,
            latency_ms: 43,
            message: '预览连接成功',
        });
    if (route === '/api/config/models/provider-models')
        return ok({ models: ['deepseek-chat', 'deepseek-reasoner'] });
    if (route === '/api/config/models/library' || route === '/api/config/models/assignments') {
        if (String(req.revision) !== String(state.revision)) return fail('模型配置已变化', 409);
        if (route.endsWith('assignments')) state.assignments = record(req.assignments);
        else if (req.action === 'delete') {
            if (
                Object.values(state.assignments).some(
                    (ref) =>
                        ref === req.model_ref ||
                        (Array.isArray(ref) && ref.includes(req.model_ref)),
                )
            )
                return fail('模型仍被角色引用');
            state.library = state.library.filter((m) => m.model_ref !== req.model_ref);
        } else {
            const entry = record(req.entry);
            const modelRef = text(entry.model_ref) || `${text(entry.model_name)}-${state.revision}`;
            state.library = [
                ...state.library.filter((m) => m.model_ref !== modelRef),
                {
                    ...entry,
                    model_ref: modelRef,
                    entry: { ...entry, model_ref: modelRef },
                    api_key_configured: true,
                },
            ];
        }
        state.revision += 1;
        return ok({ ...models(), models: models(), message: '模型配置已保存' });
    }
    if (route === '/api/prompts' && method === 'GET')
        return ok({ sections: structuredClone(state.prompts), editable: true });
    if (route === '/api/prompts/preview')
        return ok({
            rendered: text(req.template).replace(
                /\{([^}]+)\}/g,
                (match, key: string) => text(record(req.values)[key]) || match,
            ),
            unresolved: [],
        });
    if (route === '/api/prompts/save' || route === '/api/prompts/reset') {
        const section = state.prompts.find((s) => s.name === req.section);
        const key = records(section?.keys).find((k) => k.path === req.path);
        if (key) {
            key.value = route.endsWith('reset') ? key.default : req.value;
            key.overridden = !route.endsWith('reset');
        }
        return ok({ message: '模板已保存' });
    }
    if (route === '/api/plugins' && method === 'GET')
        return ok({
            items: structuredClone(state.plugins),
            manage_enabled: true,
            console_plugin: 'dashboard',
            proxy: state.proxy,
        });
    if (route === '/api/extensions') return ok({ items: [] });
    if (route === '/api/plugins/proxy') {
        state.proxy = req;
        return ok({ proxy: state.proxy, message: '代理已保存' });
    }
    if (route === '/api/plugins/probe')
        return ok({
            existing: state.plugins.find((p) => p.id === url.searchParams.get('name')),
            official: false,
        });
    if (route === '/api/plugins/check-updates')
        return ok({
            items: [
                {
                    name: 'demo',
                    current_version: '0.3.1',
                    latest_version: '0.3.2',
                    update_available: true,
                },
            ],
        });
    if (route === '/api/plugins/install') {
        const name =
            text(req.repo)
                .split('/')
                .filter(Boolean)
                .at(-1)
                ?.replace(/\.git$/, '') || 'demo';
        const existing = state.plugins.find((p) => p.id === name);
        if (existing && !req.replace)
            return fail('插件已存在，请确认替换', 409, {
                conflict: {
                    existing,
                    requested: { name, repo: req.repo },
                    reserved: existing.official === true,
                },
            });
        if (req.dry_run) return ok({ dry_run: true, message: '可以安装', version: '1.0.0' });
        state.plugins = [
            ...state.plugins.filter((p) => p.id !== name),
            {
                id: name,
                name,
                version: '1.0.0',
                enabled: true,
                official: false,
                manageable: true,
                status: 'loaded',
                hot_reload: true,
            },
        ];
        return ok({ message: '插件已安装' });
    }
    const pluginMatch = route.match(
        /^\/api\/plugins\/([^/]+)\/(config|toggle|reload|update|uninstall)$/,
    );
    if (pluginMatch) {
        const name = decodeURIComponent(pluginMatch[1]);
        const op = pluginMatch[2];
        if (op === 'config') {
            const current = state.pluginConfigs.get(name) ?? {
                revision: '1',
                form_supported: true,
                source_available: true,
                config: { enabled: true, message: 'hello' },
                schema: [
                    {
                        name: 'enabled',
                        label: '启用',
                        kind: 'scalar',
                        type: 'bool',
                        value: true,
                        default: true,
                    },
                    {
                        name: 'message',
                        label: '消息',
                        kind: 'scalar',
                        type: 'str',
                        value: 'hello',
                        default: 'hello',
                    },
                ],
                source: 'enabled = true\nmessage = "hello"\n',
            };
            if (method === 'GET') return ok(current);
            if (req.revision !== current.revision) return fail('插件配置已变化', 409);
            const next = {
                ...current,
                revision: String(Number(current.revision) + 1),
                config: req.config ?? current.config,
                source: req.source ?? current.source,
            };
            state.pluginConfigs.set(name, next);
            return ok({ ...next, message: '插件配置已保存' });
        }
        const plugin = state.plugins.find((p) => p.id === name);
        if (op === 'toggle' && plugin) {
            plugin.enabled = !plugin.enabled;
            plugin.status = plugin.enabled ? 'loaded' : 'disabled';
        }
        if (op === 'update' && plugin) plugin.version = '0.3.2';
        if (op === 'uninstall') state.plugins = state.plugins.filter((p) => p.id !== name);
        return ok({ message: '插件操作完成' });
    }
    if (route === '/api/archives')
        return ok({
            items: [
                { table_name: 'group_10001', count: state.archives.length, over_limit_count: 0 },
            ],
            can_manage: true,
            delete_enabled: true,
            summarize_available: true,
            min_target_chars: 100,
            max_target_chars: 2000,
        });
    if (route === '/api/archives/over-limit') return ok({ items: [] });
    if (route === '/api/archives/items')
        return ok({
            items: state.archives.map((a) => ({ ...a, preview: text(a.value) })),
            has_more: false,
        });
    if (route === '/api/archives/item') {
        const key = method === 'GET' ? url.searchParams.get('key') : req.key;
        const item = state.archives.find((a) => a.key === key);
        if (!item) return fail('档案不存在', 404);
        if (method === 'GET') return ok(structuredClone(item));
        if (req.version !== item.version)
            return fail('档案已被修改', 409, { current: structuredClone(item) });
        if (method === 'DELETE') {
            state.archives = state.archives.filter((a) => a !== item);
            return ok({ message: '档案已删除' });
        }
        if (method === 'PUT') {
            Object.assign(item, {
                value: req.value,
                tags: req.tags,
                version: Number(item.version) + 1,
            });
            return ok({ ...item, message: '档案已保存' });
        }
    }
    if (route === '/api/archives/summarize' || route === '/api/archives/summarize/over-limit')
        return ok({
            task_id: 'summary-1',
            status: 'done',
            target_chars: req.target_chars ?? 1000,
            message: '预览压缩完成',
        });
    if (route === '/api/archives/snapshots')
        return ok({
            items: [
                { id: 1, created_at: '2026-10-09 12:00', chars_before: 1200, chars_after: 500 },
            ],
        });
    if (route === '/api/archives/snapshot')
        return ok({ snapshot: { id: 1, value: '压缩前的归档全文。' } });
    if (route === '/api/scheduled-tasks')
        return ok({ tasks: structuredClone(state.tasks), editable: true, available: true });
    if (route === '/api/scheduled-tasks/action') {
        const task = state.tasks.find((t) => t.task_id === req.task_uuid);
        if (req.action === 'create')
            state.tasks.push({
                ...req,
                task_id: `task-${Date.now()}`,
                state: 'active',
                enabled: true,
                start_at_local: req.start_at,
                end_at_local: req.end_at,
            });
        if (req.action === 'update' && task)
            Object.assign(task, req, { start_at_local: req.start_at, end_at_local: req.end_at });
        if (req.action === 'set_state' && task)
            Object.assign(task, { state: req.state, enabled: req.state === 'active' });
        if (req.action === 'set_notification_policy' && task)
            task.one_shot_notification = req.one_shot_notification;
        if (req.action === 'delete') state.tasks = state.tasks.filter((t) => t !== task);
        return ok({ message: '定时任务已更新' });
    }
    if (route.startsWith('/api/admin/')) {
        if (route.endsWith('/standby')) state.standby = true;
        if (route.endsWith('/resume') || route.endsWith('/reboot')) state.standby = false;
        if (route.endsWith('/onebot')) state.connectOnebot = req.enabled === true;
        return ok({
            available: true,
            state: state.standby ? 'standby' : 'running',
            standby: state.standby,
            transition: false,
            connect_onebot: state.connectOnebot,
        });
    }
    if (route === '/api/system')
        return ok({
            hostname: 'DESKTOP-PREVIEW',
            os: 'Windows 11',
            python_version: '3.13.5',
            cpu_percent: 6,
            process_memory_mb: 125,
            mem_used_mb: 12300,
            mem_total_mb: 32768,
            disk_percent: 40,
        });
    if (route === '/api/bot/detail')
        return ok({
            nickname: 'Luna',
            user_id: '10001',
            online: true,
            latency_ms: 42,
            today_messages: 128,
        });
    if (route === '/api/bots')
        return ok({
            items: [
                {
                    nickname: 'Luna',
                    user_id: '10001',
                    platform: 'OneBot',
                    status: 'online',
                    latency_ms: 42,
                },
            ],
        });
    if (route === '/api/services')
        return ok({
            items: [
                { name: 'memory', description: '记忆服务', available: true },
                { name: 'scheduler', description: '任务调度', available: true },
            ],
        });
    if (route === '/api/tasks')
        return ok({
            background: [{ name: 'group_10001', status: 'idle', pipeline_key: 'group_10001' }],
            scheduled: state.tasks,
        });
    if (route === '/api/series/messages' || route === '/api/series/latency')
        return ok({
            series: Array.from({ length: 24 }, (_, i) => ({
                at: `${i}:00`,
                count: (i * 13) % 80,
                ms: 20 + ((i * 7) % 40),
            })),
            current_ms: 42,
            avg_ms: 38,
            success_rate: 0.99,
        });
    if (route === '/api/stats/api-calls')
        return ok({
            items: [
                { action: 'send_group_msg', count: 125 },
                { action: 'get_group_member_info', count: 48 },
            ],
        });
    if (route === '/api/stats/active-users')
        return ok({ items: [{ user_id: '20002', nickname: 'QIAO', count: 87 }] });
    if (route === '/api/stats/usage' || route === '/api/series/usage')
        return ok({
            available: true,
            totals: { calls: 128, input_tokens: 24500, output_tokens: 3800, cost_cny: 1.28 },
            items: [
                {
                    module: 'chat',
                    provider_name: 'DeepSeek',
                    model_name: 'deepseek-chat',
                    calls: 128,
                    cost_cny: 1.28,
                },
            ],
            points: Array.from({ length: 24 }, (_, i) => ({
                at: `${i}:00`,
                calls: i + 1,
                input_tokens: i * 100,
                output_tokens: i * 20,
                cost_cny: i / 100,
            })),
        });
    if (route === '/api/stats/usage/records')
        return ok({
            items: [
                {
                    at: '2026-10-09 12:00',
                    module: 'chat',
                    provider_name: 'DeepSeek',
                    model_name: 'deepseek-chat',
                    input_tokens: 1200,
                    output_tokens: 200,
                    cost_cny: 0.01,
                    cost_source_kind: 'builtin',
                },
            ],
        });
    if (route === '/api/config/billing')
        return ok({
            available: true,
            enabled: true,
            scripts: ['example.py'],
            bindings: [{ model_key: 'deepseek-chat', source: 'builtin' }],
            policies: {},
        });
    if (route === '/api/config/billing/reload')
        return ok({ reloaded: req.scripts ?? ['example.py'], message: '脚本已重载' });
    if (route === '/api/config/billing/preview')
        return ok({
            cost_cny: 0.01,
            builtin_cost_cny: 0.01,
            source: req.billing_script ? 'script' : 'builtin',
            elapsed_ms: 1,
        });
    if (route === '/api/chat-flows')
        return ok({
            items: [
                {
                    display_name: '开发讨论群',
                    pipeline_key: 'group_10001',
                    model: 'deepseek-chat',
                    iterations: 1,
                    prompt_chars: 1250,
                    stale: false,
                },
            ],
        });
    if (route === '/api/chat-flows/detail')
        return ok({
            display_name: '开发讨论群',
            model: 'deepseek-chat',
            system_prompt: '你是 Luna。',
            messages: [
                { role: 'user', content: '今天讨论桌面端。' },
                { role: 'assistant', content: '模型页现在可以直接配置。' },
            ],
        });
    if (route === '/api/chat-flows/prompts')
        return ok({
            items: state.historyCleared
                ? []
                : [
                      { seq: 1, model: 'deepseek-chat', recorded_at: '2026-10-09 12:00' },
                      { seq: 2, model: 'deepseek-chat', recorded_at: '2026-10-09 12:01' },
                  ],
            storage_bytes: 4200,
            limit: 100,
        });
    if (route === '/api/chat-flows/prompt')
        return ok({
            entry: {
                model: 'deepseek-chat',
                recorded_at: '2026-10-09 12:00',
                messages: [
                    { role: 'system', content: '你是 Luna。' },
                    { role: 'user', content: `这是第 ${url.searchParams.get('seq')} 条消息。` },
                ],
                response: { content: '你好！' },
            },
        });
    if (route === '/api/chat-flows/prompts/clear') {
        state.historyCleared = true;
        return ok({ message: '已清空' });
    }
    if (route === '/api/analysis/prompts')
        return ok({
            rule: '按字符数估算 Token',
            available: true,
            agents: [
                {
                    name: 'ReplyAgent',
                    model: 'deepseek-chat',
                    total_chars: 1200,
                    total_tokens: 400,
                    parts: [
                        {
                            label: '系统提示词',
                            kind: 'system',
                            chars: 1200,
                            tokens: 400,
                            text: '你是 Luna。',
                        },
                    ],
                },
            ],
        });
    return undefined;
}
