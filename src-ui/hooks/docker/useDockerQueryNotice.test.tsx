// @vitest-environment jsdom
import { cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useDockerQueryNotice } from './useDockerQueryNotice';
import { pushErrorBar } from '../ui/pushErrorBar';
import { dismissInfoBar } from '../ui/globalInfoBarStore';

vi.mock('../ui/pushErrorBar', () => ({ pushErrorBar: vi.fn() }));
vi.mock('../ui/globalInfoBarStore', () => ({ dismissInfoBar: vi.fn() }));
beforeEach(() => vi.clearAllMocks());
afterEach(cleanup);

it('相同轮询错误不重复弹出，恢复和离开目标后清理提示', () => {
    const { rerender, unmount } = renderHook(
        ({ target, error }: { target: string; error: Error | null }) =>
            useDockerQueryNotice(target, '读取失败', error),
        { initialProps: { target: 'local::docker', error: new Error('offline') } },
    );
    rerender({ target: 'local::docker', error: new Error('offline') });
    expect(pushErrorBar).toHaveBeenCalledTimes(1);
    rerender({ target: 'local::docker', error: null });
    expect(dismissInfoBar).toHaveBeenCalledWith('key:local::docker');
    rerender({ target: 'local::podman', error: new Error('denied') });
    expect(pushErrorBar).toHaveBeenLastCalledWith({
        key: 'local::podman',
        title: '读取失败',
        raw: 'denied',
    });
    unmount();
    expect(dismissInfoBar).toHaveBeenLastCalledWith('key:local::podman');
});
