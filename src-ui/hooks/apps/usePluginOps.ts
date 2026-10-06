// Karin 插件市场和应用端商店共用的操作侧：装 / 更新 / 卸载提交成部署任务，任务到终态弹条、刷新已装；
// 启用开关直接写配置，撞上外部改动交给冲突对话框。目录怎么拉、怎么铺成卡片各商店自己管。

import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { isAppConfigError } from '../../core/domain/apps/appConfigError';
import { errorText } from '../../core/domain/errors';
import {
    isPluginTaskActive,
    isPluginTaskDone,
    isPluginTaskOf,
    pluginActionVerb,
    pluginCatalogErrorCopy,
    pluginTaskHint,
    type AppPluginTask,
    type PluginTaskHint,
} from '../../core/domain/apps/pluginCatalog';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushErrorBar } from '../ui/pushErrorBar';
import { deploymentTaskStore } from '../task-queue/deploymentTaskStore';
import type { AppInstance, AppPluginAction, AppStoreResource } from '../../core/ipc/types';

/** 目录拉取失败：弹一条短的，错误照样抛出去，让 react-query 记在这条查询上 */
export function reportCatalogError(key: string, e: unknown, catalog?: string): never {
    const raw = errorText(e);
    const copy = pluginCatalogErrorCopy(raw, catalog);
    pushErrorBar({
        key,
        title: copy.title,
        raw,
        content: copy.content,
    });
    throw e instanceof Error ? e : new Error(raw);
}

export type PluginOpsOptions = {
    /** Karin 不传：它只有插件，提交时也不带 resource */
    resource?: AppStoreResource;
    /** InfoBar key 的前缀，两个商店各用各的 */
    barKey: string;
    /** 装好时实例正在跑，补一句怎么生效 */
    runningHint: string;
    /** 任务失败标题里动词前那截，如「适配器」；空串就只写「安装失败」 */
    failNoun: string;
    /** 提交就失败时的标题 */
    submitFailTitle: string;
    /** 任务 / 提交的错误原文换成短句；不给就原样交给错误条 */
    errorCopy?: (raw: string) => string;
    reloadInstalled: () => Promise<void>;
};

export function usePluginOps(instance: AppInstance, options: PluginOpsOptions) {
    const { resource, barKey, runningHint, failNoun, submitFailTitle, errorCopy, reloadInstalled } =
        options;
    const kind = resource ?? 'plugin';
    const taskState = useSyncExternalStore(
        deploymentTaskStore.subscribe,
        deploymentTaskStore.getSnapshot,
        deploymentTaskStore.getSnapshot,
    );
    const [conflict, setConflict] = useState<{ name: string; enabled: boolean } | null>(null);
    const [conflictBusy, setConflictBusy] = useState(false);
    const [localBusy, setLocalBusy] = useState<Set<string>>(() => new Set());
    const seenTerminal = useRef<Set<string>>(new Set());
    const terminalPrimed = useRef(false);

    const pluginTasks = useMemo(
        () =>
            Object.values(taskState.tasks).filter((task): task is AppPluginTask =>
                isPluginTaskOf(task, instance.id, kind),
            ),
        [instance.id, kind, taskState.tasks],
    );

    const busyNames = useMemo(() => {
        const names = new Set(localBusy);
        for (const task of pluginTasks) {
            if (isPluginTaskActive(task.status)) names.add(task.kind.plugin_name);
        }
        return names;
    }, [localBusy, pluginTasks]);

    const taskHints = useMemo<PluginTaskHint[]>(
        () => pluginTasks.map(pluginTaskHint),
        [pluginTasks],
    );

    useEffect(() => {
        seenTerminal.current.clear();
        terminalPrimed.current = false;
    }, [instance.id, kind]);

    useEffect(() => {
        if (!taskState.loaded) return;
        // 进页时已经结束的任务只记下，不再弹一遍
        if (!terminalPrimed.current) {
            for (const task of pluginTasks) {
                if (isPluginTaskDone(task.status)) seenTerminal.current.add(task.taskId);
            }
            terminalPrimed.current = true;
            return;
        }
        for (const task of pluginTasks) {
            if (!isPluginTaskDone(task.status)) continue;
            if (seenTerminal.current.has(task.taskId)) continue;
            seenTerminal.current.add(task.taskId);
            void reloadInstalled();
            const name = task.kind.plugin_name;
            const verb = pluginActionVerb(task.kind.action);
            if (task.status === 'success') {
                pushInfoBar({
                    key: `${barKey}-done:${task.taskId}`,
                    tone: 'success',
                    title: `已${verb} ${name}`,
                    content: instance.state === 'running' ? runningHint : undefined,
                    autoDismissMs: 4000,
                });
            } else if (task.status === 'failed') {
                pushErrorBar({
                    key: `${barKey}-fail:${task.taskId}`,
                    title: `${failNoun}${verb}失败`,
                    raw: task.error && errorCopy ? errorCopy(task.error) : task.error,
                });
            }
        }
    }, [
        barKey,
        errorCopy,
        failNoun,
        instance.state,
        pluginTasks,
        reloadInstalled,
        runningHint,
        taskState.loaded,
    ]);

    const runOp = useCallback(
        async (name: string, action: AppPluginAction) => {
            setLocalBusy((prev) => new Set(prev).add(name));
            try {
                await appFrameworkService.submitPluginOp(instance.id, name, action, resource);
                // 已经进了队列就等终态再刷；没进队列的自己刷一次
                const queued = Object.values(deploymentTaskStore.getSnapshot().tasks).some(
                    (task) =>
                        isPluginTaskOf(task, instance.id, kind) && task.kind.plugin_name === name,
                );
                if (!queued) await reloadInstalled();
            } catch (e) {
                const raw = errorText(e);
                pushErrorBar({
                    key: `${barKey}-op:${instance.id}:${name}`,
                    title: submitFailTitle,
                    raw: errorCopy ? errorCopy(raw) : raw,
                });
            } finally {
                setLocalBusy((prev) => {
                    const next = new Set(prev);
                    next.delete(name);
                    return next;
                });
            }
        },
        [barKey, errorCopy, instance.id, kind, reloadInstalled, resource, submitFailTitle],
    );

    const applyEnabled = useCallback(
        async (name: string, enabled: boolean, overwrite = false) => {
            try {
                await appFrameworkService.setPluginEnabled(
                    instance.id,
                    name,
                    enabled,
                    overwrite,
                    resource,
                );
                setConflict(null);
                await reloadInstalled();
            } catch (e) {
                if (isAppConfigError(e) && e.kind === 'conflict') {
                    setConflict({ name, enabled });
                    return;
                }
                pushErrorBar({
                    key: `${barKey}-enable:${instance.id}:${name}`,
                    title: '切换启用失败',
                    raw: errorText(e),
                });
            }
        },
        [barKey, instance.id, reloadInstalled, resource],
    );

    const resolveConflict = useCallback(
        async (overwrite: boolean) => {
            if (!conflict) return;
            setConflictBusy(true);
            try {
                if (!overwrite) {
                    setConflict(null);
                    await reloadInstalled();
                    return;
                }
                await applyEnabled(conflict.name, conflict.enabled, true);
            } finally {
                setConflictBusy(false);
            }
        },
        [applyEnabled, conflict, reloadInstalled],
    );

    return {
        taskHints,
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
