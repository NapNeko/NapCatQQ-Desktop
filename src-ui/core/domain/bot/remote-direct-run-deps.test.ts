import { describe, expect, it } from 'vitest';
import type { RemoteInventory } from '../../ipc/generated/domain/RemoteInventory';
import {
    inferSnowLumaLinuxPackageFromInventory,
    isBundledSnowlumaNode,
    remoteDirectRunChain,
    formatMissingDirectRunNotice,
} from './remote-direct-run-deps';

function inventory(partial: Partial<RemoteInventory> & Pick<RemoteInventory, 'selected'>): RemoteInventory {
    return {
        v: 1,
        probedAt: 't',
        home: '/root',
        items: [],
        ...partial,
    };
}

describe('remote direct-run deps', () => {
    it('treats {snowluma}/node as bundled full package', () => {
        expect(isBundledSnowlumaNode('/opt/sl/node', '/opt/sl')).toBe(true);
        expect(
            isBundledSnowlumaNode(
                '/home/u/snowluma-remote/workspace/node/bin/node',
                '/home/u/snowluma-remote/workspace/snowluma',
            ),
        ).toBe(false);
    });

    it('infers full from selected bundled node', () => {
        const inv = inventory({
            selected: {
                home: '/root',
                snowlumaDir: '/opt/snowluma',
                nodeBin: '/opt/snowluma/node',
                needsSudo: false,
            },
        });
        expect(inferSnowLumaLinuxPackageFromInventory(inv)).toBe('full');
        expect(remoteDirectRunChain('snowluma', 'full')).toEqual(['qq', 'novnc', 'snowluma']);
        expect(
            formatMissingDirectRunNotice('snowluma', { nodejs: false, qq: true, novnc: true, snowluma: true }, 'full'),
        ).toBeNull();
    });

    it('infers full from inventory nodejs item even if selected.nodeBin is portable', () => {
        const inv = inventory({
            selected: {
                home: '/root',
                snowlumaDir: '/opt/snowluma',
                nodeBin: '/usr/bin/node',
                needsSudo: false,
            },
            items: [
                {
                    kind: 'nodejs',
                    root: '/opt/snowluma/node',
                    source: 'desktopOwned',
                    verified: true,
                    nodeBin: '/opt/snowluma/node',
                },
            ],
        });
        expect(inferSnowLumaLinuxPackageFromInventory(inv)).toBe('full');
    });

    it('keeps nodejs on the lite chain', () => {
        const inv = inventory({
            selected: {
                home: '/root',
                snowlumaDir: '/root/snowluma-remote/workspace/snowluma',
                nodeBin: '/root/snowluma-remote/workspace/node/bin/node',
                needsSudo: false,
            },
        });
        expect(inferSnowLumaLinuxPackageFromInventory(inv)).toBe('lite');
        expect(remoteDirectRunChain('snowluma', 'lite')).toContain('nodejs');
        expect(
            formatMissingDirectRunNotice('snowluma', { nodejs: false, qq: true, novnc: true, snowluma: true }, 'lite'),
        ).toBe('未安装 Node.js，请安装');
    });
});
