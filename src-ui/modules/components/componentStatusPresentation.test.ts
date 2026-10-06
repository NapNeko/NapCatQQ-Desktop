import { describe, expect, it } from 'vitest';
import {
    hostComponentStatusBadge,
    shouldOfferManagedNodeInstall,
    uninstallEntryAction,
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
                detected: {
                    version: '24.18.0',
                    source: 'C:/ProgramData/NapCatQQ Desktop/NodeJs/node.exe',
                },
            }),
        ).toBe(true);
    });
});

describe('uninstallEntryAction', () => {
    const installed = (source: string) =>
        ({ state: 'installed', detected: { version: '1.0', source } }) as const;

    it('supported 组件对已安装的托管探测给组件任务', () => {
        expect(uninstallEntryAction('supported', installed('napcat.mjs'))).toBe('task');
    });

    it('system_managed 组件对已安装状态指到系统卸载流', () => {
        expect(uninstallEntryAction('system_managed', installed('HKLM/Runtimes/X64'))).toBe(
            'system',
        );
    });

    it('not_supported 组件已安装也不渲染卸载入口', () => {
        expect(uninstallEntryAction('not_supported', installed('x'))).toBe('none');
    });

    it('未安装时任何能力都不渲染卸载入口', () => {
        expect(uninstallEntryAction('supported', { state: 'not_installed' })).toBe('none');
        expect(uninstallEntryAction('system_managed', { state: 'not_installed' })).toBe('none');
    });

    it('外部 $PATH 探测来源不归桌面端管,不渲染卸载入口', () => {
        expect(uninstallEntryAction('supported', installed('$PATH/node'))).toBe('none');
        expect(uninstallEntryAction('system_managed', installed('$PATH/node'))).toBe('none');
    });
});
