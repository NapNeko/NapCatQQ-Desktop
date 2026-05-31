// ServerManager CRUD + 连接测试的 React 适配层。
// 远端组件部署走组件页 (run_component_action with host_id = "remote:<id>")。

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { serverService } from '../../core/services/server.service';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { errorText } from '../../core/domain/errors';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';
import type { ProbeReport } from '../../core/ipc/generated/domain/ProbeReport';

/// 档案显示名：优先 name，回退 host，最后裸 id。InfoBar 标题里用，让用户一眼
/// 认出是哪台机器，而不是看裸 UUID。
function serverLabel(p: { name?: string | null; host?: string | null; id?: string }): string {
    return p.name?.trim() || p.host?.trim() || p.id || '远端服务器';
}

export function useServerManager() {
    const queryClient = useQueryClient();

    const serversQuery = useQuery({
        queryKey: ['servers'],
        queryFn: () => serverService.list(),
    });

    const addMutation = useMutation({
        mutationFn: (args: { profile: ServerProfile; password?: string }) =>
            serverService.add(args.profile, args.password),
        onSuccess: (created) => {
            queryClient.invalidateQueries({ queryKey: ['servers'] });
            pushInfoBar({
                key: `server-add:${created.id}`,
                tone: 'success',
                title: '服务器已添加',
                content: `${serverLabel(created)} 已加入远端主机列表`,
                autoDismissMs: 4000,
            });
        },
        // 失败提示由调用方在 catch 里处理（RemoteHostPanel 的 addServerAsync
        // 链路要先关弹窗再决定是否接着配免密，错误上下文在那边更完整）。
    });

    const updateMutation = useMutation({
        mutationFn: (args: { profile: ServerProfile; password?: string }) =>
            serverService.update(args.profile, args.password),
        onSuccess: (updated) => {
            queryClient.invalidateQueries({ queryKey: ['servers'] });
            pushInfoBar({
                key: `server-update:${updated.id}`,
                tone: 'success',
                title: '服务器已更新',
                content: `${serverLabel(updated)} 的连接信息已保存`,
                autoDismissMs: 4000,
            });
        },
        onError: (err: unknown, args) => {
            pushInfoBar({
                key: `server-update:${args.profile.id}`,
                tone: 'danger',
                title: '更新服务器失败',
                content: errorText(err),
            });
        },
    });

    const deleteMutation = useMutation({
        mutationFn: (id: string) => serverService.delete(id),
        // delete 返回 void，但要在删除前抓一下显示名（onError 时还能用），所以从
        // 当前列表反查。删成功后列表会失效重拉，这里只为提示文案。
        onSuccess: (_void, id) => {
            const label = serverLabel(
                serversQuery.data?.find((s) => s.id === id) ?? { id },
            );
            queryClient.invalidateQueries({ queryKey: ['servers'] });
            pushInfoBar({
                key: `server-delete:${id}`,
                tone: 'success',
                title: '服务器已删除',
                content: `${label} 的档案与已保存凭据已清除`,
                autoDismissMs: 4000,
            });
        },
        onError: (err: unknown, id) => {
            pushInfoBar({
                key: `server-delete:${id}`,
                tone: 'danger',
                title: '删除服务器失败',
                content: errorText(err),
            });
        },
    });

    const testMutation = useMutation({
        mutationFn: (args: { id: string; password?: string }) =>
            serverService.testConnection(args.id, args.password),
        // 探测报告 success=false 不是 IPC 失败（命令本身成功返回了报告），所以
        // 走 onSuccess 分流：报告 success 才绿条，否则红条带后端给的 error 原因。
        onSuccess: (report, args) => {
            queryClient.invalidateQueries({ queryKey: ['servers'] });
            const label = serverLabel(
                serversQuery.data?.find((s) => s.id === args.id) ?? { id: args.id },
            );
            if (report.success) {
                const latency = `${report.latencyMs}ms`;
                pushInfoBar({
                    key: `server-test:${args.id}`,
                    tone: 'success',
                    title: '连接成功',
                    content: report.osInfo
                        ? `${label} · ${report.osInfo} · ${latency}`
                        : `${label} · ${latency}`,
                    autoDismissMs: 5000,
                });
            } else {
                pushInfoBar({
                    key: `server-test:${args.id}`,
                    tone: 'danger',
                    title: '连接失败',
                    content: report.error
                        ? `${label}：${report.error}`
                        : `${label}：无法建立 SSH 连接`,
                });
            }
        },
        onError: (err: unknown, args) => {
            pushInfoBar({
                key: `server-test:${args.id}`,
                tone: 'danger',
                title: '连接失败',
                content: errorText(err),
            });
        },
    });

    const setupKeyAuthMutation = useMutation({
        mutationFn: (args: { id: string; password: string }) =>
            serverService.setupKeyAuth(args.id, args.password),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ['servers'] });
        },
    });

    return {
        servers: serversQuery.data ?? [],
        isLoading: serversQuery.isLoading,
        refetch: serversQuery.refetch,

        addServer: addMutation.mutate,
        addServerAsync: addMutation.mutateAsync,
        isAdding: addMutation.isPending,

        updateServer: updateMutation.mutate,
        isUpdating: updateMutation.isPending,

        deleteServer: deleteMutation.mutate,
        isDeleting: deleteMutation.isPending,

        testConnection: testMutation.mutate,
        isTesting: testMutation.isPending,
        testResult: testMutation.data as ProbeReport | undefined,

        setupKeyAuth: setupKeyAuthMutation.mutateAsync,
        isSettingUpKey: setupKeyAuthMutation.isPending,
    };
}
