// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { PanelTable } from './PanelTable';

afterEach(cleanup);

it('行选择提供按钮入口，行内操作不会同时触发行选择', () => {
    const select = vi.fn();
    const remove = vi.fn();
    render(
        <PanelTable
            rows={[{ id: 'one', name: '预览模型' }]}
            onRowClick={select}
            selectedKey="one"
            columns={[
                { key: 'name', label: '模型' },
                {
                    key: 'actions',
                    label: '操作',
                    render: () => <button onClick={remove}>删除模型</button>,
                },
            ]}
        />,
    );
    const entry = screen.getByRole('button', { name: '预览模型' });
    expect(entry).toHaveAttribute('aria-pressed', 'true');
    fireEvent.click(entry);
    expect(select).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole('button', { name: '删除模型' }));
    expect(remove).toHaveBeenCalledOnce();
    expect(select).toHaveBeenCalledTimes(1);
});
