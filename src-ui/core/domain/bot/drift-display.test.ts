import { describe, expect, it } from 'vitest';
import { transformDriftEntries } from './drift-display';
import type { DriftEntry } from '../../ipc/generated/DriftEntry';

describe('transformDriftEntries', () => {
    it('transforms known NapCat/SnowLuma drift entries into human labels and values', () => {
        const entries: DriftEntry[] = [
            {
                file: 'onebot_572381217.json',
                path: 'historySync',
                internal: { enabled: false },
                external: null,
            },
            {
                file: 'onebot_572381217.json',
                path: 'trigger',
                internal: '#sl',
                external: '',
            },
            {
                file: 'napcat.json',
                path: 'fileLog',
                internal: true,
                external: false,
            },
        ];

        const displays = transformDriftEntries(entries);
        expect(displays).toHaveLength(3);

        // historySync
        expect(displays[0].label).toBe('历史消息同步');
        expect(displays[0].path).toBe('historySync');
        expect(displays[0].ours.kind).toBe('json');
        expect(displays[0].theirs.kind).toBe('json');

        // trigger
        expect(displays[1].label).toBe('指令触发前缀');
        expect(displays[1].ours).toEqual({ kind: 'scalar', text: '#sl' });
        expect(displays[1].theirs).toEqual({ kind: 'scalar', text: '(空)' });

        // fileLog
        expect(displays[2].label).toBe('文件日志');
        expect(displays[2].ours).toEqual({ kind: 'scalar', text: '开启' });
        expect(displays[2].theirs).toEqual({ kind: 'scalar', text: '关闭' });
    });

    it('handles network connections and fallback adapters', () => {
        const entries: DriftEntry[] = [
            {
                file: 'onebot.json',
                path: 'network.httpServers',
                internal: [
                    { name: 'server1', enable: true, host: '127.0.0.1', port: 3000 },
                ],
                external: [],
            },
            {
                file: 'custom.json',
                path: 'unknown.customKey',
                internal: 'valA',
                external: 'valB',
            },
        ];

        const displays = transformDriftEntries(entries);
        expect(displays[0].label).toBe('HTTP 服务器');
        expect(displays[0].ours.kind).toBe('connections');
        if (displays[0].ours.kind === 'connections') {
            expect(displays[0].ours.items[0].endpoint).toBe('http://127.0.0.1:3000/');
        }

        expect(displays[1].label).toBe('customKey');
        expect(displays[1].ours).toEqual({ kind: 'scalar', text: 'valA' });
        expect(displays[1].theirs).toEqual({ kind: 'scalar', text: 'valB' });
    });
});

