import { describe, expect, it } from 'vitest';
import {
    hostComponentStatusBadge,
    shouldOfferManagedNodeInstall,
} from './componentStatusPresentation';

describe('hostComponentStatusBadge', () => {
    const opts = { hasUpdate: false, inFlight: false };

    it('separates "found but unusable" from "not installed"', () => {
        const notInstalled = hostComponentStatusBadge({ state: 'not_installed' }, opts);
        const tooOld = hostComponentStatusBadge(
            {
                state: 'unusable',
                unusable: {
                    source: '$PATH/node',
                    version: '18.19.1',
                    reason: 'v18.19.1 不满足 ^22.13.0 || >=23.4.0',
                },
            },
            opts,
        );
        expect(notInstalled.label).toBe('未安装');
        expect(tooOld).toMatchObject({ tone: 'warning', label: '版本不符' });
    });

    it('labels a present-but-broken binary as unable to run', () => {
        const broken = hostComponentStatusBadge(
            {
                state: 'unusable',
                unusable: {
                    source: '/c/ProgramData/NapCatQQ Desktop/components/NodeJs/node.exe',
                    version: null,
                    reason: 'node.exe 存在但无法执行：exit=Some(-1073741515): ',
                },
            },
            opts,
        );
        expect(broken).toMatchObject({ tone: 'warning', label: '无法运行' });
    });
});

describe('Node.js component actions', () => {
    it('does not offer installation when a valid external Node.js is detected', () => {
        expect(
            shouldOfferManagedNodeInstall({
                state: 'installed',
                detected: { version: '24.18.0', source: '$PATH/node' },
            }),
        ).toBe(false);
    });

    it('offers installation when the managed Node.js component is detected', () => {
        expect(
            shouldOfferManagedNodeInstall({
                state: 'installed',
                detected: { version: '24.18.0', source: 'C:/ProgramData/NapCatQQ Desktop/NodeJs/node.exe' },
            }),
        ).toBe(true);
    });
});
