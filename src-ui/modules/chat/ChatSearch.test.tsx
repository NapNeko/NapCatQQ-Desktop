import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ChatSearch } from './ChatSearch';
import { emptyAccount, ingestMessage } from '../../core/domain/chat/model';

function messages() {
    let account = emptyAccount('99');
    for (let id = 1; id <= 3; id++) account = ingestMessage(account, { message_type: 'group', group_id: 12, user_id: 88, message_id: id, time: id, sender: { nickname: '小明' }, message: `Hello ${id}` });
    return account.messages.map((m, i) => ({ ...m, recalled: i === 0 }));
}
describe('native message search', () => {
    it('finds loaded messages case-insensitively and excludes recalled content', () => {
        const items = messages(); const reveal = vi.fn();
        render(<ChatSearch messages={items} onReveal={reveal} onClose={() => {}} />);
        const input = screen.getByRole('textbox', { name: '搜索已加载消息' });
        expect(input).toHaveFocus();
        fireEvent.change(input, { target: { value: 'hello' } });
        expect(screen.queryByText('Hello 1')).not.toBeInTheDocument();
        expect(screen.getByRole('status')).toHaveTextContent('2 条结果');
        fireEvent.click(screen.getByRole('button', { name: /Hello 3/ }));
        expect(reveal).toHaveBeenCalledWith(items[2].key);
    });
    it('reports an empty result and closes with Escape', () => {
        const close = vi.fn();
        render(<ChatSearch messages={messages()} onReveal={() => {}} onClose={close} />);
        const input = screen.getByRole('textbox');
        fireEvent.change(input, { target: { value: '不存在' } });
        expect(screen.getByRole('status')).toHaveTextContent('没有找到匹配消息');
        fireEvent.keyDown(input, { key: 'Escape' });
        expect(close).toHaveBeenCalledOnce();
    });
    it('separates clearing the query from closing the search by pointer', () => {
        const close = vi.fn();
        render(<ChatSearch messages={messages()} onReveal={() => {}} onClose={close} />);
        const input = screen.getByRole('textbox');
        fireEvent.change(input, { target: { value: 'hello' } });
        fireEvent.click(screen.getByRole('button', { name: '清除消息搜索' }));
        expect(input).toHaveValue('');
        expect(input).toHaveFocus();
        expect(close).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '关闭消息搜索' }));
        expect(close).toHaveBeenCalledOnce();
    });
});
