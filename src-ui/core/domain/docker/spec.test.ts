import { describe, expect, it } from 'vitest';

import type { DockerDeploySpec, DockerStatus } from '../../ipc/types';
import { defaultDeploySpec, portPurpose, validateDeploySpec } from './spec';
import { containerStateBadge, dockerStatusSummary, compactPorts } from './status';

describe('docker domain helpers', () => {
  it('builds independent default deploy specs for NapCat', () => {
    const first = defaultDeploySpec('napcat');
    const second = defaultDeploySpec('napcat');

    expect(first).toEqual({
      flavor: 'napcat',
      containerName: 'napcat',
      ports: [
        { host: 3000, container: 3000 },
        { host: 3001, container: 3001 },
        { host: 6099, container: 6099 },
      ],
      qqId: null,
    });

    first.ports[0].host = 18080;

    expect(second.ports[0]).toEqual({ host: 3000, container: 3000 });
  });

  it('validates deploy specs before submit', () => {
    const valid: DockerDeploySpec = {
      flavor: 'snowluma',
      containerName: 'snowluma-prod',
      ports: [
        { host: 5900, container: 5900 },
        { host: 6081, container: 6081 },
      ],
      qqId: null,
    };

    expect(validateDeploySpec(valid)).toBeNull();
    expect(validateDeploySpec({ ...valid, containerName: '-bad' })).toContain('容器名非法');
    expect(validateDeploySpec({ ...valid, ports: [] })).toContain('至少需要一个端口');
    expect(
      validateDeploySpec({
        ...valid,
        ports: [
          { host: 5900, container: 5900 },
          { host: 5900, container: 6081 },
        ],
      }),
    ).toContain('宿主机端口有重复');
  });

  it('summarizes docker readiness and port metadata', () => {
    const ready: DockerStatus = {
      installed: true,
      version: '27.3.1',
      composeAvailable: true,
      daemonRunning: true,
    };

    expect(dockerStatusSummary(ready)).toEqual({ ready: true, label: 'Docker 27.3.1 就绪' });
    expect(dockerStatusSummary({ ...ready, composeAvailable: false })).toEqual({
      ready: false,
      label: 'Docker 27.3.1 缺少 compose 插件',
    });
    expect(portPurpose(6099)?.label).toBe('WebUI');
    expect(containerStateBadge('running')).toEqual({ label: '运行中', tone: 'success' });
  });

  it('compacts duplicate IPv4 and IPv6 port rows', () => {
    expect(
      compactPorts([
        '0.0.0.0:6099->6099/tcp',
        '[::]:6099->6099/tcp',
        '0.0.0.0:3000->3000/tcp',
      ]),
    ).toEqual(['6099->6099/tcp', '3000->3000/tcp']);
  });
});
