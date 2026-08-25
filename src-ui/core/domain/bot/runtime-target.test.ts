import { describe, expect, it } from 'vitest';

import {
    RUNTIME_TARGET_REMOTE_PLACEHOLDER,
    runtimeTargetDisplayLabel,
    runtimeTargetTooltip,
} from './runtime-target';

const servers = [
    {
        id: '4c494f8581d453eb',
        name: 'kunming-4-8',
        host: '203.0.113.8',
        username: 'root',
        port: 22,
    },
    {
        id: 'host-only',
        name: '  ',
        host: '10.0.0.2',
        username: 'ubuntu',
        port: 2222,
    },
];

describe('runtimeTargetDisplayLabel', () => {
    it('本机显示本机', () => {
        expect(runtimeTargetDisplayLabel('local', servers)).toBe('本机');
    });

    it('未选定主机时显示远程', () => {
        expect(
            runtimeTargetDisplayLabel(RUNTIME_TARGET_REMOTE_PLACEHOLDER, servers),
        ).toBe('远程');
    });

    it('用档案名称而不是内部 id', () => {
        expect(runtimeTargetDisplayLabel('4c494f8581d453eb', servers)).toBe(
            'kunming-4-8',
        );
        expect(
            runtimeTargetDisplayLabel('remote:4c494f8581d453eb', servers),
        ).toBe('kunming-4-8');
    });

    it('没有名称时回退到主机地址', () => {
        expect(runtimeTargetDisplayLabel('host-only', servers)).toBe('10.0.0.2');
    });

    it('档案缺失时不把裸 id 亮给用户', () => {
        expect(runtimeTargetDisplayLabel('deadbeef', servers)).toBe('远程主机');
    });
});

describe('runtimeTargetTooltip', () => {
    it('拼出名称和登录端点，不暴露档案 id', () => {
        expect(runtimeTargetTooltip('4c494f8581d453eb', servers)).toBe(
            'kunming-4-8 · root@203.0.113.8',
        );
    });

    it('非 22 端口带上端口号', () => {
        expect(runtimeTargetTooltip('host-only', servers)).toBe(
            '远程主机 · ubuntu@10.0.0.2:2222',
        );
    });
});
