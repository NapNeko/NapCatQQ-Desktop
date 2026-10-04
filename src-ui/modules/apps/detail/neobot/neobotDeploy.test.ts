import { describe, expect, it } from 'vitest';
import { missingRequiredSteps, parseNeoBotDeployStatus } from './neobotDeploy';

// 回包结构与面板 deploy_status 一致（键取自 dashboard/api.py 的 _json_ok）
const REAL = {
    ok: true,
    ready: false,
    revision: 'rev-1',
    env_revision: 'env-1',
    steps: [
        { key: 'bot_identity', label: '机器人身份', required: true, done: true, hint: '必填' },
        { key: 'persona', label: '人设', required: true, done: false, hint: '必填' },
        { key: 'admin', label: '超级管理员（选填）', required: false, done: false, hint: '选填' },
    ],
    onebot: {
        host: '0.0.0.0',
        port: 8080,
        url_local: 'ws://127.0.0.1:8080/onebot/v11/ws',
        url_lan: 'ws://192.168.1.10:8080/onebot/v11/ws',
        token: '',
        token_enabled: false,
        path_hint: '/onebot/v11/ws',
        warning: '没有 access token',
    },
    values: {
        bot_account: '10001',
        bot_nick_name: 'Luna',
        bot_data: '人设',
        alias_name: ['露娜'],
        admin_accounts: ['10002', '  ', 42],
        group_chat_chance: 0.3,
    },
};

describe('parseNeoBotDeployStatus', () => {
    it('认得真实回包', () => {
        const s = parseNeoBotDeployStatus(REAL)!;
        expect(s.ready).toBe(false);
        expect(s.steps).toHaveLength(3);
        expect(s.onebot.port).toBe(8080);
        expect(s.onebot.urlLocal).toContain('127.0.0.1:8080');
        expect(s.onebot.tokenEnabled).toBe(false);
        expect(s.onebot.warning).toContain('access token');
        expect(s.values.botAccount).toBe('10001');
        expect(s.revision).toBe('rev-1');
        expect(s.envRevision).toBe('env-1');
    });

    it('管理员列表只留非空字符串（面板可能混进空白与非字符串）', () => {
        const s = parseNeoBotDeployStatus(REAL)!;
        expect(s.values.adminAccounts).toEqual(['10002']);
    });

    it('没有 steps 就 null——这一页的骨架就是它，缺了没法说「还差什么」', () => {
        expect(parseNeoBotDeployStatus({ ok: true })).toBeNull();
        expect(parseNeoBotDeployStatus(null)).toBeNull();
    });

    it('steps 缺 required 时按必填算（宁可多提示，别把必填漏掉）', () => {
        const s = parseNeoBotDeployStatus({ steps: [{ key: 'x', label: 'X' }] })!;
        expect(s.steps[0].required).toBe(true);
        expect(s.steps[0].done).toBe(false);
        // label 缺失时回落到 key，不至于在界面上留空
        expect(s.steps[0].label).toBe('X');
    });

    it('onebot 缺失时给全空默认值，不抛也不返回 undefined', () => {
        const s = parseNeoBotDeployStatus({ steps: [] })!;
        expect(s.onebot.port).toBe(0);
        expect(s.onebot.urlLocal).toBe('');
        expect(s.envRevision).toBeNull();
    });
});

describe('missingRequiredSteps', () => {
    it('只数必填且没做的', () => {
        const s = parseNeoBotDeployStatus(REAL)!;
        expect(missingRequiredSteps(s).map((x) => x.key)).toEqual(['persona']);
    });
});
