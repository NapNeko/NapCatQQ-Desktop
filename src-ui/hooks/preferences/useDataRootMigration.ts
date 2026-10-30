// 数据根迁移向导的状态、进度订阅与操作。
import { useCallback, useEffect, useState } from 'react';
import { dataRootMigrateService } from '../../core/services/data-root-migrate.service';
import type {
    DataRootMigratePreview,
    DataRootMigrateProgress,
    DataRootMigrateResult,
} from '../../core/ipc/types';

type Phase = 'guide' | 'preview' | 'running' | 'done' | 'error';

export function useDataRootMigration(open: boolean) {
    const [phase, setPhase] = useState<Phase>('guide');
    const [target, setTarget] = useState<string | null>(null);
    const [preview, setPreview] = useState<DataRootMigratePreview | null>(null);
    const [progress, setProgress] = useState<DataRootMigrateProgress | null>(null);
    const [result, setResult] = useState<DataRootMigrateResult | null>(null);
    const [errorMsg, setErrorMsg] = useState<string | null>(null);
    const [picking, setPicking] = useState(false);
    const [deletingOld, setDeletingOld] = useState(false);
    const [oldDeleted, setOldDeleted] = useState(false);
    const [restarting, setRestarting] = useState(false);

    const reset = useCallback(() => {
        setPhase('guide');
        setTarget(null);
        setPreview(null);
        setProgress(null);
        setResult(null);
        setErrorMsg(null);
        setPicking(false);
        setDeletingOld(false);
        setOldDeleted(false);
        setRestarting(false);
    }, []);

    useEffect(() => {
        if (!open) {
            reset();
        }
    }, [open, reset]);

    useEffect(() => {
        if (!open || phase !== 'running') return;
        let disposed = false;
        let unlisten: (() => void) | undefined;
        void dataRootMigrateService
            .listenProgress((p) => {
                if (!disposed) setProgress(p);
            })
            .then((u) => {
                if (disposed) u();
                else unlisten = u;
            });
        return () => {
            disposed = true;
            unlisten?.();
        };
    }, [open, phase]);

    const deleteOldRoot = async () => {
        if (!result) return;
        setDeletingOld(true);
        setErrorMsg(null);
        try {
            await dataRootMigrateService.deleteRetired(result.old_root, result.new_root);
            setOldDeleted(true);
        } catch (e) {
            setErrorMsg(e instanceof Error ? e.message : String(e));
        } finally {
            setDeletingOld(false);
        }
    };

    const restartApp = async () => {
        setRestarting(true);
        setErrorMsg(null);
        try {
            await dataRootMigrateService.restart();
        } catch (e) {
            setRestarting(false);
            setErrorMsg(e instanceof Error ? e.message : String(e));
        }
    };

    const pickTarget = async () => {
        setPicking(true);
        setErrorMsg(null);
        try {
            const path = await dataRootMigrateService.pickTargetDirectory();
            if (!path) return;
            setTarget(path);
            const p = await dataRootMigrateService.preview(path);
            setPreview(p);
            setPhase('preview');
            if (!p.ok) {
                setErrorMsg(p.blocking_reasons[0] ?? '预检未通过');
            }
        } catch (e) {
            setErrorMsg(e instanceof Error ? e.message : String(e));
            setPhase('error');
        } finally {
            setPicking(false);
        }
    };

    const startMigrate = async () => {
        if (!target || !preview?.ok) return;
        setPhase('running');
        setErrorMsg(null);
        setProgress(null);
        try {
            const r = await dataRootMigrateService.start(target);
            setResult(r);
            setPhase('done');
        } catch (e) {
            setErrorMsg(e instanceof Error ? e.message : String(e));
            setPhase('error');
        }
    };

    const cancelRunning = async () => {
        try {
            await dataRootMigrateService.cancel();
        } catch {
            // 迁移任务通过进度和最终结果反馈取消状态。
        }
    };

    return {
        phase,
        target,
        preview,
        progress,
        result,
        errorMsg,
        picking,
        deletingOld,
        oldDeleted,
        restarting,
        deleteOldRoot,
        restartApp,
        pickTarget,
        startMigrate,
        cancelRunning,
    };
}
