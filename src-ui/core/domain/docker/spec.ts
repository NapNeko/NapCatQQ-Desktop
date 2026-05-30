// 部署 spec 的默认值生成 + 校验纯函数。零 React / 零 tauri 依赖。

import type { DockerDeploySpec, DockerFlavor, PortMapping } from '../../ipc/types';

/// NapCat 默认端口:3000(HTTP) / 3001(WS) / 6099(WebUI)。
const NAPCAT_DEFAULT_PORTS: PortMapping[] = [
    { host: 3000, container: 3000 },
    { host: 3001, container: 3001 },
    { host: 6099, container: 6099 },
];

/// SnowLuma 默认端口:5900(VNC) / 6081(noVNC) / 5099(WebUI) / 3000 / 3001。
const SNOWLUMA_DEFAULT_PORTS: PortMapping[] = [
    { host: 5900, container: 5900 },
    { host: 6081, container: 6081 },
    { host: 5099, container: 5099 },
    { host: 3000, container: 3000 },
    { host: 3001, container: 3001 },
];

/// 生成某口味的默认部署 spec,前端表单初值用。
export function defaultDeploySpec(flavor: DockerFlavor): DockerDeploySpec {
    if (flavor === 'napcat') {
        return {
            flavor: 'napcat',
            containerName: 'napcat',
            ports: NAPCAT_DEFAULT_PORTS.map((p) => ({ ...p })),
            qqId: null,
        };
    }
    return {
        flavor: 'snowluma',
        containerName: 'snowluma',
        ports: SNOWLUMA_DEFAULT_PORTS.map((p) => ({ ...p })),
        qqId: null,
    };
}

/// 端口用途说明：容器端口 → 一句话用途。给部署对话框每行端口做标签，让用户
/// 知道为什么要映射这个端口、冲突时该不该改。容器端口固定（镜像 EXPOSE 写死），
/// 用户只改宿主机端口，所以按容器端口查表即可。
///
/// 端口含义来自两个上游 docker 框架：
///   NapCat-Docker：3000 OneBot HTTP / 3001 OneBot WS / 6099 WebUI
///   SnowLuma.Docker.Framework：5900 VNC / 6081 noVNC / 5099 WebUI / 3000 HTTP / 3001 WS
export interface PortPurpose {
    label: string;
    description: string;
}

const PORT_PURPOSES: Record<number, PortPurpose> = {
    3000: { label: 'OneBot HTTP', description: 'OneBot v11 HTTP 接口，机器人框架对接用' },
    3001: { label: 'OneBot WS', description: 'OneBot v11 WebSocket 接口，机器人框架对接用' },
    6099: { label: 'WebUI', description: 'NapCat 管理后台，浏览器访问扫码登录 / 改配置' },
    5099: { label: 'WebUI', description: 'SnowLuma 管理后台，浏览器访问改配置' },
    5900: { label: 'VNC', description: '远程桌面（VNC 客户端连），扫码登录 QQ 用' },
    6081: { label: 'noVNC', description: '网页版远程桌面，浏览器直接打开扫码登录 QQ' },
};

export function portPurpose(containerPort: number): PortPurpose | null {
    return PORT_PURPOSES[containerPort] ?? null;
}

/// 容器名是否合法(与后端 DockerDeploySpec::validate 同规则:首字符字母数字,
/// 其余可含 _.-)。给表单做即时校验,后端仍会再校一次。
export function isValidContainerName(name: string): boolean {
    const trimmed = name.trim();
    if (trimmed.length === 0) return false;
    return /^[a-zA-Z0-9][a-zA-Z0-9_.-]*$/.test(trimmed);
}

/// 前端提交前的整体校验,返回第一条错误文案(null 表示通过)。
export function validateDeploySpec(spec: DockerDeploySpec): string | null {
    if (!isValidContainerName(spec.containerName)) {
        return '容器名非法：需以字母或数字开头，仅可含字母数字和 _.- ';
    }
    if (spec.ports.length === 0) {
        return '至少需要一个端口映射';
    }
    const hostPorts = spec.ports.map((p) => p.host);
    if (new Set(hostPorts).size !== hostPorts.length) {
        return '宿主机端口有重复，请检查';
    }
    return null;
}
