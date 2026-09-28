// 应用端实例列表 + 操作的 React 适配层。
// useQuery 拉全量；app_instance_changed 由根上的 useAppInstanceEventsBridge 就地替换单条；
// 操作失败统一走 InfoBar。

import { useCallback } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { openExternalUrl } from '../../core/ipc/transport';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { errorText } from '../../core/domain/errors';
import { pushAppErrorBar } from './pushAppErrorBar';
import { useAppInstanceAlerts } from './useAppInstanceAlerts';
import { botConfigKey } from '../bot/useBotConfigsMap';
import { dropAppInstanceLogs, ensureAppInstanceLogStore } from './appInstanceLogStore';
import { showWebUiAccountDialog } from './webuiAccountDialogStore';
import { requestTermsConsent } from './termsDialogStore';
import { APP_INSTANCES_KEY, upsertInstance } from './appInstancesCache';
import type {
    AppFrameworkManifest,
    AppInstance,
    CreateAppInstanceRequest,
    ImportAppInstanceRequest,
} from '../../core/ipc/types';

export { useAppInstanceLog } from './appInstanceLogStore';

ensureAppInstanceLogStore();

export const APP_FRAMEWORKS_KEY = ['appFrameworks'] as const;
export { APP_INSTANCES_KEY };

export function useAppFrameworks() {
    return useQuery<AppFrameworkManifest[], Error>({
        queryKey: APP_FRAMEWORKS_KEY,
        queryFn: appFrameworkService.listFrameworks,
        staleTime: Infinity,
    });
}

export function useAppInstances() {
    const queryClient = useQueryClient();

    const query = useQuery<AppInstance[], Error>({
        queryKey: APP_INSTANCES_KEY,
        queryFn: appFrameworkService.listInstances,
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
        mutationFn: async (id: string): Promise<AppInstance | null> => {
            // 有上游条款的框架（MaiBot）：没同意过或更新后改过，先弹框；不同意就不启动
            const pending = await appFrameworkService.pendingTerms(id);
            if (pending.length) {
                const name =
                    queryClient.getQueryData<AppInstance[]>(APP_INSTANCES_KEY)?.find((i) => i.id === id)
                        ?.display_name ?? id;
                if (!(await requestTermsConsent(name, pending))) return null;
                await appFrameworkService.acceptTerms(id);
            }
            return appFrameworkService.start(id);
        },
        onSuccess: (inst) => {
            if (inst) patch(inst);
        },
        onError: (err, id) => fail('启动失败', `app-start:${id}`)(err),
    });

    const stopMutation = useMutation({
        mutationFn: (id: string) => appFrameworkService.stop(id),
        onSuccess: patch,
        onError: (err, id) => fail('停止失败', `app-stop:${id}`)(err),
    });

    // 开关先跟手翻过去，写失败再翻回来
    const autoStartMutation = useMutation({
        mutationFn: (args: { id: string; autoStart: boolean }) =>
            appFrameworkService.setInstanceAutoStart(args.id, args.autoStart),
        onMutate: ({ id, autoStart }) => {
            const prev = queryClient
                .getQueryData<AppInstance[]>(APP_INSTANCES_KEY)
                ?.find((i) => i.id === id);
            if (prev) patch({ ...prev, auto_start: autoStart });
            return prev;
        },
        onSuccess: patch,
        onError: (err, args, prev) => {
            if (prev) patch(prev);
            fail('自动启动没改成', `app-auto-start:${args.id}`)(err);
        },
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
        onSuccess: (inst) => {
            // 实例列表和应用端配置由事件桥跟着 unlinked 事件更新；解绑改了 Bot 的连接表，Bot 配置缓存这里失效
            queryClient.invalidateQueries({ queryKey: ['botConfig'] });
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

    const openWebUi = useCallback(async (id: string, path?: string) => {
        try {
            const { url, authKey, account } = await appFrameworkService.webui(id, path);
            if (account) {
                const name = queryClient
                    .getQueryData<AppInstance[]>(APP_INSTANCES_KEY)
                    ?.find((i) => i.id === id)?.display_name;
                showWebUiAccountDialog({ instanceId: id, instanceName: name ?? id, url, account });
            }
            const key = authKey.trim();
            if (key) {
                try {
                    await navigator.clipboard.writeText(key);
                    pushInfoBar({
                        key: `app-webui-copied:${id}`,
                        tone: 'success',
                        title: '密钥已复制',
                        content: '粘贴到 WebUI 登录',
                        autoDismissMs: 3000,
                    });
                } catch (e) {
                    console.warn('WebUI 密钥写入剪贴板失败:', e);
                    pushInfoBar({
                        key: `app-webui-copy-fail:${id}`,
                        tone: 'warning',
                        title: '未能复制密钥',
                        content: '到连接页查看 HTTP 鉴权密钥',
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
    }, [queryClient]);

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
        setAutoStart: autoStartMutation.mutate,
        remove: deleteMutation.mutateAsync,
        isRemoving: deleteMutation.isPending,
        unlink: unlinkMutation.mutate,
        openWebUi,

        /** 只有「重新探测」在跑的实例：刷新图标自己转，不再另挂一个转圈 */
        refreshingId: refreshMutation.isPending ? refreshMutation.variables : null,
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
