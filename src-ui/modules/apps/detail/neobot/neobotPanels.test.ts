import { describe, expect, it } from 'vitest';
import {
    parseNeoBotArchives,
    parseNeoBotAuthStatus,
    parseNeoBotModels,
    parseNeoBotPlugins,
    parseNeoBotPrompts,
} from './neobotPanels';

// 回包结构取自面板 handler（api.py 的 _plugin_payload / config_models / prompts / archives），
// 不是编的。

describe('parseNeoBotPlugins', () => {
    it('认得真实回包并保留关键字段', () => {
        const p = parseNeoBotPlugins({
            ok: true,
            manage_enabled: true,
            console_plugin: 'dashboard',
            items: [
                {
                    id: 'demo',
                    name: 'demo',
                    version: '0.3.1',
                    status: '已加载',
                    enabled: true,
                    official: false,
                    manageable: true,
                    missing_python_dependencies: ['httpx'],
                    tags: ['a', 'b'],
                },
                {
                    name: 'dashboard',
                    official: true,
                    manageable: false,
                    tags: [],
                    missing_python_dependencies: [],
                },
            ],
        })!;
        expect(p.items).toHaveLength(2);
        expect(p.items[0].id).toBe('demo');
        expect(p.items[0].missingPythonDependencies).toEqual(['httpx']);
        expect(p.items[1].official).toBe(true);
        expect(p.consolePlugin).toBe('dashboard');
        expect(p.manageEnabled).toBe(true);
    });

    it('没有 items 数组就返回 null（当 malformed，而不是渲染空列表）', () => {
        expect(parseNeoBotPlugins({ ok: true })).toBeNull();
        expect(parseNeoBotPlugins(null)).toBeNull();
    });

    it('id 缺失时回落到 name', () => {
        const p = parseNeoBotPlugins({ items: [{ name: 'only-name' }] })!;
        expect(p.items[0].id).toBe('only-name');
    });
});

describe('parseNeoBotModels', () => {
    it('按 platforms 标出「供应商没配 Key」', () => {
        const m = parseNeoBotModels({
            library: [
                {
                    model_ref: 'a',
                    provider: 'DeepSeek',
                    model_name: 'deepseek-chat',
                    model_type: 'chat',
                },
                { model_ref: 'b', provider: 'SiliconFlow', model_name: 'qwen' },
            ],
            platforms: { DeepSeek: { has_key: true } },
            assignments: { chat: 'a', vision: null },
        })!;
        expect(m.library).toHaveLength(2);
        expect(m.library[0].providerHasKey).toBe(true);
        expect(m.library[1].providerHasKey).toBe(false);
        // type_label 缺失时回落到 model_type
        expect(m.library[0].typeLabel).toBe('chat');
        // 分配只收非空字符串
        expect(m.assignments).toEqual({ chat: 'a' });
    });

    it('没有 library 就 null；条目缺 model_ref 被丢掉', () => {
        expect(parseNeoBotModels({ assignments: {} })).toBeNull();
        const m = parseNeoBotModels({ library: [{ provider: 'X' }, { model_ref: 'ok' }] })!;
        expect(m.library.map((x) => x.modelRef)).toEqual(['ok']);
    });
});

describe('parseNeoBotPrompts', () => {
    it('摊平分区的键，并保留「是否被覆盖过」', () => {
        const p = parseNeoBotPrompts({
            editable: true,
            sections: [
                {
                    name: 'reply',
                    keys: [
                        {
                            path: 'system',
                            label: '系统提示词',
                            kind: 'template',
                            value: 'v',
                            overridden: true,
                            placeholders: ['nickname'],
                        },
                        { path: 'x', value: 123 },
                    ],
                },
            ],
        })!;
        expect(p.sections).toHaveLength(1);
        expect(p.sections[0].keys[0].overridden).toBe(true);
        expect(p.sections[0].keys[0].placeholders).toEqual(['nickname']);
        // value 不是字符串就当空串，label 缺失回落到 path
        expect(p.sections[0].keys[1].value).toBe('');
        expect(p.sections[0].keys[1].label).toBe('x');
    });

    it('没有 sections 就 null', () => {
        expect(parseNeoBotPrompts({ editable: true })).toBeNull();
    });
});

describe('parseNeoBotArchives', () => {
    it('表名两个键名都认，并带出超限数', () => {
        const a = parseNeoBotArchives({
            summarize_available: true,
            items: [
                { table_name: 'g1', count: 412, over_limit: 0 },
                { name: 'p1', count: 88, over_limit: 3 },
            ],
        })!;
        expect(a.tables.map((x) => x.name)).toEqual(['g1', 'p1']);
        expect(a.tables[1].overLimit).toBe(3);
        expect(a.summarizeAvailable).toBe(true);
    });

    it('没有 items 就 null', () => {
        expect(parseNeoBotArchives({ ok: true })).toBeNull();
    });
});

describe('parseNeoBotAuthStatus', () => {
    it('未设密码且本机可设：能据此提示用户去面板设置', () => {
        const s = parseNeoBotAuthStatus({
            ok: true,
            configured: false,
            setup_required: true,
            setup_allowed: true,
            loopback: true,
            version: '1.2.3',
        })!;
        expect(s.configured).toBe(false);
        expect(s.setupAllowed).toBe(true);
        expect(s.version).toBe('1.2.3');
    });

    it('未设密码但本机不可设（远端实例）：setup_allowed 为 false', () => {
        const s = parseNeoBotAuthStatus({
            configured: false,
            setup_allowed: false,
            loopback: false,
        })!;
        expect(s.configured).toBe(false);
        expect(s.setupAllowed).toBe(false);
        expect(s.loopback).toBe(false);
    });

    it('已设密码', () => {
        const s = parseNeoBotAuthStatus({ configured: true, setup_allowed: false })!;
        expect(s.configured).toBe(true);
    });

    it('configured 缺失或不是布尔就 null——那不是 auth/status 的回包，别瞎猜', () => {
        expect(parseNeoBotAuthStatus({ ok: true })).toBeNull();
        expect(parseNeoBotAuthStatus({ configured: 'yes' })).toBeNull();
        expect(parseNeoBotAuthStatus(null)).toBeNull();
    });
});
