import { describe, expect, it } from 'vitest';
import type { ComponentDetectResult, ComponentInfo } from '../../ipc/types';
import { deriveStatus, type HostInfo } from './types';

const host: HostInfo = {
    host_id: 'local',
    display_name: '本机',
    os: 'windows',
    locality: 'local',
};

const nodeInfo: ComponentInfo = {
    id: 'nodejs',
    display_name: 'Node.js',
    description: '',
    supported_targets: [{ os: 'windows', locality: 'local' }],
    category: 'runtime_dep',
};

function detect(partial: Partial<ComponentDetectResult>): ComponentDetectResult {
    return { component_id: 'nodejs', host_id: 'local', supported: true, ...partial };
}

describe('deriveStatus', () => {
    it('maps detected to installed', () => {
        const status = deriveStatus(
            host,
            nodeInfo,
            detect({ detected: { version: '22.13.0', source: '$PATH/node' } }),
        );
        expect(status).toEqual({
            state: 'installed',
            detected: { version: '22.13.0', source: '$PATH/node' },
        });
    });

    it('maps unusable (no detected) to its own state instead of not_installed', () => {
        const unusable = {
            source: '$PATH/node',
            version: '18.19.1',
            reason: 'v18.19.1 不满足 ^22.13.0 || >=23.4.0',
        };
        expect(deriveStatus(host, nodeInfo, detect({ unusable }))).toEqual({
            state: 'unusable',
            unusable,
        });
    });

    it('falls back to not_installed when neither detected nor unusable', () => {
        expect(deriveStatus(host, nodeInfo, detect({}))).toEqual({ state: 'not_installed' });
    });
});
