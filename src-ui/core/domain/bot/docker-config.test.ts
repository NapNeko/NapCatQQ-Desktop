import { describe, expect, it } from 'vitest';
import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import type { HttpClientConfig } from '../../ipc/generated/domain/HttpClientConfig';
import type { HttpServerConfig } from '../../ipc/generated/domain/HttpServerConfig';
import type { HttpSseServerConfig } from '../../ipc/generated/domain/HttpSseServerConfig';
import type { WebsocketServerConfig } from '../../ipc/generated/domain/WebsocketServerConfig';
import type { WebsocketClientConfig } from '../../ipc/generated/domain/WebsocketClientConfig';
import { createDefaultBotConfig } from './config-defaults';
import { createDefaultConnection } from './connections';
import {
    defaultBotDockerConfig,
    dockerPortRows,
    setDockerPortMapping,
    validateBotDockerConfig,
} from './docker-config';

function dockerBot(backend: 'napcat' | 'snowluma' = 'napcat'): BotConfig {
    const config = createDefaultBotConfig();
    config.bot.QQID = 10001;
    config.bot.name = 'Docker Bot';
    config.bot.backend_type = backend;
    config.bot.deploymentType = 'docker';
    config.bot.runtime_target = 'test-server';
    return config;
}

function http(port: number, enable = true): HttpServerConfig {
    return {
        ...(createDefaultConnection('httpServer') as HttpServerConfig),
        name: `http-${port}`,
        port,
        enable,
    };
}

function sse(port: number): HttpSseServerConfig {
    return {
        ...(createDefaultConnection('httpSseServer') as HttpSseServerConfig),
        name: `sse-${port}`,
        port,
    };
}

function ws(port: number): WebsocketServerConfig {
    return {
        ...(createDefaultConnection('websocketServer') as WebsocketServerConfig),
        name: `ws-${port}`,
        port,
    };
}

describe('Docker Bot 自动端口', () => {
    it('默认只允许宿主机访问，开放外部访问需要显式设置', () => {
        expect(defaultBotDockerConfig().publishOnAllInterfaces).toBe(false);
    });

    it.each([
        ['napcat', [3000, 3001, 6099]],
        ['snowluma', [5900, 6081, 5099, 3000, 3001]],
    ] as const)('%s 保留默认端口并按 QQ 号分配宿主机端口', (backend, expected) => {
        const config = dockerBot(backend);
        const rows = dockerPortRows(config);
        expect(rows.map((row) => row.container).sort((a, b) => a - b)).toEqual(
            [...expected].sort((a, b) => a - b),
        );
        expect(rows.every((row) => row.host === row.container + 1)).toBe(true);
        expect(rows.every((row) => row.automatic)).toBe(true);
        expect(validateBotDockerConfig(config)).toBeNull();
    });

    it('HTTP、SSE 和 WS 监听自动发布且同一端口只出现一行', () => {
        const config = dockerBot();
        config.connect.httpServers = [http(8080), http(9000, false)];
        config.connect.httpSseServers = [sse(8081)];
        config.connect.websocketServers = [ws(8080), ws(8082)];
        config.connect.httpClients = [
            {
                ...(createDefaultConnection('httpClient') as HttpClientConfig),
                url: 'http://example.com:9001/',
            },
        ];
        config.connect.websocketClients = [
            {
                ...(createDefaultConnection('websocketClient') as WebsocketClientConfig),
                url: 'ws://example.com:9002/',
            },
        ];
        const rows = dockerPortRows(config);
        expect(rows.filter((row) => row.container === 8080)).toHaveLength(1);
        expect(rows).toEqual(
            expect.arrayContaining([
                expect.objectContaining({ container: 8080, host: 8081, automatic: true }),
                expect.objectContaining({ container: 8081, host: 8082, automatic: true }),
                expect.objectContaining({ container: 8082, host: 8083, automatic: true }),
            ]),
        );
        expect(rows.some((row) => row.container === 9000)).toBe(false);
        expect(rows.some((row) => row.container === 9001 || row.container === 9002)).toBe(false);
    });

    it('宿主机覆盖只修改指定端口，额外映射与其他容器设置保留', () => {
        const config = dockerBot();
        config.connect.httpServers = [http(8080)];
        const original = {
            ...defaultBotDockerConfig(),
            environment: { TZ: 'Asia/Shanghai' },
            memoryMb: 512,
            portMappings: [{ container: 9000, host: 19000 }],
        };
        const settings = setDockerPortMapping(original, 8080, 18080);
        config.docker = settings;
        expect(settings).not.toBe(original);
        expect(original.portMappings).toEqual([{ container: 9000, host: 19000 }]);
        expect(settings.environment).toEqual({ TZ: 'Asia/Shanghai' });
        expect(settings.memoryMb).toBe(512);
        expect(dockerPortRows(config)).toEqual(
            expect.arrayContaining([
                expect.objectContaining({ container: 8080, host: 18080, automatic: false }),
                expect.objectContaining({ container: 9000, host: 19000, automatic: false }),
            ]),
        );
        expect(validateBotDockerConfig(config)).toBeNull();
    });

    it('同一容器端口再次覆盖会替换旧设置', () => {
        let settings = setDockerPortMapping(defaultBotDockerConfig(), 3000, 13000);
        settings = setDockerPortMapping(settings, 3000, 23000);
        expect(settings.portMappings).toEqual([{ container: 3000, host: 23000 }]);
    });

    it('新增监听超过宿主机端口上限时提示错误，显式映射后可保存', () => {
        const config = dockerBot();
        config.connect.httpServers = [http(65535)];
        expect(validateBotDockerConfig(config)).not.toBeNull();
        config.docker = setDockerPortMapping(defaultBotDockerConfig(), 65535, 15000);
        expect(validateBotDockerConfig(config)).toBeNull();
        expect(dockerPortRows(config)).toEqual(
            expect.arrayContaining([
                expect.objectContaining({ container: 65535, host: 15000, automatic: false }),
            ]),
        );
    });
});

