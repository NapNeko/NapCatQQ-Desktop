import { describe, expect, it } from 'vitest';
import type { ConnectionHealth } from '../../core/ipc/generated/domain/ConnectionHealth';
import { serverConnectionNote, serverLifecycleBadge } from './serverCardPresentation';

function failedServer(health: ConnectionHealth | null) {
    return { state: 'failed' as const, health };
}

describe('serverLifecycleBadge', () => {
    it('失败态只给一个短徽标，不再用长句占位', () => {
        expect(serverLifecycleBadge('failed')).toEqual({
            tone: 'danger',
            label: '无法连接',
            dot: true,
        });
    });
});

describe('serverConnectionNote', () => {
    it('失败原因和次数合成一行，原文与重试入口放 title', () => {
        const note = serverConnectionNote(
            failedServer({
                consecutiveFailures: 2,
                lastFailureReason: '密码连接失败（请检查用户名 / 密码 / 网络）',
            }),
        );
        expect(note?.text).toBe('2 次失败 · 密码连接失败（请检查用户名 / 密码 / 网络）');
        expect(note?.title).toContain('密码连接失败');
        expect(note?.title).toContain('Wi-Fi');
    });

    it('没有失败原因时只报次数', () => {
        expect(serverConnectionNote(failedServer({ consecutiveFailures: 3 }))?.text).toBe(
            '已连续失败 3 次',
        );
    });

    it.each([undefined, 0])('失败次数为 %s 时悬停仍可读完整原因', (consecutiveFailures) => {
        const reason = 'SSH 连接失败，远端主机未响应，请检查网络与端口';
        const note = serverConnectionNote(
            failedServer({ consecutiveFailures, lastFailureReason: reason }),
        );
        expect(note?.text).toBe(reason);
        expect(note?.title).toContain(reason);
        expect(note?.title).toContain('Wi-Fi');
    });

    it('没有健康度信息时不再补一句废话，交给徽标', () => {
        expect(serverConnectionNote(failedServer(null))).toBeNull();
    });

    it('在线和未测试状态没有异常说明', () => {
        expect(
            serverConnectionNote({ state: 'connected', health: { consecutiveFailures: 0 } }),
        ).toBeNull();
        expect(serverConnectionNote({ state: 'disconnected', health: null })).toBeNull();
    });
});
