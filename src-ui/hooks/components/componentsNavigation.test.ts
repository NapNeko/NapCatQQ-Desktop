// @vitest-environment jsdom
import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
    consumeDockerDeployment,
    getPendingDockerDeployment,
    openDockerDeployment,
    registerComponentsNavigator,
    subscribeDockerDeployment,
    usePendingDockerDeployment,
} from './componentsNavigation';

const disposers: Array<() => void> = [];

afterEach(() => {
    cleanup();
    for (const dispose of disposers.splice(0)) dispose();
    const pending = getPendingDockerDeployment();
    if (pending) consumeDockerDeployment(pending.requestId);
});

describe('Docker 部署导航请求', () => {
    it('路由尚未注册时保留目标，请求在跳转回调前已可读取', () => {
        openDockerDeployment('remote:first');
        expect(getPendingDockerDeployment()?.hostId).toBe('remote:first');

        const seen: string[] = [];
        disposers.push(
            registerComponentsNavigator(() => {
                seen.push(getPendingDockerDeployment()!.hostId);
            }),
        );
        openDockerDeployment('remote:second');
        expect(seen).toEqual(['remote:second']);
    });

    it('连续打开同一主机也有唯一请求，旧消费不会清除新请求', () => {
        openDockerDeployment('remote:one');
        const first = getPendingDockerDeployment()!;
        openDockerDeployment('remote:one');
        const second = getPendingDockerDeployment()!;

        expect(first.requestId).not.toBe(second.requestId);
        consumeDockerDeployment(first.requestId);
        expect(getPendingDockerDeployment()).toBe(second);
        consumeDockerDeployment(second.requestId);
        expect(getPendingDockerDeployment()).toBeNull();
    });

    it('订阅只通知真实变更，注销后不再接收请求和消费通知', () => {
        const seen: Array<string | null> = [];
        const off = subscribeDockerDeployment(() => {
            seen.push(getPendingDockerDeployment()?.hostId ?? null);
        });
        disposers.push(off);
        openDockerDeployment('remote:one');
        const first = getPendingDockerDeployment()!;
        consumeDockerDeployment(first.requestId + 1);
        expect(seen).toEqual(['remote:one']);
        consumeDockerDeployment(first.requestId);
        off();
        openDockerDeployment('remote:two');
        expect(seen).toEqual(['remote:one', null]);
    });

    it('StrictMode 的旧注销不会撤掉较新的 navigator', () => {
        const oldNavigator = vi.fn();
        const newNavigator = vi.fn();
        const offOld = registerComponentsNavigator(oldNavigator);
        const offNew = registerComponentsNavigator(newNavigator);
        disposers.push(offOld, offNew);

        offOld();
        openDockerDeployment('remote:one');
        expect(oldNavigator).not.toHaveBeenCalled();
        expect(newNavigator).toHaveBeenCalledOnce();
        offNew();
        openDockerDeployment('remote:two');
        expect(newNavigator).toHaveBeenCalledOnce();
    });

    it('已挂载目标页能订阅请求，消费一次后恢复空闲', () => {
        const hook = renderHook(() => usePendingDockerDeployment());
        expect(hook.result.current).toBeNull();
        act(() => openDockerDeployment('remote:target'));
        const request = hook.result.current!;
        expect(request.hostId).toBe('remote:target');
        act(() => consumeDockerDeployment(request.requestId));
        expect(hook.result.current).toBeNull();
    });
});
