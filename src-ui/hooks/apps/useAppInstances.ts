// 应用端实例列表 + 操作的 React 适配层。
// useQuery 拉全量，app_instance_changed 事件就地替换单条；操作失败统一走 InfoBar。

import { useCallback } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { openExternalUrl } from '../../core/ipc/transport';
import { useDomainEvents } from '../events/useDomainEvents';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { errorText } from '../../core/domain/errors';
import { pushAppErrorBar } from './pushAppErrorBar';
import { useAppInstanceAlerts } from './useAppInstanceAlerts';
import { botConfigKey } from '../bot/useBotConfigsMap';
import { dropAppInstanceLogs, ensureAppInstanceLogStore } from './appInstanceLogStore';
import { matchesAppInstallTask } from '../../modules/apps/instanceState';
import type {
    AppFrameworkManifest,
    AppInstance,
    CreateAppInstanceRequest,
    ImportAppInstanceRequest,
} from '../../core/ipc/types';

export { useAppInstanceLog } from './appInstanceLogStore';

ensureAppInstanceLogStore();

export const APP_FRAMEWORKS_KEY = ['appFrameworks'] as const;
export const APP_INSTANCES_KEY = ['appInstances'] as const;

export function useAppFrameworks() {
    return useQuery<AppFrameworkManifest[], Error>({
        queryKey: APP_FRAMEWORKS_KEY,
        queryFn: appFrameworkService.listFrameworks,
        staleTime: Infinity,
    });
}

function upsertInstance(list: AppInstance[] | undefined, next: AppInstance): AppInstance[] {
    if (!list?.length) return [next];
    const idx = list.findIndex((i) => i.id === next.id);
    if (idx < 0) return [...list, next];
    return list.map((i) => (i.id === next.id ? next : i));
}

