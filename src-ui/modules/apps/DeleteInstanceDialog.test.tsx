// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { AppInstance } from '../../core/ipc/types';
import { DeleteInstanceDialog } from './DeleteInstanceDialog';
import { isAppHostRemoved } from './hostLabel';

const instance: AppInstance = {
    id: 'xiuxian',
    framework_id: 'nonebot2',
    display_name: 'bot-xiuxian',
    host_id: 'remote:removed-server',
    placement: 'remote_native',
    install_dir: '/home/bot-xiuxian',
    port: 13120,
    state: 'stopped',
    origin: 'imported',
    auto_start: false,
    install_renderer: false,
    created_at_ms: 1,
};

afterEach(cleanup);

describe('移除失去主机的应用实例', () => {
    it('已移除的主机只提供记录移除，不显示远端删目录选项', () => {
        const onConfirm = vi.fn().mockResolvedValue(undefined);
        render(
            <DeleteInstanceDialog
                instance={instance}
                hostRemoved
                isRemoving={false}
                onClose={vi.fn()}
                onConfirm={onConfirm}
            />,
        );
        expect(screen.getByRole('heading', { name: '移除实例记录？' })).toBeTruthy();
        expect(screen.queryByRole('checkbox')).toBeNull();
        fireEvent.click(screen.getByRole('button', { name: '移除记录' }));
        expect(onConfirm).toHaveBeenCalledWith(false);
    });

    it('主机在弹窗打开期间被移除时，忽略之前勾选的删除目录选项', () => {
        const onConfirm = vi.fn().mockResolvedValue(undefined);
        const props = { instance, isRemoving: false, onClose: vi.fn(), onConfirm };
        const view = render(<DeleteInstanceDialog {...props} />);
        expect(screen.getByRole('button', { name: '释放并还原' })).toBeTruthy();
        fireEvent.click(screen.getByRole('checkbox'));
        view.rerender(<DeleteInstanceDialog {...props} hostRemoved />);
        fireEvent.click(screen.getByRole('button', { name: '移除记录' }));
        expect(onConfirm).toHaveBeenCalledWith(false);
    });

    it('服务器列表未加载或加载失败时不判断主机已移除', () => {
        expect(isAppHostRemoved(instance.host_id, [], false)).toBe(false);
        expect(isAppHostRemoved(instance.host_id, [], true)).toBe(true);
        expect(isAppHostRemoved(instance.host_id, [{ id: 'removed-server' }], true)).toBe(false);
        expect(isAppHostRemoved('local', [], true)).toBe(false);
    });
});
