import type { RemoteInventory } from '../../ipc/generated/domain/RemoteInventory';
import type { RemoteInventoryKind } from '../../ipc/generated/domain/RemoteInventoryKind';
import type { RemoteInventorySource } from '../../ipc/generated/domain/RemoteInventorySource';
import type { RemoteSelectedPaths } from '../../ipc/generated/domain/RemoteSelectedPaths';

export function inventoryKindLabel(kind: RemoteInventoryKind): string {
    switch (kind) {
        case 'qq':
            return 'QQ';
        case 'napcat':
            return 'NapCat';
        case 'snowluma':
            return 'SnowLuma';
        case 'nodejs':
            return 'Node.js';
        case 'ncd_watch':
            return 'ncd-watch';
        case 'docker_container':
            return 'Docker 容器';
        default:
            return kind;
    }
}

export function inventorySourceLabel(source: RemoteInventorySource): string {
    switch (source) {
        case 'userOverride':
            return '手动覆盖';
        case 'desktopOwned':
            return '桌面安装';
        case 'officialInstaller':
            return '官方安装器';
        case 'systemPackage':
            return '系统包';
        case 'pathLookup':
            return 'PATH';
        case 'process':
            return '运行中进程';
        default:
            return source;
    }
}

export function isInventoryItemSelected(
    kind: RemoteInventoryKind,
    root: string,
    selected: RemoteSelectedPaths,
): boolean {
    switch (kind) {
        case 'qq':
            return selected.qqInstallBase === root;
        case 'napcat':
            return selected.napcatRoot === root;
        case 'snowluma':
            return selected.snowlumaDir === root;
        case 'nodejs':
            return selected.nodeBin === root;
        case 'ncd_watch':
            return selected.ncdWatchRoot === root;
        case 'docker_container':
            return false;
        default:
            return false;
    }
}

export function inventorySummary(inv: RemoteInventory | null | undefined): string {
    if (!inv || inv.items.length === 0) {
        return '未发现安装，将按桌面默认路径安装';
    }
    const kinds = new Set(inv.items.filter((i) => i.verified).map((i) => i.kind));
    const labels = [...kinds].map(inventoryKindLabel);
    return `已发现 ${labels.join('、')}`;
}
