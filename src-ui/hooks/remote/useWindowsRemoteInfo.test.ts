import { describe, expect, it } from 'vitest';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';
import { shouldProbeWindowsRemote, windowsRemoteInfoQueryKey } from './useWindowsRemoteInfo';

const profile: ServerProfile = {
    id: 'host-a',
    name: 'Windows',
    host: '192.0.2.1',
    port: 22,
    username: 'operator',
    platform: null,
    authMethod: 'key',
    rememberCredential: false,
    state: 'connected',
};

describe('Windows remote information scope', () => {
    it('probes connected auto and Windows profiles but never explicit Linux or offline profiles', () => {
        expect(shouldProbeWindowsRemote(profile)).toBe(true);
        expect(shouldProbeWindowsRemote({ ...profile, platform: 'windows' })).toBe(true);
        expect(shouldProbeWindowsRemote({ ...profile, platform: 'linux' })).toBe(false);
        expect(shouldProbeWindowsRemote({ ...profile, state: 'disconnected' })).toBe(false);
    });

    it('isolates cached capabilities when an existing profile changes target or identity', () => {
        const key = windowsRemoteInfoQueryKey(profile);
        for (const change of [
            { host: '192.0.2.2' },
            { port: 2222 },
            { username: 'other' },
            { platform: 'linux' },
        ]) {
            expect(windowsRemoteInfoQueryKey({ ...profile, ...change })).not.toEqual(key);
        }
    });
});
