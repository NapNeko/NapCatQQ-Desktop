// Docker 容器 / 状态的展示派生纯函数。零 React / 零 tauri 依赖。

import type { ContainerInfo, ContainerState, DockerFlavor, DockerStatus } from '../../ipc/types';

/// 容器状态徽章:给 UI 一个语义色 + 中文标签。color 用中性语义名,
/// 具体映射到 Fluent / Tailwind 色由组件层决定(这层不碰样式库)。
export interface ContainerBadge {
    label: string;
    tone: 'success' | 'danger' | 'warning' | 'neutral';
}

export function containerStateBadge(state: ContainerState): ContainerBadge {
    switch (state) {
        case 'running':
            return { label: '运行中', tone: 'success' };
        case 'exited':
            return { label: '已停止', tone: 'neutral' };
        case 'created':
            return { label: '已创建', tone: 'neutral' };
        case 'restarting':
            return { label: '重启中', tone: 'warning' };
        case 'paused':
            return { label: '已暂停', tone: 'warning' };
        default:
            return { label: '未知', tone: 'danger' };
    }
}

/// 一句话概括 docker 是否能直接部署,给页面顶部状态条用。
export function dockerStatusSummary(status: DockerStatus): {
    ready: boolean;
    label: string;
} {
    if (!status.installed) {
        return { ready: false, label: '未安装 Docker' };
    }
    if (!status.daemonRunning) {
        return { ready: false, label: `Docker ${status.version} 已装，但守护进程未运行` };
    }
    if (!status.composeAvailable) {
        return { ready: false, label: `Docker ${status.version} 缺少 compose 插件` };
    }
    return { ready: true, label: `Docker ${status.version} 就绪` };
}

/// 判断一个容器名是不是本工程部署的 NapCat / SnowLuma(用于在容器列表里
/// 标记"这是我们部署的")。简单按镜像名前缀判断。
export function isManagedImage(image: string): boolean {
    return image.includes('napcat-docker') || image.includes('snowluma');
}

/// 判断一个容器是否属于指定 flavor(NapCat / SnowLuma)。按官方镜像 repo 名匹配:
/// NapCat -> napcat-docker, SnowLuma -> snowluma。框架行「Docker 部署」按钮据此
/// 判定该 flavor 是否已部署,已部署就禁用按钮避免重复部署撞容器名/端口。
export function containerMatchesFlavor(container: ContainerInfo, flavor: DockerFlavor): boolean {
    const image = container.image;
    if (flavor === 'napcat') return image.includes('napcat-docker');
    if (flavor === 'snowluma') return image.includes('snowluma');
    return false;
}

/// 端口文案去重压缩。
///
/// docker 对每个映射会同时列 IPv4 和 IPv6 两条(`0.0.0.0:18211->18211/tcp`
/// 和 `[::]:18211->18211/tcp`)，对用户是同一个端口。这里按"宿主端口->容器
/// 端口/协议"归一去重(丢掉绑定地址前缀)，让 napcat 那种 4 条塌成 2 条，
/// 既去噪又稳定卡片高度。
///
/// 返回精简后的展示文案数组，形如 `18211->18211/tcp`。
export function compactPorts(ports: string[]): string[] {
    const seen = new Set<string>();
    const out: string[] = [];
    for (const raw of ports) {
        // 去掉绑定地址前缀：`0.0.0.0:18211->...` / `[::]:18211->...` → `18211->...`
        // 匹配开头到"紧挨数字的那个冒号"，把 IPv4 / IPv6 绑定地址一并吃掉。
        const stripped = raw.replace(/^.*:(?=\d)/, '');
        const key = stripped || raw;
        if (seen.has(key)) continue;
        seen.add(key);
        out.push(key);
    }
    return out;
}
