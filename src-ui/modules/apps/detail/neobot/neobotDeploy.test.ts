import { describe, expect, it } from 'vitest';
import { isBotAccountUnset, missingRequiredSteps, parseNeoBotDeployStatus } from './neobotDeploy';

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
        path_hint: '/onebot',
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

    // 面板那份已经 str().strip() 洗过一遍（api.py 的 deploy_status），这里再洗一次是**防御**：
    // 上游换了实现、或中间层改了序列化时，不至于把空白 QQ 带进界面
    it('管理员列表只留非空字符串（面板已洗过，这里是防御）', () => {
        const s = parseNeoBotDeployStatus(REAL)!;
        expect(s.values.adminAccounts).toEqual(['10002']);
    });

    it('机器人 QQ 给成数字也认，不当成没填', () => {
        const s = parseNeoBotDeployStatus({
            ...REAL,
            values: { ...REAL.values, bot_account: 123456789 },
            defaults: { bot_account: 0 },
        })!;
        expect(s.values.botAccount).toBe('123456789');
        expect(s.defaults.botAccount).toBe('0');
        expect(isBotAccountUnset(s)).toBe(false);
    });

    it('没有 steps 就 null——这一页的骨架就是它，缺了没法说「还差什么」', () => {
        expect(parseNeoBotDeployStatus({ ok: true })).toBeNull();
        expect(parseNeoBotDeployStatus(null)).toBeNull();
    });

    it('steps 缺 required 时按必填算（宁可多提示，别把必填漏掉）', () => {
        const s = parseNeoBotDeployStatus({ steps: [{ key: 'x' }] })!;
        expect(s.steps[0].required).toBe(true);
        expect(s.steps[0].done).toBe(false);
        // label 缺失时回落到 key，不至于在界面上留空（夹具故意不给 label，才真走到这支）
        expect(s.steps[0].label).toBe('x');
    });

    it('onebot 缺失时给全空默认值，不抛也不返回 undefined', () => {
        const s = parseNeoBotDeployStatus({ steps: [] })!;
        expect(s.onebot.port).toBe(0);
        expect(s.onebot.urlLocal).toBe('');
        expect(s.envRevision).toBeNull();
    });
});

describe('isBotAccountUnset', () => {
    const withAccount = (account: string, fallback = '0') =>
        parseNeoBotDeployStatus({
            steps: [],
            values: { bot_account: account },
            defaults: { bot_account: fallback },
        })!;

    it('空字符串算没填', () => {
        expect(isBotAccountUnset(withAccount(''))).toBe(true);
    });

    it('等于出厂占位也算没填——不能拿字面量判，要对着面板给的 defaults 比', () => {
        expect(isBotAccountUnset(withAccount('0'))).toBe(true);
        // 出厂占位换成别的值时，判断跟着走
        expect(isBotAccountUnset(withAccount('default-qq', 'default-qq'))).toBe(true);
    });

    it('填了真 QQ 就是填了', () => {
        expect(isBotAccountUnset(withAccount('10001'))).toBe(false);
    });

    it('面板没给 defaults 时，只要非空就算填了', () => {
        expect(isBotAccountUnset(withAccount('10001', ''))).toBe(false);
    });
});

describe('missingRequiredSteps', () => {
    it('只数必填且没做的', () => {
        const s = parseNeoBotDeployStatus(REAL)!;
        expect(missingRequiredSteps(s).map((x) => x.key)).toEqual(['persona']);
    });
});
