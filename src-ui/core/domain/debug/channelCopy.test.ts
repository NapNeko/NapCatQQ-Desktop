import { describe, expect, it } from 'vitest';
import type { DebugChannelId } from '../../ipc/generated/debug/DebugChannelId';
import type { DebugChannelStatus } from '../../ipc/generated/debug/DebugChannelStatus';
import {
    NO_CHANNEL_EXITS,
    UPGRADE_RUNTIME_EXIT,
    channelShortLabel,
    channelStatusCopy,
} from './channelCopy';

describe('channelStatusCopy', () => {
    const CASES = [
        { s: { kind: 'unknown' }, text: '未测试', tone: 'neutral' },
        { s: { kind: 'available' }, text: '可用', tone: 'success' },
        {
            s: { kind: 'tunneled', local_port: 43210 },
            text: '已开隧道（本地端口 43210）',
            tone: 'success',
        },
        {
            s: { kind: 'unreachable', reason: '连接被拒绝' },
            text: '连不上（连接被拒绝）',
            tone: 'danger',
        },
        { s: { kind: 'auth_failed', status: 401 }, text: 'token 错误（401）', tone: 'danger' },
        {
            s: { kind: 'upstream_too_old' },
            text: '上游版本太老，升级到最新版 NapCat / SnowLuma 后可用',
            tone: 'warning',
        },
        { s: { kind: 'bot_not_running' }, text: 'Bot 未运行', tone: 'warning' },
        { s: { kind: 'not_logged_in' }, text: 'QQ 未登录', tone: 'warning' },
        {
            s: { kind: 'unsupported', reason: '容器没有映射 8080 端口' },
            text: '不支持（容器没有映射 8080 端口）',
            tone: 'neutral',
        },
    ] satisfies Array<{
        s: DebugChannelStatus;
        text: string;
        tone: 'success' | 'warning' | 'danger' | 'neutral';
    }>;

    it.each(CASES)('$s.kind', ({ s, text, tone }) => {
        expect(channelStatusCopy(s)).toEqual({ text, tone });
    });

    it('覆盖了 DebugChannelStatus 的全部 kind', () => {
        expect(new Set(CASES.map((c) => c.s.kind)).size).toBe(9);
    });
});

describe('channelShortLabel', () => {
    it('四种通道各有短名', () => {
        const cases: Array<[DebugChannelId, string]> = [
            [{ kind: 'auto' }, '自动'],
            [{ kind: 'internal' }, '内部通道'],
            [{ kind: 'http', name: 'http-default' }, 'HTTP · http-default'],
            [{ kind: 'ws', name: 'ws-8080' }, 'WS · ws-8080'],
        ];
        for (const [id, label] of cases) expect(channelShortLabel(id)).toBe(label);
    });
});

describe('「去哪解决」的出口', () => {
    it('全都不可用时：组件页装 / 修运行时 + 机器人页开 WS 服务', () => {
        expect(NO_CHANNEL_EXITS).toEqual([
            { route: 'components', label: '去「组件」页装/修运行时' },
            { route: 'bots', label: '去「机器人」页开 WS 服务' },
        ]);
    });

    it('上游版本太老：去组件页升级', () => {
        expect(UPGRADE_RUNTIME_EXIT).toEqual({ route: 'components', label: '去「组件」页升级' });
    });
});
