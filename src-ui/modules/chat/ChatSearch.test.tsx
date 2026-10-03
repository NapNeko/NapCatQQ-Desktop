import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ChatSearch } from './ChatSearch';
import { emptyAccount, ingestMessage } from '../../core/domain/chat/model';

function messages(count = 3, text = (id: number) => `Hello ${id}`) {
    let account = emptyAccount('99');
    for (let id = 1; id <= count; id++) account = ingestMessage(account, { message_type: 'group', group_id: 12, user_id: 88, message_id: id, time: id, sender: { nickname: '小明' }, message: text(id) });
    return account.messages.map((m, i) => ({ ...m, recalled: i === 0 }));
}
describe('native message search', () => {
    it('browses loaded records with date groups before entering a keyword', () => {
        render(<ChatSearch messages={messages()} onReveal={vi.fn()} onClose={vi.fn()} />);
        expect(screen.getAllByRole('option')).toHaveLength(2);
        expect(screen.getByRole('status')).toHaveTextContent('2 条记录');
        expect(screen.getByText('1970/01/01')).toBeInTheDocument();
    });
    it('separates photos and videos, stickers, files and links', () => {
        const segments = [
            { type: 'text', data: { text: '普通消息' } },
            { type: 'image', data: { url: 'https://cdn.example/photo.png' } },
            { type: 'video', data: { file: 'video' } },
            { type: 'image', data: { url: 'https://cdn.example/sticker.gif', sub_type: 1 } },
            { type: 'face', data: { id: 14 } },
            { type: 'file', data: { name: '资料.pdf' } },
            { type: 'text', data: { text: 'https://example.com' } },
        ];
        const items = messages(7).map((message, index) => ({ ...message, recalled: false, segments: [segments[index]] }));
        render(<ChatSearch messages={items} onReveal={vi.fn()} onClose={vi.fn()} />);
        fireEvent.click(screen.getByRole('tab', { name: '图片/视频' }));
        expect(screen.getAllByRole('option')).toHaveLength(2);
        expect(screen.getByAltText('图片预览')).toHaveAttribute('src', 'https://cdn.example/photo.png');
        fireEvent.click(screen.getByRole('tab', { name: '表情' }));
        expect(screen.getAllByRole('option')).toHaveLength(2);
        expect(screen.getByAltText('图片预览')).toHaveAttribute('src', 'https://cdn.example/sticker.gif');
        fireEvent.click(screen.getByRole('tab', { name: '文件' }));
        expect(screen.getAllByRole('option')).toHaveLength(1);
        expect(screen.getByRole('option')).toHaveTextContent('资料.pdf');
        fireEvent.click(screen.getByRole('tab', { name: '链接' }));
        expect(screen.getAllByRole('option')).toHaveLength(1);
        expect(screen.getByRole('option')).toHaveTextContent('https://example.com');
    });
    it('filters by sender and local date and restores records after clearing filters', () => {
        const items = messages().map((message, index) => ({ ...message, senderId: String(index), senderName: `成员${index}`, at: new Date(`2026-10-0${index + 1}T12:00:00`).getTime() }));
        render(<ChatSearch messages={items} onReveal={vi.fn()} onClose={vi.fn()} />);
        fireEvent.click(screen.getByRole('button', { name: '筛选' }));
        fireEvent.change(screen.getByRole('combobox', { name: '筛选发送者' }), { target: { value: '1' } });
        expect(screen.getAllByRole('option').filter(option => option.getAttribute('id'))).toHaveLength(1);
        fireEvent.change(screen.getByLabelText('起始日期'), { target: { value: '2026-10-03' } });
        expect(screen.queryAllByRole('option').filter(option => option.getAttribute('id'))).toHaveLength(0);
        fireEvent.click(screen.getByRole('button', { name: '清除筛选' }));
        expect(screen.getAllByRole('option').filter(option => option.getAttribute('id'))).toHaveLength(2);
    });
    it('finds loaded messages case-insensitively and excludes recalled content', () => {
        const items = messages(); const reveal = vi.fn();
        render(<ChatSearch messages={items} onReveal={reveal} onClose={() => {}} />);
        const input = screen.getByRole('combobox', { name: '搜索已加载消息' });
        expect(input).toHaveFocus();
        fireEvent.change(input, { target: { value: 'hello' } });
        expect(screen.queryByRole('option', { name: /Hello 1/ })).not.toBeInTheDocument();
        expect(screen.getByRole('status')).toHaveTextContent('2 条结果');
        expect(screen.getByText(/仅搜索当前会话已加载的 2 条消息/)).toBeInTheDocument();
        fireEvent.click(screen.getByRole('option', { name: /Hello 3/ }));
        expect(reveal).toHaveBeenCalledWith(items[2].key);
    });
    it('reports an empty result and closes with Escape', () => {
        const close = vi.fn();
        render(<ChatSearch messages={messages()} onReveal={() => {}} onClose={close} />);
        const input = screen.getByRole('combobox');
        fireEvent.change(input, { target: { value: '不存在' } });
        expect(screen.getByRole('status')).toHaveTextContent('没有找到匹配消息');
        fireEvent.keyDown(input, { key: 'Escape' });
        expect(close).toHaveBeenCalledOnce();
    });
    it('separates clearing the query from closing the search by pointer', () => {
        const close = vi.fn();
        render(<ChatSearch messages={messages()} onReveal={() => {}} onClose={close} />);
        const input = screen.getByRole('combobox');
        fireEvent.change(input, { target: { value: 'hello' } });
        fireEvent.click(screen.getByRole('button', { name: '清除消息搜索' }));
        expect(input).toHaveValue('');
        expect(input).toHaveFocus();
        expect(close).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '关闭消息搜索' }));
        expect(close).toHaveBeenCalledOnce();
    });

    it('selects results with arrow keys and reveals the active message with Enter', () => {
        const items = messages(); const reveal = vi.fn();
        render(<ChatSearch messages={items} onReveal={reveal} onClose={() => {}} />);
        const input = screen.getByRole('combobox');
        fireEvent.change(input, { target: { value: 'hello' } });
        const newest = screen.getByRole('option', { name: /Hello 3/ });
        const older = screen.getByRole('option', { name: /Hello 2/ });
        expect(newest).toHaveAttribute('aria-selected', 'true');
        expect(input).toHaveAttribute('aria-activedescendant', newest.id);
        fireEvent.keyDown(input, { key: 'ArrowDown' });
        expect(input).toHaveFocus();
        expect(input).toHaveAttribute('aria-activedescendant', older.id);
        fireEvent.keyDown(input, { key: 'ArrowDown' });
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(reveal).toHaveBeenLastCalledWith(items[1].key);
        fireEvent.keyDown(input, { key: 'ArrowUp' });
        fireEvent.keyDown(input, { key: 'ArrowUp' });
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(reveal).toHaveBeenLastCalledWith(items[2].key);
        expect(newest).toHaveAttribute('aria-selected', 'true');
    });

    it('does not navigate, reveal or close while an IME is composing', () => {
        const reveal = vi.fn(); const close = vi.fn();
        render(<ChatSearch messages={messages()} onReveal={reveal} onClose={close} />);
        const input = screen.getByRole('combobox');
        fireEvent.change(input, { target: { value: 'hello' } });
        const selected = input.getAttribute('aria-activedescendant');
        fireEvent.compositionStart(input);
        fireEvent.keyDown(input, { key: 'ArrowDown' });
        fireEvent.keyDown(input, { key: 'Enter' });
        fireEvent.keyDown(input, { key: 'Escape' });
        fireEvent.compositionEnd(input);
        fireEvent.keyDown(input, { key: 'Enter', isComposing: true });
        fireEvent.keyDown(input, { key: 'Escape', keyCode: 229 });
        expect(input).toHaveAttribute('aria-activedescendant', selected);
        expect(reveal).not.toHaveBeenCalled();
        expect(close).not.toHaveBeenCalled();
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(reveal).toHaveBeenCalledOnce();
    });

    it('highlights case-insensitive literal text without interpreting markup or regular expressions', () => {
        const { container } = render(<ChatSearch messages={messages(3, () => 'A.*[B] <img src=x onerror=alert(1)> a.*[b]')} onReveal={() => {}} onClose={() => {}} />);
        const input = screen.getByRole('combobox');
        fireEvent.change(input, { target: { value: 'a.*[b]' } });
        expect(screen.getAllByRole('option')).toHaveLength(2);
        const marks = container.querySelectorAll('mark');
        expect(Array.from(marks, node => node.textContent)).toEqual(['A.*[B]', 'a.*[b]', 'A.*[B]', 'a.*[b]']);
        expect(container.querySelector('.native-chat-search-result-text img')).toBeNull();
        fireEvent.change(input, { target: { value: '<img src=x onerror=alert(1)>' } });
        expect(container.querySelectorAll('mark')).toHaveLength(2);
        expect(container.querySelector('.native-chat-search-result-text img')).toBeNull();
        fireEvent.change(input, { target: { value: 'a.+[b]' } });
        expect(screen.queryAllByRole('option')).toHaveLength(0);
    });

    it('preserves the query and selected message when messages update and safely falls back when it disappears', () => {
        const items = messages(); const reveal = vi.fn();
        const props = { onReveal: reveal, onClose: () => {} };
        const { rerender } = render(<ChatSearch messages={items} {...props} />);
        const input = screen.getByRole('combobox');
        fireEvent.change(input, { target: { value: 'hello' } });
        fireEvent.keyDown(input, { key: 'ArrowDown' });
        rerender(<ChatSearch messages={messages(4)} {...props} />);
        expect(input).toHaveValue('hello');
        expect(screen.getByRole('option', { name: /Hello 2/ })).toHaveAttribute('aria-selected', 'true');
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(reveal).toHaveBeenLastCalledWith(items[1].key);
        rerender(<ChatSearch messages={[items[2]]} {...props} />);
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(reveal).toHaveBeenLastCalledWith(items[2].key);
        rerender(<ChatSearch messages={[]} {...props} />);
        expect(input).not.toHaveAttribute('aria-activedescendant');
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(reveal).toHaveBeenCalledTimes(2);
    });

    it('lets users expand loaded results beyond the first 50', () => {
        const reveal = vi.fn(); const items = messages(61);
        render(<ChatSearch messages={items} onReveal={reveal} onClose={() => {}} />);
        fireEvent.change(screen.getByRole('combobox'), { target: { value: 'hello' } });
        expect(screen.getAllByRole('option')).toHaveLength(50);
        expect(screen.getByRole('status')).toHaveTextContent('60 条结果 · 显示最近 50 条');
        fireEvent.click(screen.getByRole('button', { name: '显示更多结果（还有 10 条）' }));
        expect(screen.getAllByRole('option')).toHaveLength(60);
        expect(screen.queryByRole('button', { name: /显示更多结果/ })).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole('option', { name: /Hello 2$/ }));
        expect(reveal).toHaveBeenCalledWith(items[1].key);
    });

    it('keeps keyboard navigation usable when crossing the first result page', () => {
        const reveal = vi.fn(); const items = messages(61);
        render(<ChatSearch messages={items} onReveal={reveal} onClose={() => {}} />);
        const input = screen.getByRole('combobox');
        fireEvent.change(input, { target: { value: 'hello' } });
        for (let index = 0; index < 50; index++) fireEvent.keyDown(input, { key: 'ArrowDown' });
        const selected = screen.getByRole('option', { selected: true });
        expect(selected).toHaveTextContent('Hello 11');
        expect(input).toHaveAttribute('aria-activedescendant', selected.id);
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(reveal).toHaveBeenCalledWith(items[10].key);
    });

    it('loads one history page only on request and retains the search when older messages arrive', () => {
        const load = vi.fn(); const close = vi.fn();
        const props = { onReveal: vi.fn(), onClose: close, onLoadEarlier: load };
        const items = messages();
        const { rerender } = render(<ChatSearch messages={[items[2]]} history={{ loading: false, loaded: true, done: false, error: '' }} {...props} />);
        const input = screen.getByRole('combobox');
        fireEvent.change(input, { target: { value: 'hello' } });
        expect(load).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '加载更早消息' }));
        expect(load).toHaveBeenCalledOnce();
        expect(close).not.toHaveBeenCalled();
        rerender(<ChatSearch messages={[items[2]]} history={{ loading: true, loaded: true, done: false, error: '' }} {...props} />);
        expect(screen.getByRole('button', { name: '正在加载…' })).toBeDisabled();
        expect(screen.getByRole('status')).toHaveTextContent('正在加载更早消息');
        fireEvent.click(screen.getByRole('button', { name: '正在加载…' }));
        expect(load).toHaveBeenCalledOnce();
        rerender(<ChatSearch messages={items} history={{ loading: false, loaded: true, done: true, error: '' }} {...props} />);
        expect(input).toHaveValue('hello');
        expect(screen.getByRole('status')).toHaveTextContent('2 条结果 · 已到最早消息');
        expect(screen.getByRole('option', { selected: true })).toHaveTextContent('Hello 3');
        expect(screen.queryByRole('button', { name: '加载更早消息' })).not.toBeInTheDocument();
        expect(load).toHaveBeenCalledOnce();
    });

    it('shows history errors with a retry action and disables loading while disconnected', () => {
        const load = vi.fn();
        const props = { messages: messages(), onReveal: vi.fn(), onClose: vi.fn(), onLoadEarlier: load, history: { loading: false, loaded: true, done: false, error: '请求超时' } };
        const { rerender } = render(<ChatSearch {...props} />);
        expect(screen.getByRole('alert')).toHaveTextContent('读取历史失败：请求超时');
        fireEvent.click(screen.getByRole('button', { name: '重试加载更早消息' }));
        expect(load).toHaveBeenCalledOnce();
        rerender(<ChatSearch {...props} canLoadEarlier={false} />);
        expect(screen.getByRole('button', { name: '重试加载更早消息' })).toBeDisabled();
        expect(screen.getByRole('status')).toHaveTextContent('连接恢复后可加载更早消息');
        fireEvent.click(screen.getByRole('button', { name: '重试加载更早消息' }));
        expect(load).toHaveBeenCalledOnce();
    });

    it('distinguishes initial history loading and offers useful empty scope guidance', () => {
        const props = { messages: [], onReveal: vi.fn(), onClose: vi.fn(), onLoadEarlier: vi.fn() };
        const { rerender } = render(<ChatSearch {...props} history={{ loading: true, loaded: false, done: false, error: '' }} />);
        expect(screen.getByRole('status')).toHaveTextContent('正在读取消息');
        expect(screen.getByText(/已加载的 0 条消息/)).toBeInTheDocument();
        fireEvent.change(screen.getByRole('combobox'), { target: { value: 'hello' } });
        expect(within(screen.getByRole('listbox')).queryAllByRole('option')).toHaveLength(0);
        expect(screen.getByRole('status')).toHaveTextContent('可换个关键词或加载更早消息');
        rerender(<ChatSearch {...props} history={{ loading: false, loaded: true, done: true, error: '' }} />);
        expect(screen.getByRole('status')).toHaveTextContent('试试其他关键词');
        expect(screen.getByRole('status')).not.toHaveTextContent('或加载更早消息');
    });
});
