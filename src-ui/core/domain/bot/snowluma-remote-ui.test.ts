import { describe, expect, it } from 'vitest';
import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import {
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
});