export function useAppInstances() {
    const queryClient = useQueryClient();

    const query = useQuery<AppInstance[], Error>({
        queryKey: APP_INSTANCES_KEY,
        queryFn: appFrameworkService.listInstances,
    });

    useDomainEvents((event) => {
        if (event.kind === 'app_instance_changed') {
            queryClient.setQueryData<AppInstance[]>(APP_INSTANCES_KEY, (old) =>
                upsertInstance(old, event.instance),
            );
            const reason = event.reason ?? '';
            if (reason === 'linked' || reason === 'unlinked' || reason === 'port_changed') {
                queryClient.invalidateQueries({ queryKey: ['appInstanceConfig', event.instance.id] });
                queryClient.invalidateQueries({ queryKey: ['appConfigText', event.instance.id] });
            }
            return;
        }
        if (event.kind !== 'deployment_task_changed') return;
        const { task } = event;
        if (task.status !== 'success' && task.status !== 'failed' && task.status !== 'cancelled') {
            return;
        }
        queryClient.setQueryData<AppInstance[]>(APP_INSTANCES_KEY, (old) => {
            if (!old?.length) return old;
            const hit = old.find((instance) => matchesAppInstallTask(task, instance));
            if (!hit || hit.state !== 'installing') return old;
            if (task.status === 'success') {
                return upsertInstance(old, { ...hit, state: 'installed', last_error: undefined });
            }
            return upsertInstance(old, {
                ...hit,
                state: 'not_installed',
                last_error: task.error ?? hit.last_error,
            });
        });
    });

    const patch = useCallback(
        (next: AppInstance) => {
            queryClient.setQueryData<AppInstance[]>(APP_INSTANCES_KEY, (old) =>
                upsertInstance(old, next),
            );
        },
        [queryClient],
    );

    useAppInstanceAlerts(query.data ?? [], query.error ? errorText(query.error) : null);

    const fail = (title: string, key: string) => (err: unknown) => {
        pushAppErrorBar({ key, title, raw: errorText(err) });
    };

    const createMutation = useMutation({
        mutationFn: (req: CreateAppInstanceRequest) => appFrameworkService.create(req),
        onSuccess: patch,
        onError: fail('新建应用实例失败', 'app-create'),
    });

    const importMutation = useMutation({
        mutationFn: (req: ImportAppInstanceRequest) => appFrameworkService.importInstance(req),
        onSuccess: patch,
        onError: fail('导入应用实例失败', 'app-import'),
    });

    const installMutation = useMutation({
        mutationFn: (id: string) => appFrameworkService.install(id),
        onSuccess: (_taskId, id) => {
            pushInfoBar({
                key: `app-install:${id}`,
                tone: 'info',
                title: '安装任务已提交',
                content: '进度在任务队列查看；完成后实例状态自动更新。',
                autoDismissMs: 4000,
            });
        },
        onError: (err, id) => fail('提交安装失败', `app-install:${id}`)(err),
    });

    const startMutation = useMutation({
        mutationFn: (id: string) => appFrameworkService.start(id),
        onSuccess: (inst) => {
            patch(inst);
        },
        onError: (err, id) => fail('启动失败', `app-start:${id}`)(err),
    });

    const stopMutation = useMutation({
        mutationFn: (id: string) => appFrameworkService.stop(id),
        onSuccess: patch,
        onError: (err, id) => fail('停止失败', `app-stop:${id}`)(err),
    });

    const refreshMutation = useMutation({
        mutationFn: (id: string) => appFrameworkService.refresh(id),
        onSuccess: patch,
        onError: (err, id) => fail('刷新失败', `app-refresh:${id}`)(err),
    });

    const deleteMutation = useMutation({
        mutationFn: (args: { id: string; removeFiles: boolean }) =>
            appFrameworkService.delete(args.id, args.removeFiles),
        onSuccess: (_void, args) => {
            dropAppInstanceLogs(args.id);
            queryClient.setQueryData<AppInstance[]>(APP_INSTANCES_KEY, (old) =>
                (old ?? []).filter((i) => i.id !== args.id),
            );
        },
        onError: (err, args) => fail('注销失败', `app-delete:${args.id}`)(err),
    });

    const unlinkMutation = useMutation({
        mutationFn: (id: string) => appFrameworkService.unlink(id),
        onSuccess: (inst, _id, _ctx) => {
            patch(inst);
            // 解绑改了 Bot 的连接表，Bot 配置缓存要失效
            queryClient.invalidateQueries({ queryKey: ['botConfig'] });
            queryClient.invalidateQueries({ queryKey: ['appInstanceConfig', inst.id] });
            queryClient.invalidateQueries({ queryKey: ['appConfigText', inst.id] });
            pushInfoBar({
                key: `app-unlink:${inst.id}`,
                tone: 'success',
                title: '已解除对接',
                content: `${inst.display_name} 与协议 Bot 的连接已移除`,
                autoDismissMs: 3000,
            });
        },
        onError: (err, id) => fail('解除对接失败', `app-unlink:${id}`)(err),
    });

    const openWebUi = useCallback(async (id: string) => {
        try {
            const { url, authKey } = await appFrameworkService.webui(id);
            const key = authKey.trim();
            if (key) {
                try {
                    await navigator.clipboard.writeText(key);
                    pushInfoBar({
                        key: `app-webui-copied:${id}`,
                        tone: 'success',
                        title: '密钥已复制',
                        content: '粘贴即可登录 WebUI',
                        autoDismissMs: 3000,
                    });
                } catch (e) {
                    console.warn('WebUI 密钥写入剪贴板失败:', e);
                    pushInfoBar({
                        key: `app-webui-copy-fail:${id}`,
                        tone: 'warning',
                        title: '未能复制密钥',
                        content: '请到连接页查看 HTTP 鉴权密钥',
                    });
                }
            }
            await openExternalUrl(url);
        } catch (err) {
            pushAppErrorBar({
                key: `app-webui:${id}`,
                title: '打开 WebUI 失败',
                raw: errorText(err),
            });
        }
    }, []);

    return {
        instances: query.data ?? [],
        isLoading: query.isLoading,
        error: query.error ? errorText(query.error) : null,
        refetch: query.refetch,
        patch,

        create: createMutation.mutateAsync,
        isCreating: createMutation.isPending,
        importInstance: importMutation.mutateAsync,
        isImporting: importMutation.isPending,
        install: installMutation.mutate,
        start: startMutation.mutate,
        stop: stopMutation.mutate,
        refresh: refreshMutation.mutate,
        remove: deleteMutation.mutateAsync,
        isRemoving: deleteMutation.isPending,
        unlink: unlinkMutation.mutate,
        openWebUi,

        pendingId:
            startMutation.isPending
                ? startMutation.variables
                : stopMutation.isPending
                  ? stopMutation.variables
                  : refreshMutation.isPending
                    ? refreshMutation.variables
                    : installMutation.isPending
                      ? installMutation.variables
                      : unlinkMutation.isPending
                        ? unlinkMutation.variables
                        : null,
    };
}

/// 对接成功后让 Bot 配置缓存失效（连接表变了）。
export function invalidateBotConfigAfterLink(
    queryClient: ReturnType<typeof useQueryClient>,
    botId: string,
) {
    queryClient.invalidateQueries({ queryKey: botConfigKey(botId) });
}
