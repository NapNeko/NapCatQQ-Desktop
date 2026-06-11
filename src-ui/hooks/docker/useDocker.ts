// Docker 管理面 React 适配层。
//
// 接口速查（写 UI 时照着接）：
//   入参：hostId（"local" 或 "remote:<serverId>"）
//   返回：
//     status            DockerStatus | undefined   当前主机的 docker 探测结果
//     isProbing         boolean                    probe 是否进行中
//     containers        ContainerInfo[]            容器列表（含已停止）
//     isLoadingList     boolean
//     refetch()                                    手动刷新 probe + 列表
//     install()         → Promise<string>          帮装 docker，返回结果文案
//     isInstalling      boolean
//     deploy(spec)      → Promise<DeployedContainer>一键部署 NapCat/SnowLuma
//     isDeploying       boolean
//     containerAction({name, action})              start/stop/restart/remove
//     isActing          boolean
//     composeDown({name, removeVolumes})           停并清理一个部署
//     isComposingDown   boolean
//     fetchLogs(name, tail?) → Promise<string>     取容器日志（命令式调用）

import { useCallback } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';

import { dockerService } from '../../core/services/docker.service';
import { dockerActionStore } from './dockerActionStore';
import { dockerInstallProgressStore } from './dockerInstallProgressStore';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { errorText } from '../../core/domain/errors';
import type {
    ContainerAction,
    ContainerInfo,
    DockerDeploySpec,
    DockerStatus,
} from '../../core/ipc/types';

/// 容器操作动词。InfoBar 标题用，比裸 action 字面量友好。
const ACTION_VERB: Record<ContainerAction, string> = {
    start: '启动',
    stop: '停止',
    restart: '重启',
    remove: '删除',
};

export function useDocker(hostId: string) {
    const queryClient = useQueryClient();

    const statusQuery = useQuery({
        queryKey: ['docker', 'status', hostId],
        queryFn: () => dockerService.probe(hostId),
    });

    const containersQuery = useQuery({
        queryKey: ['docker', 'containers', hostId],
        queryFn: () => dockerService.listContainers(hostId),
        // daemon 没起时列容器会失败，禁用查询避免反复报错。
        enabled: statusQuery.data?.daemonRunning ?? false,
    });

    const invalidate = useCallback(() => {
        queryClient.invalidateQueries({ queryKey: ['docker', 'status', hostId] });
        queryClient.invalidateQueries({ queryKey: ['docker', 'containers', hostId] });
    }, [queryClient, hostId]);

    const installMutation = useMutation({
        mutationFn: async () => {
            const taskId = crypto.randomUUID();
            dockerInstallProgressStore.started(taskId, hostId);
            dockerActionStore.markInstalling(hostId, taskId);
            return dockerService.install(hostId, taskId);
        },
        onSuccess: invalidate,
        onSettled: () => dockerActionStore.clearInstalling(hostId),
    });

    const deployMutation = useMutation({
        mutationFn: (args: { spec: DockerDeploySpec; taskId?: string }) =>
            dockerService.deploy(hostId, args.spec, args.taskId ?? crypto.randomUUID()),
        onSuccess: invalidate,
    });

    const actionMutation = useMutation({
        mutationFn: (args: { name: string; action: ContainerAction }) =>
            dockerService.containerAction(hostId, args.name, args.action),
        onSuccess: (_void, { name, action }) => {
            invalidate();
            const verb = ACTION_VERB[action];
            pushInfoBar({
                key: `container-action:${hostId}:${name}`,
                tone: 'success',
                title: `容器已${verb}`,
                content: name,
                autoDismissMs: 4000,
            });
        },
        onError: (err: unknown, { name, action }) => {
            pushInfoBar({
                key: `container-action:${hostId}:${name}`,
                tone: 'danger',
                title: `容器${ACTION_VERB[action]}失败`,
                content: `${name}：${errorText(err)}`,
            });
        },
    });

    const composeDownMutation = useMutation({
        mutationFn: (args: { name: string; removeVolumes: boolean }) =>
            dockerService.composeDown(hostId, args.name, args.removeVolumes),
        onSuccess: invalidate,
    });

    const fetchLogs = useCallback(
        (name: string, tail = 400): Promise<string> => dockerService.logs(hostId, name, tail),
        [hostId],
    );

    return {
        status: statusQuery.data as DockerStatus | undefined,
        isProbing: statusQuery.isLoading,

        containers: (containersQuery.data ?? []) as ContainerInfo[],
        isLoadingList: containersQuery.isLoading,

        refetch: () => {
            invalidate();
        },

        install: installMutation.mutateAsync,
        isInstalling: installMutation.isPending,

        // deploy 暴露的接口只接受 spec，taskId 内部自动生成（这条路径没有进度 UI）。
        deploy: (spec: DockerDeploySpec) =>
            deployMutation.mutateAsync({ spec, taskId: crypto.randomUUID() }),
        isDeploying: deployMutation.isPending,

        containerAction: actionMutation.mutate,
        isActing: actionMutation.isPending,

        composeDown: composeDownMutation.mutate,
        isComposingDown: composeDownMutation.isPending,

        fetchLogs,
    };
}
