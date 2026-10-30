import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { featureTogglesStore } from '../preferences/featureTogglesStore';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { localOs } from '../../core/domain/runtime/env';
import {
    consumePendingBotCreateRequest,
    createDockerBotConfig,
    getPendingBotCreateRequest,
    openCreateDockerBot,
    registerBotCreateNavigator,
} from './botCreateNavigation';

vi.mock('../ui/globalInfoBarStore', () => ({ pushInfoBar: vi.fn() }));
vi.mock('../../core/domain/runtime/env', () => ({ localOs: vi.fn(() => 'windows') }));

const unregister: Array<() => void> = [];

function clearPending(): void {
    const pending = getPendingBotCreateRequest();
    if (pending) consumePendingBotCreateRequest(pending.requestId);
}

beforeEach(() => {
    vi.mocked(localOs).mockReturnValue('windows');
    featureTogglesStore._reset();
    clearPending();
});

afterEach(() => {
    for (const off of unregister.splice(0)) off();
    clearPending();
    featureTogglesStore._reset();
});

describe('Docker Bot 新建导航', () => {
    it('先保存主机和框架再跳转；消费后不会重复取到请求', () => {
        const seen: unknown[] = [];
        unregister.push(registerBotCreateNavigator(() => seen.push(getPendingBotCreateRequest())));
        openCreateDockerBot({ hostId: 'remote:one', flavor: 'napcat' });

        const pending = getPendingBotCreateRequest()!;
        expect(seen).toEqual([pending]);
        expect(pending).toMatchObject({ hostId: 'remote:one', flavor: 'napcat' });
        expect(consumePendingBotCreateRequest(pending.requestId)).toBe(true);
        expect(getPendingBotCreateRequest()).toBeNull();
        expect(consumePendingBotCreateRequest(pending.requestId)).toBe(false);
    });

    it('旧请求的迟到消费不能清除另一台主机的新请求', () => {
        openCreateDockerBot({ hostId: 'remote:one', flavor: 'napcat' });
        const first = getPendingBotCreateRequest()!;
        openCreateDockerBot({ hostId: 'remote:two', flavor: 'snowluma' });
        const second = getPendingBotCreateRequest()!;

        expect(second.requestId).toBeGreaterThan(first.requestId);
        expect(consumePendingBotCreateRequest(first.requestId)).toBe(false);
        expect(getPendingBotCreateRequest()).toBe(second);
        expect(consumePendingBotCreateRequest(second.requestId)).toBe(true);
    });

    it('StrictMode 旧注册的清理不会撤销后来注册的导航器', () => {
        const first = vi.fn();
        const second = vi.fn();
        const offFirst = registerBotCreateNavigator(first);
        unregister.push(offFirst, registerBotCreateNavigator(second));
        offFirst();
        openCreateDockerBot({ hostId: 'remote:one', flavor: 'napcat' });

        expect(first).not.toHaveBeenCalled();
        expect(second).toHaveBeenCalledOnce();
    });

    it.each(['local', 'remote:', 'remote:local', 'remote:remote'])(
        '拒绝无具体远端主机的 %s，并取消之前未完成的请求',
        (hostId) => {
            const navigate = vi.fn();
            unregister.push(registerBotCreateNavigator(navigate));
            openCreateDockerBot({ hostId: 'remote:one', flavor: 'napcat' });
            navigate.mockClear();
            openCreateDockerBot({ hostId, flavor: 'napcat' });

            expect(navigate).not.toHaveBeenCalled();
            expect(getPendingBotCreateRequest()).toBeNull();
            expect(pushInfoBar).toHaveBeenLastCalledWith(
                expect.objectContaining({ tone: 'warning', title: '无法新建 Docker Bot' }),
            );
        },
    );

    it('不能通过新建入口绕过已关闭的框架功能', () => {
        featureTogglesStore.apply({ napcat: false });
        const navigate = vi.fn();
        unregister.push(registerBotCreateNavigator(navigate));
        openCreateDockerBot({ hostId: 'remote:one', flavor: 'napcat' });

        expect(navigate).not.toHaveBeenCalled();
        expect(getPendingBotCreateRequest()).toBeNull();
        expect(pushInfoBar).toHaveBeenCalledWith(
            expect.objectContaining({ content: '请先在设置中启用 NapCat。' }),
        );
    });

    it.each(['napcat', 'snowluma'] as const)('预填 %s 的 Docker 配置并保留空账号', (flavor) => {
        const config = createDockerBotConfig({ hostId: 'remote:server-profile-id', flavor });
        expect(config.bot).toMatchObject({
            QQID: 0,
            name: '',
            backend_type: flavor,
            runtime_target: 'server-profile-id',
            deploymentType: 'docker',
        });
        expect(config.connect.websocketClients).toEqual([]);
        expect(config.advanced.autoStart).toBe(false);
        expect(() => createDockerBotConfig({ hostId: 'local', flavor })).toThrow('Linux 主机');
    });

    it('Linux 本机入口保留本机目标，允许新建容器 Bot', () => {
        vi.mocked(localOs).mockReturnValue('linux');
        openCreateDockerBot({ hostId: 'local', flavor: 'napcat' });
        expect(getPendingBotCreateRequest()).toMatchObject({ hostId: 'local' });
        expect(createDockerBotConfig({ hostId: 'local', flavor: 'napcat' }).bot).toMatchObject({
            runtime_target: 'local',
            deploymentType: 'docker',
        });
    });
});
