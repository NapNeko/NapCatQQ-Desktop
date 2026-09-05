import { describe, expect, it } from 'vitest';
import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import {
    isSnowlumaRemoteUiRetryAvailable,
    snowlumaDaemonScope,
    snowlumaDaemonStateForConfig,
} from './snowluma-remote-ui';

function config(runtimeTarget: string, deploymentType: 'native' | 'docker'): BotConfig {
    return {
        bot: {
            backend_type: 'snowluma',
            runtime_target: runtimeTarget,
            deploymentType,
        },
    } as BotConfig;
}

describe('SnowLuma daemon scope', () => {
    it('本机与远端 Native 读取各自主机状态', () => {
        const states = { local: 'ready', 'server-a': 'crashed' } as const;

        expect(snowlumaDaemonScope(config('local', 'native'))).toBe('local');
        expect(
            snowlumaDaemonStateForConfig(config('local', 'native'), states),
        ).toBe('ready');
        expect(
            snowlumaDaemonStateForConfig(config('server-a', 'native'), states),
        ).toBe('crashed');
    });

    it('Docker 不读取共享 daemon 状态', () => {
        expect(
            snowlumaDaemonStateForConfig(
                config('server-a', 'docker'),
                { 'server-a': 'crashed' },
            ),
        ).toBeNull();
    });

    it('远端运行中的 SnowLuma 始终提供 UI 隧道重试入口', () => {
        expect(
            isSnowlumaRemoteUiRetryAvailable({
                config: config('server-a', 'native'),
                active: true,
                transportFailed: false,
            }),
        ).toBe(true);
        expect(
            isSnowlumaRemoteUiRetryAvailable({
                config: config('server-a', 'docker'),
                active: true,
                transportFailed: false,
            }),
        ).toBe(true);
        expect(
            isSnowlumaRemoteUiRetryAvailable({
                config: config('server-a', 'native'),
                active: false,
                transportFailed: false,
            }),
        ).toBe(false);
        expect(
            isSnowlumaRemoteUiRetryAvailable({
                config: config('server-a', 'native'),
                active: true,
                transportFailed: true,
            }),
        ).toBe(false);
    });
});
