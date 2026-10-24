import { describe, expect, it } from 'vitest';
import type { DockerStatus } from '../../ipc/types';
import { dockerSetupAction } from './setup';

const ready: DockerStatus = {
    installed: true,
    daemonRunning: true,
    composeAvailable: true,
    version: '27.3.1',
};

describe('Docker 环境补全动作', () => {
    it('未安装时先安装引擎，不受无意义的其余探测字段影响', () => {
        expect(dockerSetupAction({ ...ready, installed: false })).toEqual({
            label: '安装 Docker',
            pendingLabel: '正在安装',
        });
        expect(dockerSetupAction()).toEqual(dockerSetupAction({ ...ready, installed: false }));
    });

    it('引擎已装但未运行时先启动，不能误引导重装或只补 Compose', () => {
        expect(
            dockerSetupAction({ ...ready, daemonRunning: false, composeAvailable: false }),
        ).toEqual({ label: '启动 Docker', pendingLabel: '正在启动' });
    });

    it('引擎可用而 Compose 缺失时只补全部署依赖', () => {
        expect(dockerSetupAction({ ...ready, composeAvailable: false })).toEqual({
            label: '补全 Compose',
            pendingLabel: '正在补全',
        });
    });

    it('环境完整后不再提供安装或修复动作', () => {
        expect(dockerSetupAction(ready)).toBeNull();
    });
});
