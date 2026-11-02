import type { BotBasicConfig } from '../../ipc/generated/domain/BotBasicConfig';
import type { ServerProfile } from '../../ipc/generated/domain/ServerProfile';
import {
    RUNTIME_TARGET_REMOTE_PLACEHOLDER,
    isRuntimeTargetLocal,
    remoteHostIdFromRuntimeTarget,
    serverProfileIdFromRuntimeTarget,
    runtimeModeForTarget,
} from './runtime-target';

type RuntimeConfig = Pick<BotBasicConfig, 'runtime_target' | 'deploymentType' | 'backend_type'>;
type Host = Pick<ServerProfile, 'id' | 'platform'>;

/** 配置表单中的运行场景决策；此层不发探测请求。 */
export function runtimeSelection(
    data: RuntimeConfig,
    servers: readonly Host[],
    localDockerAllowed: boolean,
) {
    const isRemote = !isRuntimeTargetLocal(data.runtime_target);
    const profileId = serverProfileIdFromRuntimeTarget(data.runtime_target);
    const server = servers.find((item) => item.id === profileId);
    const remoteHostId = remoteHostIdFromRuntimeTarget(data.runtime_target);
    const dockerHostId = isRemote ? remoteHostId : localDockerAllowed ? 'local' : null;
    const dockerAllowed = isRemote ? server?.platform !== 'windows' : localDockerAllowed;
    return {
        mode: isRemote ? ('remote' as const) : ('local' as const),
        isRemote,
        selectedServerId: server?.id,
        hostId: isRemote ? remoteHostId : 'local',
        deploymentType: data.deploymentType,
        dockerAllowed,
        dockerHostId,
        showHostFields: isRemote || localDockerAllowed,
        showContainer: dockerAllowed && data.deploymentType === 'docker' && dockerHostId != null,
        showSnowLumaMode: data.backend_type === 'snowluma' && data.deploymentType !== 'docker',
    };
}

export function selectRuntimeHost(
    profileId: string,
    servers: readonly Host[],
): Partial<BotBasicConfig> {
    return {
        runtime_target: profileId,
        ...(servers.find((server) => server.id === profileId)?.platform === 'windows'
            ? { deploymentType: 'native' as const }
            : {}),
    };
}

export function selectRuntimeMode(
    mode: 'local' | 'remote',
    data: RuntimeConfig,
    servers: readonly Host[],
    localDockerAllowed: boolean,
): Partial<BotBasicConfig> {
    // 单选项重复点击不能丢掉已经选定的具体远端。
    if (mode === runtimeModeForTarget(data.runtime_target)) return {};
    if (mode === 'local')
        return {
            runtime_target: 'local',
            deploymentType: localDockerAllowed ? data.deploymentType : 'native',
        };
    return selectRuntimeHost(
        servers.length === 1 ? servers[0].id : RUNTIME_TARGET_REMOTE_PLACEHOLDER,
        servers,
    );
}

export function runtimeSceneExplanation(data: RuntimeConfig): string | null {
    if (!remoteHostIdFromRuntimeTarget(data.runtime_target)) return null;
    if (data.backend_type === 'napcat' && data.deploymentType === 'native')
        return 'Bot 启动后桌面经 SSH 隧道打开远端 NapCat WebUI（本机 6099）并轮询登录态；日志打出 WebUI 地址前先用持久化 token 尝试。';
    if (data.backend_type === 'snowluma' && data.deploymentType === 'native')
        return '桌面把远端 5099 / 6081 转发到本机回环，列表可开 WebUI 与 noVNC 扫码；登录密码来自远端 secret，打开时复制到剪贴板。冷/热启动由「启动模式」决定。';
    if (data.backend_type === 'snowluma' && data.deploymentType === 'docker')
        return '容器内自带 QQ 图形环境，登录在容器内完成；VNC 与 WebUI 密码由桌面写入 compose。列表在隧道就绪后可开 noVNC 扫码与 WebUI。';
    return null;
}
export type RuntimeSelection = ReturnType<typeof runtimeSelection>;
