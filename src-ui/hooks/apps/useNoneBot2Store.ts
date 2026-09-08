import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { isAppConfigError } from '../../core/domain/apps/appConfigError';
import { errorText } from '../../core/domain/errors';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushAppErrorBar } from './pushAppErrorBar';
import { deploymentTaskStore } from '../task-queue/deploymentTaskStore';
import {
    filterNoneBot2Store,
    overlayInstalledFromTasks,
    pluginCatalogErrorCopy,
    storeOpErrorCopy,
    type StoreKindFilter,
    type StoreTaskHint,
} from '../../modules/apps/detail/nonebot2/nonebot2StoreModel';
import type {
    AppInstance,
    AppPluginAction,
    AppStoreInstalled,
    AppStoreMarketEntry,
    AppStoreResource,
    DeploymentTaskKind,
    DeploymentTaskSnapshot,
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

export function useNoneBot2Store(instance: AppInstance, resource: AppStoreResource) {
    const taskState = useSyncExternalStore(
        deploymentTaskStore.subscribe,
        deploymentTaskStore.getSnapshot,
        deploymentTaskStore.getSnapshot,
    );

    const [market, setMarket] = useState<AppStoreMarketEntry[]>([]);
    const [installed, setInstalled] = useState<AppStoreInstalled[]>([]);
    const [adapters, setAdapters] = useState<AppStoreInstalled[]>([]);
    const [query, setQuery] = useState('');
    const [kindFilter, setKindFilter] = useState<StoreKindFilter>('all');
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
            const [nextMarket, nextInstalled, nextAdapters] = await Promise.all([
                appFrameworkService.listStore(instance.framework_id, resource),
                appFrameworkService.listStoreInstalled(instance.id, resource),
                resource === 'plugin'
                    ? appFrameworkService.listStoreInstalled(instance.id, 'adapter')
                    : Promise.resolve([]),
            ]);
            setMarket(nextMarket);
            setInstalled(nextInstalled);
            setAdapters(nextAdapters);
        } catch (e) {
            const raw = errorText(e);
            setError(raw);
            const copy = pluginCatalogErrorCopy(raw);
            pushAppErrorBar({
                key: `nb-store-catalog:${instance.id}:${resource}`,
                title: copy.title,
                raw,
                content: copy.content,
            });
        } finally {
            setLoading(false);
        }
    }, [instance.framework_id, instance.id, resource]);

    useEffect(() => {
        void reload();
    }, [reload]);

    const pluginTasks = useMemo(
        () =>
            Object.values(taskState.tasks).filter(
                (task): task is AppPluginTask =>
                    isAppPluginTask(task)
                    && task.kind.instance_id === instance.id
                    && (task.kind.resource ?? 'plugin') === resource,
            ),
        [instance.id, resource, taskState.tasks],
    );

    const busyNames = useMemo(() => {
        const names = new Set(localBusy);
        for (const task of pluginTasks) {
            if (!ACTIVE.has(task.status)) continue;
            names.add(task.kind.plugin_name);
        }
        return names;
    }, [localBusy, pluginTasks]);

    const taskHints = useMemo<StoreTaskHint[]>(
        () =>
            pluginTasks.map((task) => ({
                pluginName: task.kind.plugin_name,
                action: task.kind.action,
                status: task.status,
                atMs: toMs(task.endedAtMs) || toMs(task.startedAtMs) || toMs(task.submittedAtMs),
            })),
        [pluginTasks],
    );

    const enabledAdapterModules = useMemo(
        () =>
            (resource === 'plugin' ? adapters : installed)
                .filter((item) => item.enabled)
                .map((item) => item.id),
        [adapters, installed, resource],
    );

    const rows = useMemo(
        () =>
            filterNoneBot2Store({
                resource,
                entries: market,
                installed: overlayInstalledFromTasks(installed, taskHints, resource),
                query,
                kindFilter,
                enabledAdapterModules,
                linked: !!instance.link,
            }),
        [enabledAdapterModules, installed, instance.link, kindFilter, market, query, resource, taskHints],
    );

    useEffect(() => {
        seenTerminal.current.clear();
        terminalPrimed.current = false;
    }, [instance.id, resource]);

    useEffect(() => {
        if (!taskState.loaded) return;
        if (!terminalPrimed.current) {
            for (const task of pluginTasks) {
                if (TERMINAL.has(task.status)) seenTerminal.current.add(task.taskId);
            }
            terminalPrimed.current = true;
            return;
        }
        const kindLabel = resource === 'adapter' ? '适配器' : '插件';
        for (const task of pluginTasks) {
            if (!TERMINAL.has(task.status)) continue;
            if (seenTerminal.current.has(task.taskId)) continue;
            seenTerminal.current.add(task.taskId);
            void reload();
            const name = task.kind.plugin_name;
            const verb = actionVerb(task.kind.action);
            if (task.status === 'success') {
                pushInfoBar({
                    key: `nb-store-done:${task.taskId}`,
                    tone: 'success',
                    title: `已${verb} ${name}`,
                    content: instance.state === 'running' ? '需重启后生效' : undefined,
                    autoDismissMs: 4000,
                });
            } else if (task.status === 'failed') {
                pushAppErrorBar({
                    key: `nb-store-fail:${task.taskId}`,
                    title: `${kindLabel}${verb}失败`,
                    raw: task.error ? storeOpErrorCopy(task.error) : null,
                });
            }
        }
    }, [instance.id, instance.state, pluginTasks, reload, resource, taskState.loaded]);

    const runOp = useCallback(
        async (id: string, action: AppPluginAction) => {
            setLocalBusy((prev) => new Set(prev).add(id));
            try {
                await appFrameworkService.submitPluginOp(instance.id, id, action, resource);
                const hasTask = Object.values(deploymentTaskStore.getSnapshot().tasks).some(
                    (task) =>
                        task.kind.kind === 'app_plugin'
                        && task.kind.instance_id === instance.id
                        && task.kind.plugin_name === id
                        && (task.kind.resource ?? 'plugin') === resource,
                );
                if (!hasTask) await reload();
            } catch (e) {
                pushAppErrorBar({
                    key: `nb-store-op:${instance.id}:${id}`,
                    title: '操作失败',
                    raw: storeOpErrorCopy(errorText(e)),
                });
            } finally {
                setLocalBusy((prev) => {
                    const next = new Set(prev);
                    next.delete(id);
                    return next;
                });
            }
        },
        [instance.id, reload, resource],
    );

    const applyEnabled = useCallback(
        async (id: string, enabled: boolean, overwrite = false) => {
            try {
                await appFrameworkService.setPluginEnabled(
                    instance.id,
                    id,
                    enabled,
                    overwrite,
                    resource,
                );
                setConflict(null);
                await reload();
            } catch (e) {
                if (isAppConfigError(e) && e.kind === 'conflict') {
                    setConflict({ name: id, enabled });
                    return;
                }
                pushAppErrorBar({
                    key: `nb-store-enable:${instance.id}:${id}`,
                    title: '切换启用失败',
                    raw: errorText(e),
                });
            }
        },
        [instance.id, reload, resource],
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
