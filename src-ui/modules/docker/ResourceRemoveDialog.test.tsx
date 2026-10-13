import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { ResourceRemoveDialog } from './ResourceRemoveDialog';

afterEach(cleanup);

describe('Docker 删除保护', () => {
    it('数据卷必须输入完整名称，失败保持弹窗并显示原因', async () => {
        const confirm = vi.fn().mockRejectedValue(new Error('volume is in use'));
        const close = vi.fn();
        render(
            <ResourceRemoveDialog
                target={{ kind: 'volume', id: 'data', name: 'data' }}
                busy={false}
                onConfirm={confirm}
                onClose={close}
            />,
        );
        const button = screen.getByRole('button', { name: '删除数据卷' });
        expect(button).toBeDisabled();
        fireEvent.change(screen.getByLabelText('输入数据卷名称确认'), { target: { value: 'dat' } });
        expect(button).toBeDisabled();
        fireEvent.change(screen.getByLabelText('输入数据卷名称确认'), {
            target: { value: 'data' },
        });
        fireEvent.click(button);
        await waitFor(() =>
            expect(screen.getByRole('alert')).toHaveTextContent('volume is in use'),
        );
        expect(confirm).toHaveBeenCalledWith(false);
        expect(close).not.toHaveBeenCalled();
    });
    it('容器删除默认不强制，明确勾选后才传 force', async () => {
        const confirm = vi.fn().mockResolvedValue(undefined);
        const close = vi.fn();
        render(
            <ResourceRemoveDialog
                target={{ kind: 'container', id: 'abc', name: 'demo' }}
                busy={false}
                onConfirm={confirm}
                onClose={close}
            />,
        );
        expect(screen.getByRole('checkbox')).not.toBeChecked();
        fireEvent.click(screen.getByRole('checkbox'));
        fireEvent.click(screen.getByRole('button', { name: '删除容器' }));
        await waitFor(() => expect(confirm).toHaveBeenCalledWith(true));
        expect(close).toHaveBeenCalledTimes(1);
    });
    it('连续点击不会重复提交删除', async () => {
        const confirm = vi.fn(() => new Promise<void>(() => {}));
        render(
            <ResourceRemoveDialog
                target={{ kind: 'network', id: 'net', name: 'net' }}
                busy={false}
                onConfirm={confirm}
                onClose={vi.fn()}
            />,
        );
        const button = screen.getByRole('button', { name: '删除网络' });
        fireEvent.click(button);
        fireEvent.click(button);
        expect(confirm).toHaveBeenCalledTimes(1);
    });
});
