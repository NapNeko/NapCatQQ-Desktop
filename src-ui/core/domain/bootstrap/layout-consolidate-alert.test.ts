import { describe, expect, it } from 'vitest';
import {
    buildLayoutConsolidateContent,
    resolveLayoutConsolidateAlert,
} from './layout-consolidate-alert';
import type { DataLayoutConsolidateSnapshot } from '../../ipc/types';

function snap(
    partial: Partial<DataLayoutConsolidateSnapshot>,
): DataLayoutConsolidateSnapshot {
    return {
        performed: false,
        moved_count: 0,
        warnings: [],
        ...partial,
    };
}

describe('resolveLayoutConsolidateAlert', () => {
    it('layout 缺失或仅 skip 时不弹', () => {
        expect(resolveLayoutConsolidateAlert(null).kind).toBe('none');
        expect(resolveLayoutConsolidateAlert(undefined).kind).toBe('none');
        expect(
            resolveLayoutConsolidateAlert(
                snap({ performed: false, skipped_reason: 'layout current' }),
            ).kind,
        ).toBe('none');
    });

    it('performed 成功时 success + 备份路径提示', () => {
        const alert = resolveLayoutConsolidateAlert(
            snap({
                performed: true,
                backup_path: 'D:\\Desktop\\NapCatQQ-Desktop-backup-2x-1.zip',
                moved_count: 3,
            }),
        );
        expect(alert.kind).toBe('success');
        if (alert.kind === 'none') throw new Error('expected alert');
        expect(alert.title).toBe('数据目录已整理');
        expect(alert.content).toContain('含密钥，请自行保管');
        expect(alert.content).toContain('整理 3 项');
        expect(alert.autoDismissMs).toBe(10_000);
    });

    it('error 时 warning 且不自动消失', () => {
        const alert = resolveLayoutConsolidateAlert(
            snap({
                performed: false,
                error: 'backup failed: disk full',
            }),
        );
        expect(alert.kind).toBe('warning');
        if (alert.kind === 'none') throw new Error('expected alert');
        expect(alert.title).toBe('数据目录整理未完成');
        expect(alert.content).toContain('disk full');
        expect(alert.content).toContain('原数据目录未删除');
        expect(alert.autoDismissMs).toBe(0);
    });

    it('warnings 非空时 tone 为 warning', () => {
        const alert = resolveLayoutConsolidateAlert(
            snap({
                performed: true,
                warnings: ['components/NapCatQQ: destination exists, skip move'],
            }),
        );
        expect(alert.kind).toBe('warning');
    });
});

describe('buildLayoutConsolidateContent', () => {
    it('无字段时回落默认文案', () => {
        expect(buildLayoutConsolidateContent(snap({}))).toBe('数据目录已检查。');
    });
});