describe('Docker Bot 配置校验', () => {
    it('直接运行时不受遗留容器设置影响', () => {
        const config = dockerBot();
        config.bot.deploymentType = 'native';
        config.docker = {
            ...defaultBotDockerConfig(),
            portMappings: [{ container: 0, host: 0 }],
        };
        expect(validateBotDockerConfig(config)).toBeNull();
    });

    it('拒绝宿主机端口冲突，包括与自动管理端口的冲突', () => {
        const config = dockerBot();
        config.docker = {
            ...defaultBotDockerConfig(),
            portMappings: [{ container: 8080, host: 6100 }],
        };
        expect(validateBotDockerConfig(config)).not.toBeNull();
    });

    it('拒绝同一容器端口的重复手工映射', () => {
        const config = dockerBot();
        config.docker = {
            ...defaultBotDockerConfig(),
            portMappings: [
                { container: 8080, host: 18080 },
                { container: 8080, host: 28080 },
            ],
        };
        expect(validateBotDockerConfig(config)).not.toBeNull();
    });

    it.each([0, -1, 65536, 1.5])('拒绝无效端口 %s', (port) => {
        const config = dockerBot();
        config.docker = {
            ...defaultBotDockerConfig(),
            portMappings: [{ container: 8080, host: port }],
        };
        expect(validateBotDockerConfig(config)).not.toBeNull();
        config.docker.portMappings = [{ container: port, host: 18080 }];
        expect(validateBotDockerConfig(config)).not.toBeNull();
    });

    it('允许普通环境变量并拒绝非法变量名和受管凭据覆盖', () => {
        const config = dockerBot();
        config.docker = {
            ...defaultBotDockerConfig(),
            environment: { TZ: 'Asia/Shanghai', CUSTOM_VALUE: '$literal:#value' },
        };
        expect(validateBotDockerConfig(config)).toBeNull();
        for (const key of [
            'BAD-NAME',
            '1VALUE',
            'WEBUI_TOKEN',
            'ACCOUNT',
            'VNC_PASSWD',
            'NODE_OPTIONS',
            'NAPCAT_CUSTOM',
            'SNOWLUMA_CUSTOM',
            'NCD_CUSTOM',
        ]) {
            config.docker.environment = { [key]: 'value' };
            expect(validateBotDockerConfig(config)).not.toBeNull();
        }
        config.docker.environment = { CUSTOM_VALUE: '\0' };
        expect(validateBotDockerConfig(config)).not.toBeNull();
    });

    it('内存限制可留空，有值时须为正整数', () => {
        const config = dockerBot();
        config.docker = defaultBotDockerConfig();
        expect(validateBotDockerConfig(config)).toBeNull();
        config.docker.memoryMb = 512;
        expect(validateBotDockerConfig(config)).toBeNull();
        for (const memoryMb of [0, -1, 0.5, 4294967296]) {
            config.docker.memoryMb = memoryMb;
            expect(validateBotDockerConfig(config)).not.toBeNull();
        }
    });
});
