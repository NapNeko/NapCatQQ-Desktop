import { describe, expect, it } from 'vitest';
import type { RemoteInventory } from '../../ipc/generated/domain/RemoteInventory';
import {
    inventoryKindLabel,
    inventorySourceLabel,
    inventorySummary,
    isInventoryItemSelected,
} from './inventory-labels';

describe('inventory labels', () => {
    it('maps kind and source to Chinese labels', () => {
        expect(inventoryKindLabel('napcat')).toBe('NapCat');
        expect(inventorySourceLabel('officialInstaller')).toBe('官方安装器');
        expect(inventorySourceLabel('process')).toBe('运行中进程');
    });

    it('marks selected qq root', () => {
        expect(
            isInventoryItemSelected('qq', '/home/u/Napcat', {
                home: '/home/u',
                qqInstallBase: '/home/u/Napcat',
                needsSudo: false,
            }),
        ).toBe(true);
        expect(
            isInventoryItemSelected('docker_container', 'ncbot-1', {
                home: '/home/u',
                needsSudo: false,
            }),
        ).toBe(false);
    });

    it('summarizes empty vs discovered inventory', () => {
        expect(inventorySummary(null)).toBe('未发现安装，将按桌面默认路径安装');
        const inv: RemoteInventory = {
            v: 1,
            probedAt: '2026-08-25T00:00:00Z',
            home: '/home/u',
            items: [
                {
                    kind: 'qq',
                    root: '/home/u/Napcat',
                    source: 'officialInstaller',
                    verified: true,
                },
            ],
            selected: { home: '/home/u', qqInstallBase: '/home/u/Napcat', needsSudo: false },
            bots: [],
        };
        expect(inventorySummary(inv)).toBe('已发现 QQ');
    });
});
