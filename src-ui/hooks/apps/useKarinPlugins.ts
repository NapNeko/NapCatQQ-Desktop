import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { isAppConfigError } from '../../core/domain/apps/appConfigError';
import { errorText } from '../../core/domain/errors';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushAppErrorBar } from './pushAppErrorBar';
import { deploymentTaskStore } from '../task-queue/deploymentTaskStore';
import {
    filterKarinPlugins,
    overlayInstalledFromTasks,
    pluginCatalogErrorCopy,
    type KarinPluginKindFilter,
    type PluginTaskHint,
} from '../../modules/apps/detail/karin/karinPluginsModel';
import type {
    AppPluginAction,
    AppInstance,
    DeploymentTaskKind,
    DeploymentTaskSnapshot,
    KarinPluginInstalled,
    KarinPluginMarketEntry,
} from '../../core/ipc/types';

type AppPluginTask = DeploymentTaskSnapshot & {
    kind: Extract<DeploymentTaskKind, { kind: 'app_plugin' }>;
};

function isAppPluginTask(task: DeploymentTaskSnapshot): task is AppPluginTask {
    return task.kind.kind === 'app_plugin';
}

const ACTIVE = new Set(['queued', 'running', 'waiting_input']);
const TERMINAL = new Set(['success', 'failed', 'cancelled']);

function toMs(value: bigint | number | null | undefined): number {
    if (value == null) return 0;
    return typeof value === 'bigint' ? Number(value) : value;
}

function actionVerb(action: AppPluginAction): string {
    if (action === 'install') return '安装';
    if (action === 'update') return '更新';
    return '卸载';
}

