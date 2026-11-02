import { describe, expect, it } from 'vitest';
import { runtimeSelection, selectRuntimeHost, selectRuntimeMode } from './runtime-selection';

const config = {
    runtime_target: 'remote:win',
    deploymentType: 'docker' as const,
    backend_type: 'snowluma' as const,
};
const hosts = [
    { id: 'win', platform: 'windows' },
    { id: 'linux', platform: 'linux' },
];

describe('运行场景选择的配置约束', () => {
    it('旧配置的 remote: 前缀仍正确识别 Windows，并禁止选择容器', () => {
        const selected = runtimeSelection(config, hosts, true);
        expect(selected.selectedServerId).toBe('win');
        expect(selected.dockerAllowed).toBe(false);
        expect(selectRuntimeHost('win', hosts)).toEqual({
            runtime_target: 'win',
            deploymentType: 'native',
        });
    });

    it('切回不支持容器的本机清除容器方式；Linux 保留原选择', () => {
        expect(selectRuntimeMode('local', config, hosts, false)).toEqual({
            runtime_target: 'local',
            deploymentType: 'native',
        });
        expect(selectRuntimeMode('local', config, hosts, true)).toEqual({
            runtime_target: 'local',
            deploymentType: 'docker',
        });
    });

    it('重复选择远程模式保留已选主机，不把保存目标改成占位符', () => {
        const selected = { ...config, runtime_target: 'linux' };
        expect(selectRuntimeMode('remote', selected, hosts, true)).toEqual({});
    });

    it('唯一远端自动选择并应用其能力，多个远端保留待选状态', () => {
        expect(
            selectRuntimeMode('remote', { ...config, runtime_target: 'local' }, [hosts[0]], true),
        ).toEqual({
            runtime_target: 'win',
            deploymentType: 'native',
        });
        const patch = selectRuntimeMode(
            'remote',
            { ...config, runtime_target: 'local' },
            hosts,
            true,
        );
        const selected = runtimeSelection({ ...config, ...patch }, hosts, true);
        expect(selected.hostId).toBeNull();
        expect(selected.selectedServerId).toBeUndefined();
        expect(selected.showContainer).toBe(false);
    });
});
