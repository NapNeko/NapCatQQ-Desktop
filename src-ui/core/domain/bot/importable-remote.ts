import type { ImportableRemoteBot } from '../../ipc/generated/domain/ImportableRemoteBot';

export function importableRemoteBotKey(row: ImportableRemoteBot): string {
    return `${row.serverId}::${row.backend}::${row.deployment}::${row.qqId}`;
}

export function importableBackendLabel(backend: ImportableRemoteBot['backend']): string {
    return backend === 'snowluma' ? 'SnowLuma' : 'NapCat';
}

export function importableDeploymentLabel(
    deployment: ImportableRemoteBot['deployment'],
): string {
    return deployment === 'docker' ? 'Docker' : '直接运行';
}

export function importableSourceLabel(source: ImportableRemoteBot['source']): string {
    switch (source) {
        case 'configFile':
            return '配置文件';
        case 'runtimeStatus':
            return '运行记录';
        case 'dockerContainer':
            return 'Docker 容器';
        default:
            return source;
    }
}
