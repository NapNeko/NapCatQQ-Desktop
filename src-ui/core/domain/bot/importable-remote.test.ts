import { describe, expect, it } from 'vitest';
import type { ImportableRemoteBot } from '../../ipc/generated/domain/ImportableRemoteBot';
import {
    importableBackendLabel,
    importableDeploymentLabel,
    importableRemoteBotKey,
    importableSourceLabel,
} from './importable-remote';

const row: ImportableRemoteBot = {
    serverId: 's1',
    serverName: 'kunming',
    qqId: 10001,
    backend: 'snowluma',
    deployment: 'native',
    source: 'configFile',
    alreadyImported: false,
    selectable: true,
};

describe('importable remote bot labels', () => {
    it('builds a stable selection key', () => {
        expect(importableRemoteBotKey(row)).toBe('s1::snowluma::native::10001');
    });

    it('maps backend / deployment / source', () => {
        expect(importableBackendLabel('snowluma')).toBe('SnowLuma');
        expect(importableDeploymentLabel('docker')).toBe('Docker');
        expect(importableSourceLabel('configFile')).toBe('配置文件');
        expect(importableSourceLabel('dockerContainer')).toBe('Docker 容器');
    });
});
