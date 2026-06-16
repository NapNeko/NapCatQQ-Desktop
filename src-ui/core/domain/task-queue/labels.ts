// 任务队列展示用文案；不依赖 ComponentRow 反查。

import type { ComponentId } from '../../ipc/types';

const COMPONENT_DISPLAY_NAME: Record<ComponentId, string> = {
    napcat: 'NapCat',
    snowluma: 'SnowLuma',
    qq: 'QQ',
    nodejs: 'Node.js',
    novnc: 'noVNC',
    desktop_self: 'Desktop',
};

const STEP_KIND_LABEL: Record<string, string> = {
    ensure_installed: '安装',
    force_install: '安装',
    update: '更新',
    uninstall: '卸载',
    verify: '校验',
    ensure_dependencies: '补全系统依赖',
    install: '安装',
};

export function componentDisplayName(componentId: ComponentId): string {
    return COMPONENT_DISPLAY_NAME[componentId] ?? componentId;
}

export function componentActionTitle(
    componentId: ComponentId,
    stepKind: string | undefined,
    message: string,
): string {
    const name = componentDisplayName(componentId);
    const step = stepKind ? STEP_KIND_LABEL[stepKind] : undefined;
    if (step) return `${name} · ${step}`;
    const msg = message?.trim();
    if (msg) return `${name} · ${msg}`;
    return `${name} · 完成`;
}

export function dockerInstallTitle(hostLabel: string): string {
    const label = hostLabel?.trim() || '远程主机';
    return `Docker · ${label}`;
}

export function dockerDeployTitle(hostLabel: string, flavor?: string): string {
    const label = hostLabel?.trim() || '远程主机';
    const fw =
        flavor === 'napcat' ? 'NapCat' : flavor === 'snowluma' ? 'SnowLuma' : flavor?.trim();
    if (fw) return `拉取镜像 · ${fw} · ${label}`;
    return `拉取镜像 · ${label}`;
}