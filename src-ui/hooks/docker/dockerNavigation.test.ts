// @vitest-environment jsdom
import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
    openDockerManagement,
    registerDockerNavigator,
    setDockerSelectedHost,
    useDockerHostSelection,
} from './dockerNavigation';

const disposers: Array<() => void> = [];

afterEach(() => {
    cleanup();
    for (const dispose of disposers.splice(0)) dispose();
    setDockerSelectedHost('');
});

describe('Docker 管理页主机导航', () => {
    it('选中主机跨页面挂卸载保留，返回部署主机时会更新选择', () => {
        const first = renderHook(() => useDockerHostSelection());
        act(() => first.result.current.setSelectedHost('remote:one'));
        first.unmount();

        const second = renderHook(() => useDockerHostSelection());
        expect(second.result.current.selectedHost).toBe('remote:one');
        act(() => openDockerManagement('remote:two'));
        expect(second.result.current.selectedHost).toBe('remote:two');
        second.unmount();

        const third = renderHook(() => useDockerHostSelection());
        expect(third.result.current.selectedHost).toBe('remote:two');
    });

    it('无 navigator 时也保留目标，目标页随后挂载仍能取到', () => {
        openDockerManagement('remote:before-mount');
        const hook = renderHook(() => useDockerHostSelection());
        expect(hook.result.current.selectedHost).toBe('remote:before-mount');
    });

    it('多个订阅者同步主机，setter 和相同主机快照保持稳定', () => {
        const first = renderHook(() => useDockerHostSelection());
        const second = renderHook(() => useDockerHostSelection());
        const setSelectedHost = first.result.current.setSelectedHost;
        act(() => setSelectedHost('remote:one'));
        expect(second.result.current.selectedHost).toBe('remote:one');
        expect(first.result.current.setSelectedHost).toBe(setSelectedHost);
        const sameHostSnapshot = first.result.current;
        act(() => setSelectedHost('remote:one'));
        expect(first.result.current).toBe(sameHostSnapshot);

        first.unmount();
        act(() => second.result.current.setSelectedHost('remote:two'));
        expect(second.result.current.selectedHost).toBe('remote:two');
    });

    it('注销旧 navigator 不会撤掉新注册，最新注销后不再跳转', () => {
        const oldNavigator = vi.fn();
        const newNavigator = vi.fn();
        const offOld = registerDockerNavigator(oldNavigator);
        const offNew = registerDockerNavigator(newNavigator);
        disposers.push(offOld, offNew);
        offOld();
        openDockerManagement('remote:one');
        expect(oldNavigator).not.toHaveBeenCalled();
        expect(newNavigator).toHaveBeenCalledOnce();
        offNew();
        openDockerManagement('remote:two');
        expect(newNavigator).toHaveBeenCalledOnce();
        const hook = renderHook(() => useDockerHostSelection());
        expect(hook.result.current.selectedHost).toBe('remote:two');
    });

    it('重复进入当前主机仍会导航，不要求先更改选择', () => {
        const navigate = vi.fn();
        disposers.push(registerDockerNavigator(navigate));
        openDockerManagement('remote:same');
        openDockerManagement('remote:same');
        expect(navigate).toHaveBeenCalledTimes(2);
    });
});
