import { describe, expect, it } from 'vitest';
import { formatUptime, parseNeoBotOverview } from './neobotPanel';

/** 与面板 /api/overview 的真实回包同形（键取自 dashboard/api.py 的 _json_ok） */
const REAL = {
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
        { level: 'warning', text: '尚未配置超级管理员账号', hint: '在配置管理里填 admin_accounts' },
    ],
};

describe('parseNeoBotOverview', () => {
    it('认得真实回包', () => {
        const o = parseNeoBotOverview(REAL);
        expect(o).not.toBeNull();
        expect(o!.online).toBe(true);
        expect(o!.appVersion).toBe('1.2.3');
        expect(o!.botNickname).toBe('Luna');
        expect(o!.botUserId).toBe('10001');
        expect(o!.todayMessages).toBe(128);
        expect(o!.pluginsLoaded).toBe(6);
        expect(o!.pluginsTotal).toBe(7);
        expect(o!.latencyMs).toBe(42);
        expect(o!.notices).toHaveLength(1);
        expect(o!.notices[0].hint).toContain('admin_accounts');
    });

    it('缺键给默认值，不抛也不返回 undefined', () => {
        const o = parseNeoBotOverview({ ok: true });
        expect(o).not.toBeNull();
        expect(o!.online).toBe(false);
        expect(o!.appName).toBe('');
        expect(o!.todayMessages).toBe(0);
        expect(o!.latencyMs).toBeNull();
        expect(o!.notices).toEqual([]);
    });

    it('类型不符就丢，不把字符串当数字', () => {
        const o = parseNeoBotOverview({
            online: 'yes',
            today_messages: '128',
            latency_ms: null,
            notices: 'nope',
        });
        expect(o!.online).toBe(false);
        expect(o!.todayMessages).toBe(0);
        expect(o!.latencyMs).toBeNull();
        expect(o!.notices).toEqual([]);
    });

    it('QQ 号是数字也转成字符串（超长数字别被 JS 精度吃掉）', () => {
        expect(parseNeoBotOverview({ bot_user_id: 123456789012345 }).botUserId).toBe(
            '123456789012345',
        );
        expect(parseNeoBotOverview({ bot_user_id: '10001' }).botUserId).toBe('10001');
        expect(parseNeoBotOverview({ bot_user_id: null }).botUserId).toBeNull();
    });

    it('notices 里的空文本条目被丢掉', () => {
        const o = parseNeoBotOverview({ notices: [{ text: '' }, { text: '有内容' }, 'garbage'] });
        expect(o!.notices.map((n) => n.text)).toEqual(['有内容']);
    });

    it('不是对象就返回 null（调用方据此显示「面板没给数据」）', () => {
        expect(parseNeoBotOverview(null)).toBeNull();
        expect(parseNeoBotOverview(undefined)).toBeNull();
        expect(parseNeoBotOverview('x')).toBeNull();
        expect(parseNeoBotOverview([1, 2])).toBeNull();
    });
});

describe('formatUptime', () => {
    it('按量级说人话', () => {
        expect(formatUptime(0)).toBe('—');
        expect(formatUptime(-5)).toBe('—');
        expect(formatUptime(45)).toBe('45 秒');
        expect(formatUptime(300)).toBe('5 分');
        expect(formatUptime(8130)).toBe('2 小时 15 分');
        expect(formatUptime(90000)).toBe('1 天 1 小时');
    });
});