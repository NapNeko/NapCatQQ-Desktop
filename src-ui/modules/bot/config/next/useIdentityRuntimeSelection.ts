import { useEffect, useMemo } from 'react';
import { useServers } from '../../../../hooks/remote/useServers';
import { localOs } from '../../../../core/domain/runtime/env';
import {
    runtimeSelection,
    runtimeSceneExplanation,
    selectRuntimeMode,
    selectRuntimeHost,
} from '../../../../core/domain/bot/runtime-selection';
import type { BotBasicConfig } from '../../../../core/ipc/generated/domain/BotBasicConfig';

export function useIdentityRuntimeSelection(
    data: BotBasicConfig,
    onChange: (patch: Partial<BotBasicConfig>) => void,
) {
    const { data: servers = [], isLoading } = useServers();
    const localDockerAllowed = localOs() === 'linux';
    const selection = runtimeSelection(data, servers, localDockerAllowed);
    // 主机平台可在档案异步加载/探测后才确定；禁用按钮之外也要修正待保存值。
    useEffect(() => {
        if (data.deploymentType === 'docker' && !selection.dockerAllowed)
            onChange({ deploymentType: 'native' });
    }, [data.deploymentType, selection.dockerAllowed, onChange]);
    const serverItems = useMemo(
        () =>
            servers.map((server) => ({
                value: server.id,
                label: `${server.name} · ${server.host}`,
            })),
        [servers],
    );
    const selectedServer = servers.find((server) => server.id === selection.selectedServerId);
    return {
        selection,
        serverItems,
        noRemoteHosts: selection.isRemote && servers.length === 0 && !isLoading,
        transportFailed:
            selection.isRemote &&
            selection.hostId != null &&
            (!selectedServer || selectedServer.state === 'failed'),
        sceneExplanation: runtimeSceneExplanation(data),
        selectMode: (mode: 'local' | 'remote') =>
            onChange(selectRuntimeMode(mode, data, servers, localDockerAllowed)),
        selectHost: (profileId: string) => onChange(selectRuntimeHost(profileId, servers)),
        selectDeployment: (deploymentType: BotBasicConfig['deploymentType']) =>
            onChange({ deploymentType }),
    };
}
