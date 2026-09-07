import { describe, expect, it } from 'vitest';
import { karinDefaultConfig } from '../../../../core/domain/apps/karinConfig';
import { listKarinConnections } from './karinConnectionsModel';

describe('listKarinConnections', () => {
    it('把单例和列表摊成卡片，对接只标在反向 WS 上', () => {
        const rows = listKarinConnections(karinDefaultConfig(7777), true);
        expect(rows.map((r) => r.kind)).toEqual([
            'webui',
            'reverseWs',
            'forwardWs',
            'onebotHttp',
            'console',
        ]);
        const webui = rows[0];
        expect(webui.kind).toBe('webui');
        if (webui.kind === 'webui') {
            expect(webui.summary).toBe('http://0.0.0.0:7777');
            expect(webui.removable).toBe(false);
        }
        const reverse = rows[1];
        expect(reverse.kind).toBe('reverseWs');
        if (reverse.kind === 'reverseWs') {
            expect(reverse.linked).toBe(true);
            expect(reverse.removable).toBe(false);
        }
        expect(rows.filter((r) => r.removable)).toHaveLength(2);
    });
});