export function useKarinPlugins(instance: AppInstance) {
    const taskState = useSyncExternalStore(
        deploymentTaskStore.subscribe,
        deploymentTaskStore.getSnapshot,
        deploymentTaskStore.getSnapshot,
    );

    const [market, setMarket] = useState<KarinPluginMarketEntry[]>([]);
    const [installed, setInstalled] = useState<KarinPluginInstalled[]>([]);
    const [query, setQuery] = useState('');
    const [kindFilter, setKindFilter] = useState<KarinPluginKindFilter>('all');
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [conflict, setConflict] = useState<{ name: string; enabled: boolean } | null>(null);
    const [conflictBusy, setConflictBusy] = useState(false);
    const [localBusy, setLocalBusy] = useState<Set<string>>(() => new Set());
    const seenTerminal = useRef<Set<string>>(new Set());
    const terminalPrimed = useRef(false);

    const reload = useCallback(async () => {
        setLoading(true);
        setError(null);
        try {
            const [nextMarket, nextInstalled] = await Promise.all([
                appFrameworkService.listPluginMarket(),
                appFrameworkService.listPlugins(instance.id),
            ]);
            setMarket(nextMarket);
            setInstalled(nextInstalled);
        } catch (e) {
            const raw = errorText(e);
            setError(raw);
            const copy = pluginCatalogErrorCopy(raw);
            pushAppErrorBar({
                key: `karin-plugin-catalog:${instance.id}`,
                title: copy.title,
                raw,
                content: copy.content,
            });
        } finally {
            setLoading(false);
        }
    }, [instance.id]);

    useEffect(() => {
        void reload();
    }, [reload]);

    const pluginTasks = useMemo(
        () =>
            Object.values(taskState.tasks).filter(
                (task): task is AppPluginTask =>
                    isAppPluginTask(task)
                    && task.kind.instance_id === instance.id
                    && (task.kind.resource ?? 'plugin') === 'plugin',
            ),
        [instance.id, taskState.tasks],
    );

    const busyNames = useMemo(() => {
        const names = new Set(localBusy);
        for (const task of pluginTasks) {
            if (!ACTIVE.has(task.status)) continue;
            names.add(task.kind.plugin_name);
        }
        return names;
    }, [localBusy, pluginTasks]);

    const taskHints = useMemo<PluginTaskHint[]>(
        () =>
            pluginTasks.map((task) => ({
                pluginName: task.kind.plugin_name,
                action: task.kind.action,
                status: task.status,
                atMs: toMs(task.endedAtMs) || toMs(task.startedAtMs) || toMs(task.submittedAtMs),
            })),
        [pluginTasks],
    );

    const rows = useMemo(
        () =>
            filterKarinPlugins(
                market,
                overlayInstalledFromTasks(installed, taskHints),
                query,
                kindFilter,
            ),
        [installed, kindFilter, market, query, taskHints],
    );

    useEffect(() => {
        seenTerminal.current.clear();
        terminalPrimed.current = false;
    }, [instance.id]);

    useEffect(() => {
        if (!taskState.loaded) return;
        if (!terminalPrimed.current) {
            for (const task of pluginTasks) {
                if (TERMINAL.has(task.status)) seenTerminal.current.add(task.taskId);
            }
            terminalPrimed.current = true;
            return;
        }
        for (const task of pluginTasks) {
            if (!TERMINAL.has(task.status)) continue;
            if (seenTerminal.current.has(task.taskId)) continue;
            seenTerminal.current.add(task.taskId);
            void reload();
            const name = task.kind.plugin_name;
            const verb = actionVerb(task.kind.action);
            if (task.status === 'success') {
                pushInfoBar({
                    key: `karin-plugin-done:${task.taskId}`,
                    tone: 'success',
                    title: `已${verb} ${name}`,
                    content: instance.state === 'running' ? 'Karin 会热加载' : undefined,
                    autoDismissMs: 4000,
                });
            } else if (task.status === 'failed') {
                pushAppErrorBar({
                    key: `karin-plugin-fail:${task.taskId}`,
                    title: `${verb}失败`,
                    raw: task.error,
                });
            }
        }
    }, [instance.id, instance.state, pluginTasks, reload, taskState.loaded]);

    const runOp = useCallback(
        async (pluginName: string, action: AppPluginAction) => {
            setLocalBusy((prev) => new Set(prev).add(pluginName));
            try {
                await appFrameworkService.submitPluginOp(instance.id, pluginName, action);
                const hasTask = Object.values(deploymentTaskStore.getSnapshot().tasks).some(
                    (task) =>
                        task.kind.kind === 'app_plugin' &&
                        task.kind.instance_id === instance.id &&
                        task.kind.plugin_name === pluginName,
                );
                if (!hasTask) {
                    await reload();
                }
            } catch (e) {
                pushAppErrorBar({
                    key: `karin-plugin-op:${instance.id}:${pluginName}`,
                    title: '插件操作失败',
                    raw: errorText(e),
                });
            } finally {
                setLocalBusy((prev) => {
                    const next = new Set(prev);
                    next.delete(pluginName);
                    return next;
                });
            }
        },
        [instance.id, reload],
    );

    const applyEnabled = useCallback(
        async (pluginName: string, enabled: boolean, overwrite = false) => {
            try {
                await appFrameworkService.setPluginEnabled(instance.id, pluginName, enabled, overwrite);
                setConflict(null);
                await reload();
            } catch (e) {
                if (isAppConfigError(e) && e.kind === 'conflict') {
                    setConflict({ name: pluginName, enabled });
                    return;
                }
                pushAppErrorBar({
                    key: `karin-plugin-enable:${instance.id}:${pluginName}`,
                    title: '切换启用失败',
                    raw: errorText(e),
                });
            }
        },
        [instance.id, reload],
    );

    const resolveConflict = useCallback(
        async (overwrite: boolean) => {
            if (!conflict) return;
            setConflictBusy(true);
            try {
                if (!overwrite) {
                    setConflict(null);
                    await reload();
                    return;
                }
                await applyEnabled(conflict.name, conflict.enabled, true);
            } finally {
                setConflictBusy(false);
            }
        },
        [applyEnabled, conflict, reload],
    );

    return {
        rows,
        query,
        setQuery,
        kindFilter,
        setKindFilter,
        loading,
        error,
        reload,
        busyNames,
        runOp,
        applyEnabled,
        conflict,
        conflictBusy,
        dismissConflict: () => setConflict(null),
        reloadConflict: () => void resolveConflict(false),
        overwriteConflict: () => void resolveConflict(true),
    };
}
