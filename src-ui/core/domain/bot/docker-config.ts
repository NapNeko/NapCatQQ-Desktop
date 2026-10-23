// Bot 容器设置与最终端口预览；规则对应 Rust BotConfig::docker_deploy_spec。
import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import type { BotDockerConfig } from '../../ipc/generated/domain/BotDockerConfig';

export function defaultBotDockerConfig(): BotDockerConfig {
    return {
        portMappings: [],
        environment: {},
        restartPolicy: 'unless-stopped',
        publishOnAllInterfaces: false,
    };
}

export interface DockerPortRow {
    container: number;
    host: number;
    label: string;
    automatic: boolean;
    required: boolean;
}

export function dockerPortRows(config: BotConfig): DockerPortRow[] {
    const ports = new Map<number, string>(
        config.bot.backend_type === 'snowluma'
            ? [
                  [5900, 'VNC'],
                  [6081, 'noVNC'],
                  [5099, 'WebUI'],
                  [3000, 'OneBot HTTP'],
                  [3001, 'OneBot WS'],
              ]
            : [
                  [3000, 'OneBot HTTP'],
                  [3001, 'OneBot WS'],
                  [6099, 'WebUI'],
              ],
    );
    for (const connection of [
        ...config.connect.httpServers,
        ...config.connect.httpSseServers,
        ...config.connect.websocketServers,
    ]) {
        if (connection.enable && !ports.has(connection.port)) {
            ports.set(connection.port, connection.name || 'OneBot 监听');
        }
    }
    const mappings = config.docker?.portMappings ?? [];
    const offset = config.bot.QQID % 500;
    const rows = Array.from(ports, ([container, label]) => {
        const override = mappings.find((mapping) => mapping.container === container);
        return {
            container,
            host: override?.host ?? container + offset,
            label,
            automatic: !override,
            required: true,
        };
    });
    for (const mapping of mappings) {
        if (!ports.has(mapping.container)) {
            rows.push({ ...mapping, label: '额外映射', automatic: false, required: false });
        }
    }
    return rows;
}

export function setDockerPortMapping(
    settings: BotDockerConfig,
    container: number,
    host: number,
): BotDockerConfig {
    const others = settings.portMappings.filter((mapping) => mapping.container !== container);
    return { ...settings, portMappings: [...others, { container, host }] };
}

// 托管凭据、账号、用户及探针变量由部署层维护，不能被自定义项覆盖。
const RESERVED_ENVIRONMENT = new Set(['WEBUI_TOKEN', 'ACCOUNT', 'VNC_PASSWD', 'NODE_OPTIONS']);

export function validateDockerEnvironmentKey(key: string): string | null {
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key))
        return '变量名需以字母或下划线开头，仅含字母、数字和下划线';
    if (
        RESERVED_ENVIRONMENT.has(key) ||
        ['NAPCAT_', 'SNOWLUMA_', 'NCD_'].some((prefix) => key.startsWith(prefix))
    )
        return '此变量由 Desktop 管理，请使用其它变量名';
    return null;
}

export function validateBotDockerConfig(config: BotConfig): string | null {
    if (config.bot.deploymentType !== 'docker') return null;
    const settings = config.docker;
    const overrides = settings?.portMappings ?? [];
    const seenContainers = new Set<number>();
    for (const mapping of overrides) {
        if (!validPort(mapping.container) || !validPort(mapping.host))
            return 'Docker 端口必须是 1–65535 的整数';
        if (seenContainers.has(mapping.container))
            return `Docker 容器端口 ${mapping.container} 有重复映射`;
        seenContainers.add(mapping.container);
    }
    const seenHosts = new Set<number>();
    for (const row of dockerPortRows(config)) {
        if (!validPort(row.container)) return 'Docker 容器端口必须是 1–65535 的整数';
        if (!validPort(row.host))
            return `容器端口 ${row.container} 的自动宿主机端口超出范围，请在「容器」中设置映射`;
        if (seenHosts.has(row.host))
            return `Docker 宿主机端口 ${row.host} 有重复，请在「容器」中调整映射`;
        seenHosts.add(row.host);
    }
    if (settings) {
        if (!['unless-stopped', 'always', 'on-failure', 'no'].includes(settings.restartPolicy))
            return 'Docker 重启策略无效';
        if (
            settings.memoryMb != null &&
            (!Number.isInteger(settings.memoryMb) ||
                settings.memoryMb <= 0 ||
                settings.memoryMb > 4294967295)
        )
            return 'Docker 内存上限必须是正整数 MiB，留空表示不限';
        for (const key of Object.keys(settings.environment)) {
            const error = validateDockerEnvironmentKey(key);
            if (error) return `Docker 环境变量 ${key || '（空）'}：${error}`;
            if (settings.environment[key].includes('\0'))
                return `Docker 环境变量 ${key} 的值不能包含空字符`;
        }
    }
    return null;
}

function validPort(port: number): boolean {
    return Number.isInteger(port) && port > 0 && port <= 65535;
}
