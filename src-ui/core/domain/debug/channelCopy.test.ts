import { describe, expect, it } from 'vitest';
import type { DebugChannelStatus } from '../../ipc/generated/debug/DebugChannelStatus';
import { channelShortLabel, channelStatusCopy } from './channelCopy';

describe('通道展示保留诊断信息', () => {
    it.each([
        [{ kind: 'tunneled', local_port: 43210 }, '43210'],
        [{ kind: 'unreachable', reason: '连接被拒绝' }, '连接被拒绝'],
        [{ kind: 'auth_failed', status: 401 }, '401'],
        [{ kind: 'unsupported', reason: '容器没有映射端口' }, '容器没有映射端口'],
    ] satisfies Array<[DebugChannelStatus, string]>)('%j', (status, detail) => {
        expect(channelStatusCopy(status).text).toContain(detail);
    });

    it.each(['http', 'ws'] as const)('%s 通道保留用户配置的名称', (kind) => {
        expect(channelShortLabel({ kind, name: '群消息连接' })).toContain('群消息连接');
    });
});
