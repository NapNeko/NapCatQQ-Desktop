import { describe, expect, it } from 'vitest';
import type { RemoteInventory } from '../../ipc/generated/domain/RemoteInventory';
import {
    inferSnowLumaLinuxPackageFromInventory,
    inventoryInstalledHints,
    remoteDirectRunChain,
    localDirectRunChain,
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
    it('defaults to full package when inventory is missing', () => {
        expect(inferSnowLumaLinuxPackageFromInventory(null)).toBe('full');
        expect(inferSnowLumaLinuxPackageFromInventory(undefined)).toBe('full');
        expect(remoteDirectRunChain('snowluma', null)).toEqual(['qq', 'novnc', 'snowluma']);
        expect(remoteDirectRunChain('snowluma', undefined)).not.toContain('nodejs');
    });

    it('reads snowlumaLinuxPackage from inventory and defaults missing to full', () => {
        const inv = inventory({
            selected: {
                home: '/root',
                snowlumaDir: '/opt/snowluma',
                nodeBin: '/opt/snowluma/node',
                needsSudo: false,
            },
            snowlumaLinuxPackage: 'full',
        });
        expect(inferSnowLumaLinuxPackageFromInventory(inv)).toBe('full');
        expect(remoteDirectRunChain('snowluma', 'full')).toEqual(['qq', 'novnc', 'snowluma']);
        expect(
            formatMissingDirectRunNotice('snowluma', { nodejs: false, qq: true, novnc: true, snowluma: true }, 'full'),
        ).toBeNull();
    });

    it('does not re-infer from items when inventory field is set', () => {
        const inv = inventory({
            selected: {
                home: '/root',
                snowlumaDir: '/opt/snowluma',
                nodeBin: '/usr/bin/node',
                needsSudo: false,
            },
            snowlumaLinuxPackage: 'full',
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
            snowlumaLinuxPackage: 'lite',
        });
        expect(inferSnowLumaLinuxPackageFromInventory(inv)).toBe('lite');
        expect(remoteDirectRunChain('snowluma', 'lite')).toContain('nodejs');
        expect(
            formatMissingDirectRunNotice('snowluma', { nodejs: false, qq: true, novnc: true, snowluma: true }, 'lite'),
        ).toBe('未安装 Node.js，请安装');
    });

    it('requires external Node only for local Lite SnowLuma', () => {
        expect(localDirectRunChain('snowluma', 'full')).toEqual(['qq', 'snowluma']);
        expect(localDirectRunChain('snowluma', 'lite')).toEqual(['nodejs', 'qq', 'snowluma']);
    });

    it('treats selected qqInstallBase as installed hint even when not $HOME/Napcat', () => {
        const inv = inventory({
            selected: {
                home: '/root',
                qqInstallBase: '/',
                qqBin: '/opt/QQ/qq',
                snowlumaDir: '/opt/snowluma',
                needsSudo: true,
            },
        });
        expect(inventoryInstalledHints(inv)).toEqual({
            qq: true,
            snowluma: true,
        });
    });
});
