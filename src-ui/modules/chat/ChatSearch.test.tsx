import { fireEvent, render, screen, within, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ChatSearch } from './ChatSearch';
import { emptyAccount, ingestMessages, openConversation } from '../../core/domain/chat/model';
import { chatMediaService } from '../../core/services/chat-media.service';
import { qqFaceAssetService } from '../../core/services/qq-face-assets.service';

function messages(count = 3, text = (id: number) => `Hello ${id}`) {
    const account = ingestMessages(
        openConversation(emptyAccount('99'), {
            key: 'group:12',
            type: 'group',
            id: '12',
            name: '群',
        }),
        Array.from({ length: count }, (_, index) => ({
            message_type: 'group',
            group_id: 12,
            user_id: 88,
            message_id: index + 1,
            time: index + 1,
            sender: { nickname: '小明' },
            message: text(index + 1),
        })),
        true,
    );
    return account.messages.map((m, i) => ({ ...m, recalled: i === 0 }));
}
function openSearchInput() {
    if (!screen.queryByRole('combobox', { name: /^搜索/ }))
        fireEvent.click(screen.getByRole('button', { name: '搜索消息' }));
    return screen.getByRole('combobox', { name: /^搜索/ });
}
describe('native message search', () => {
    it('resolves archived image IDs and refreshes failed URLs with the timeline media reader', async () => {
        const target = {
            bot_id: 'archive-media',
            qq_id: 99,
            name: '测试',
            backend: 'napcat' as const,
            host: { kind: 'remote' as const, server_id: 'remote' },
            running: true,
            online: true,
        };
        const read = vi
            .spyOn(chatMediaService, 'image')
            .mockResolvedValueOnce('data:image/png;base64,iVBORw0KGgo=')
            .mockResolvedValueOnce('https://cdn.example/refreshed-archive.png');
        const item = {
            ...messages()[1],
            segments: [{ type: 'image', data: { file_id: 'old-archive-image' } }],
        };
        const reveal = vi.fn();
        render(
            <ChatSearch
                target={target}
                messages={[]}
                archivedMessages={[item]}
                initialScope="account"
                onReveal={reveal}
                onClose={vi.fn()}
            />,
        );
        const image = await screen.findByAltText('图片');
        expect(image).toHaveAttribute('src', 'data:image/png;base64,iVBORw0KGgo=');
        expect(read).toHaveBeenCalledWith(target, item.segments[0].data, undefined, {
            signal: expect.any(AbortSignal),
            context: { messageId: item.id, imageIndex: 0 },
        });
        fireEvent.error(image);
        await waitFor(() =>
            expect(screen.getByAltText('图片')).toHaveAttribute(
                'src',
                'https://cdn.example/refreshed-archive.png',
            ),
        );
        expect(read).toHaveBeenCalledWith(target, item.segments[0].data, true, {
            signal: expect.any(AbortSignal),
            context: { messageId: item.id, imageIndex: 0 },
        });
        fireEvent.click(screen.getByRole('button', { name: '看大图' }));
        expect(await screen.findByAltText('消息图片')).toHaveAttribute(
            'src',
            'https://cdn.example/refreshed-archive.png',
        );
        expect(reveal).not.toHaveBeenCalled();
    });
    it('expands search in the shared toolbar and renders nested markdown safely', () => {
        const item = {
            ...messages()[1],
            segments: [
                {
                    type: 'markdown',
                    data: {
                        data: {
                            content:
                                '# 工作日志\n\n**正文关键词**\n\n[查看](javascript:alert(1)) <img src=x onerror=alert(1)>',
                        },
                    },
                },
            ],
        };
        const { container } = render(
            <ChatSearch
                messages={[item]}
                archivedMessages={[item]}
                currentSession="group:12"
                onReveal={vi.fn()}
                onClose={vi.fn()}
            />,
        );
        const toggle = screen.getByRole('button', { name: '搜索消息' });
        const toolbar = toggle.closest('.native-chat-search-toolbar')!;
        expect(screen.queryByRole('combobox')).toBeNull();
        expect(within(toolbar).getByRole('tab', { name: '图片/视频' })).toBeInTheDocument();
        expect(within(toolbar).getByRole('tab', { name: '当前会话' })).toBeInTheDocument();
        const input = openSearchInput();
        expect(input).toHaveFocus();
        fireEvent.change(input, { target: { value: '正文关键词' } });
        expect(screen.getByRole('option')).toHaveTextContent('正文关键词');
        expect(container.querySelector('.native-chat-search-markdown strong')).toHaveTextContent(
            '正文关键词',
        );
        expect(
            container.querySelector(
                '.native-chat-search-markdown img, .native-chat-search-markdown a',
            ),
        ).toBeNull();
        fireEvent.click(toggle);
        expect(toggle).toHaveAttribute('aria-expanded', 'false');
        expect(toggle).toHaveFocus();
        expect(screen.queryByRole('combobox')).toBeNull();
        expect(screen.getByRole('status')).toHaveTextContent('1 条记录');
    });
    it('renders QQ faces inline and omits duplicate image labels unless the preview fails', () => {
        const item = {
            ...messages()[1],
            segments: [
                { type: 'text', data: { text: '你好' } },
                { type: 'face', data: { id: 14 } },
                { type: 'image', data: { url: 'https://cdn.example/sticker.gif', sub_type: 1 } },
            ],
        };
        render(<ChatSearch messages={[item]} onReveal={vi.fn()} onClose={vi.fn()} />);
        const option = screen.getByRole('option');
        expect(within(option).getByAltText('QQ 表情 14')).toBeInTheDocument();
        expect(option).toHaveTextContent('你好');
        expect(option).not.toHaveTextContent('[图片]');
        fireEvent.error(within(option).getByAltText('图片'));
        expect(option).toHaveTextContent('[图片加载失败]');
    });
    it('uses the same display-only markerless face projection in archived search results', async () => {
        vi.spyOn(qqFaceAssetService, 'acquire').mockImplementation(async (url) => ({
            url,
            release: vi.fn(),
        }));
        const segments = [
            { type: 'face', data: { id: '498' } },
            { type: 'text', data: { text: '[中!]' } },
            { type: 'face', data: { id: '494' } },
            { type: 'text', data: { text: '[举杯邀月]' } },
            { type: 'face', data: { id: '495' } },
            { type: 'text', data: { text: '[兔来]' } },
        ];
        const snapshot = JSON.stringify(segments);
        const item = { ...messages()[1], segments };
        render(
            <ChatSearch
                messages={[]}
                archivedMessages={[item]}
                initialScope="account"
                onReveal={vi.fn()}
                onClose={vi.fn()}
            />,
        );
        const option = screen.getByRole('option');
        for (const id of ['498', '494', '495'])
            expect(within(option).getByAltText('QQ 表情 ' + id)).toHaveAttribute('width', '72');
        for (const caption of ['[中!]', '[举杯邀月]', '[兔来]'])
            expect(option).not.toHaveTextContent(caption);
        await waitFor(() =>
            expect(within(option).getByAltText('QQ 表情 498').getAttribute('src')).toContain(
                '/apng/',
            ),
        );
        expect(JSON.stringify(segments)).toBe(snapshot);
    });
    it('includes preserved recalled content only when recall protection is enabled', () => {
        const item = { ...messages()[1], recalled: true };
        const props = {
            messages: [
                {
                    ...item,
                    recalled: false,
                    segments: [{ type: 'text', data: { text: '撤回占位' } }],
                },
            ],
            archivedMessages: [item],
            onReveal: vi.fn(),
            onClose: vi.fn(),
        };
        const { rerender } = render(<ChatSearch {...props} preventRecall />);
        expect(screen.getByRole('option')).toHaveAttribute('data-recalled', 'true');
        expect(screen.getByRole('option')).toHaveTextContent('已撤回');
        expect(screen.getByRole('option')).toHaveTextContent('Hello 2');
        expect(screen.getByRole('option')).not.toHaveTextContent('撤回占位');
        rerender(<ChatSearch {...props} preventRecall={false} />);
        expect(screen.queryByRole('option')).not.toBeInTheDocument();
    });
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
        const items = messages(7).map((message, index) => ({
            ...message,
            recalled: false,
            segments: [segments[index]],
        }));
        render(<ChatSearch messages={items} onReveal={vi.fn()} onClose={vi.fn()} />);
        fireEvent.click(screen.getByRole('tab', { name: '图片/视频' }));
        expect(screen.getAllByRole('option')).toHaveLength(2);
        expect(screen.getByAltText('图片')).toHaveAttribute('src', 'https://cdn.example/photo.png');
        fireEvent.click(screen.getByRole('tab', { name: '表情' }));
        expect(screen.getAllByRole('option')).toHaveLength(2);
        expect(screen.getByAltText('图片')).toHaveAttribute(
            'src',
            'https://cdn.example/sticker.gif',
        );
        fireEvent.click(screen.getByRole('tab', { name: '文件' }));
        expect(screen.getAllByRole('option')).toHaveLength(1);
        expect(screen.getByRole('option')).toHaveTextContent('资料.pdf');
        fireEvent.click(screen.getByRole('tab', { name: '链接' }));
        expect(screen.getAllByRole('option')).toHaveLength(1);
        expect(screen.getByRole('option')).toHaveTextContent('https://example.com');
    });
    it('filters by sender and local date and restores records after clearing filters', () => {
        const items = messages().map((message, index) => ({
            ...message,
            senderId: String(index),
            senderName: `成员${index}`,
            at: new Date(`2026-10-0${index + 1}T12:00:00`).getTime(),
        }));
        render(<ChatSearch messages={items} onReveal={vi.fn()} onClose={vi.fn()} />);
        fireEvent.click(screen.getByRole('button', { name: '筛选' }));
        fireEvent.change(screen.getByRole('combobox', { name: '筛选发送者' }), {
            target: { value: '1' },
        });
        expect(
            screen.getAllByRole('option').filter((option) => option.getAttribute('id')),
        ).toHaveLength(1);
        fireEvent.change(screen.getByLabelText('起始日期'), { target: { value: '2026-10-03' } });
        expect(
            screen.queryAllByRole('option').filter((option) => option.getAttribute('id')),
        ).toHaveLength(0);
        fireEvent.click(screen.getByRole('button', { name: '清除筛选' }));
        expect(
            screen.getAllByRole('option').filter((option) => option.getAttribute('id')),
        ).toHaveLength(2);
    });
    it('finds loaded messages case-insensitively and excludes recalled content', () => {
        const items = messages();
        const reveal = vi.fn();
        render(<ChatSearch messages={items} onReveal={reveal} onClose={() => {}} />);
        const input = openSearchInput();
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
        const input = openSearchInput();
        fireEvent.change(input, { target: { value: '不存在' } });
        expect(screen.getByRole('status')).toHaveTextContent('没有找到匹配消息');
        fireEvent.keyDown(input, { key: 'Escape' });
        expect(close).toHaveBeenCalledOnce();
    });
    it('separates clearing the query from closing the search by pointer', () => {
        const close = vi.fn();
        render(<ChatSearch messages={messages()} onReveal={() => {}} onClose={close} />);
        const input = openSearchInput();
        fireEvent.change(input, { target: { value: 'hello' } });
        fireEvent.click(screen.getByRole('button', { name: '清除消息搜索' }));
        expect(input).toHaveValue('');
        expect(input).toHaveFocus();
        expect(close).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '关闭消息搜索' }));
        expect(close).toHaveBeenCalledOnce();
    });

    it('selects results with arrow keys and reveals the active message with Enter', () => {
        const items = messages();
        const reveal = vi.fn();
        render(<ChatSearch messages={items} onReveal={reveal} onClose={() => {}} />);
        const input = openSearchInput();
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
        const reveal = vi.fn();
        const close = vi.fn();
        render(<ChatSearch messages={messages()} onReveal={reveal} onClose={close} />);
        const input = openSearchInput();
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
        const { container } = render(
            <ChatSearch
                messages={messages(3, () => 'A.*[B] <img src=x onerror=alert(1)> a.*[b]')}
                onReveal={() => {}}
                onClose={() => {}}
            />,
        );
        const input = openSearchInput();
        fireEvent.change(input, { target: { value: 'a.*[b]' } });
        expect(screen.getAllByRole('option')).toHaveLength(2);
        const marks = container.querySelectorAll('mark');
        expect(Array.from(marks, (node) => node.textContent)).toEqual([
            'A.*[B]',
            'a.*[b]',
            'A.*[B]',
            'a.*[b]',
        ]);
        expect(container.querySelector('.native-chat-search-result-text img')).toBeNull();
        fireEvent.change(input, { target: { value: '<img src=x onerror=alert(1)>' } });
        expect(container.querySelectorAll('mark')).toHaveLength(2);
        expect(container.querySelector('.native-chat-search-result-text img')).toBeNull();
        fireEvent.change(input, { target: { value: 'a.+[b]' } });
        expect(screen.queryAllByRole('option')).toHaveLength(0);
    });

    it('preserves the query and selected message when messages update and safely falls back when it disappears', () => {
        const items = messages();
        const reveal = vi.fn();
        const props = { onReveal: reveal, onClose: () => {} };
        const { rerender } = render(<ChatSearch messages={items} {...props} />);
        const input = openSearchInput();
        fireEvent.change(input, { target: { value: 'hello' } });
        fireEvent.keyDown(input, { key: 'ArrowDown' });
        rerender(<ChatSearch messages={messages(4)} {...props} />);
        expect(input).toHaveValue('hello');
        expect(screen.getByRole('option', { name: /Hello 2/ })).toHaveAttribute(
            'aria-selected',
            'true',
        );
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
        const reveal = vi.fn();
        const items = messages(61);
        render(<ChatSearch messages={items} onReveal={reveal} onClose={() => {}} />);
        fireEvent.change(openSearchInput(), { target: { value: 'hello' } });
        expect(screen.getAllByRole('option')).toHaveLength(50);
        expect(screen.getByRole('status')).toHaveTextContent('60 条结果 · 显示最近 50 条');
        fireEvent.click(screen.getByRole('button', { name: '显示更多结果（还有 10 条）' }));
        expect(screen.getAllByRole('option')).toHaveLength(60);
        expect(screen.queryByRole('button', { name: /显示更多结果/ })).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole('option', { name: /Hello 2$/ }));
        expect(reveal).toHaveBeenCalledWith(items[1].key);
    });

    it('keeps keyboard navigation usable when crossing the first result page', () => {
        const reveal = vi.fn();
        const items = messages(61);
        render(<ChatSearch messages={items} onReveal={reveal} onClose={() => {}} />);
        const input = openSearchInput();
        fireEvent.change(input, { target: { value: 'hello' } });
        for (let index = 0; index < 50; index++) fireEvent.keyDown(input, { key: 'ArrowDown' });
        const selected = screen.getByRole('option', { selected: true });
        expect(selected).toHaveTextContent('Hello 11');
        expect(input).toHaveAttribute('aria-activedescendant', selected.id);
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(reveal).toHaveBeenCalledWith(items[10].key);
    });

    it('loads one history page only on request and retains the search when older messages arrive', () => {
        const load = vi.fn();
        const close = vi.fn();
        const props = { onReveal: vi.fn(), onClose: close, onLoadEarlier: load };
        const items = messages();
        const { rerender } = render(
            <ChatSearch
                messages={[items[2]]}
                history={{ loading: false, loaded: true, done: false, error: '' }}
                {...props}
            />,
        );
        const input = openSearchInput();
        fireEvent.change(input, { target: { value: 'hello' } });
        expect(load).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '加载更早消息' }));
        expect(load).toHaveBeenCalledOnce();
        expect(close).not.toHaveBeenCalled();
        rerender(
            <ChatSearch
                messages={[items[2]]}
                history={{ loading: true, loaded: true, done: false, error: '' }}
                {...props}
            />,
        );
        expect(screen.getByRole('button', { name: '正在加载…' })).toBeDisabled();
        expect(screen.getByRole('status')).toHaveTextContent('正在加载更早消息');
        fireEvent.click(screen.getByRole('button', { name: '正在加载…' }));
        expect(load).toHaveBeenCalledOnce();
        rerender(
            <ChatSearch
                messages={items}
                history={{ loading: false, loaded: true, done: true, error: '' }}
                {...props}
            />,
        );
        expect(input).toHaveValue('hello');
        expect(screen.getByRole('status')).toHaveTextContent('2 条结果 · 已到最早消息');
        expect(screen.getByRole('option', { selected: true })).toHaveTextContent('Hello 3');
        expect(screen.queryByRole('button', { name: '加载更早消息' })).not.toBeInTheDocument();
        expect(load).toHaveBeenCalledOnce();
    });

    it('shows history errors with a retry action and disables loading while disconnected', () => {
        const load = vi.fn();
        const props = {
            messages: messages(),
            onReveal: vi.fn(),
            onClose: vi.fn(),
            onLoadEarlier: load,
            history: { loading: false, loaded: true, done: false, error: '请求超时' },
        };
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
        const { rerender } = render(
            <ChatSearch
                {...props}
                history={{ loading: true, loaded: false, done: false, error: '' }}
            />,
        );
        expect(screen.getByRole('status')).toHaveTextContent('正在读取消息');
        expect(screen.getByText(/已加载的 0 条消息/)).toBeInTheDocument();
        fireEvent.change(openSearchInput(), { target: { value: 'hello' } });
        expect(within(screen.getByRole('listbox')).queryAllByRole('option')).toHaveLength(0);
        expect(screen.getByRole('status')).toHaveTextContent('可换个关键词或加载更早消息');
        rerender(
            <ChatSearch
                {...props}
                history={{ loading: false, loaded: true, done: true, error: '' }}
            />,
        );
        expect(screen.getByRole('status')).toHaveTextContent('试试其他关键词');
        expect(screen.getByRole('status')).not.toHaveTextContent('或加载更早消息');
    });

    it('merges current-session archived records with loaded messages and keeps current recall state', () => {
        const items = messages(5).map((message) => ({ ...message, recalled: false }));
        const loaded = {
            ...items[1],
            segments: [{ type: 'text', data: { text: '现在的正文' } }],
        };
        const reveal = vi.fn();
        const revealMessage = vi.fn();
        render(
            <ChatSearch
                messages={[loaded, { ...items[2], recalled: true }, items[3]]}
                archivedMessages={[
                    items[0],
                    {
                        ...items[1],
                        key: 'archive/old-key',
                        segments: [{ type: 'text', data: { text: '归档旧正文' } }],
                    },
                    items[2],
                    { ...items[3], recalled: true },
                    { ...items[4], session: 'private:55', key: 'private:55/5' },
                ]}
                currentSession="group:12"
                onReveal={reveal}
                onRevealMessage={revealMessage}
                onClose={vi.fn()}
            />,
        );
        expect(screen.getAllByRole('option')).toHaveLength(2);
        expect(screen.getByText(/当前会话已加载及已保存的 2 条消息/)).toBeInTheDocument();
        expect(screen.getByRole('option', { name: /现在的正文/ })).toBeInTheDocument();
        expect(screen.queryByRole('option', { name: /归档旧正文|Hello 3|Hello 4/ })).toBeNull();
        fireEvent.click(screen.getByRole('option', { name: /Hello 1/ }));
        expect(revealMessage).toHaveBeenCalledWith(items[0]);
        expect(reveal).not.toHaveBeenCalled();
    });

    it('searches only this account archive across conversations and reveals the result with its session', () => {
        const items = messages(3).map((message) => ({ ...message, recalled: false }));
        const privateMessage = {
            ...items[1],
            session: 'private:55' as const,
            key: 'private:55/2',
            segments: [{ type: 'text', data: { text: '跨会话资料' } }],
        };
        const revealMessage = vi.fn();
        render(
            <ChatSearch
                messages={[items[2]]}
                archivedMessages={[items[0], privateMessage]}
                currentSession="group:12"
                conversations={{
                    'group:12': { key: 'group:12', type: 'group', id: '12', name: '工作群' },
                    'private:55': { key: 'private:55', type: 'private', id: '55', name: '小张' },
                }}
                onReveal={vi.fn()}
                onRevealMessage={revealMessage}
                onClose={vi.fn()}
                onLoadEarlier={vi.fn()}
                history={{ loading: true, loaded: true, done: false, error: '请求超时' }}
            />,
        );
        fireEvent.click(screen.getByRole('tab', { name: '本账号' }));
        expect(screen.getAllByRole('option')).toHaveLength(2);
        expect(screen.getByText(/本账号已保存的 2 条消息 · 2 个会话/)).toBeInTheDocument();
        expect(screen.getByRole('option', { name: /小张.*跨会话资料/ })).toHaveTextContent('小张');
        expect(screen.queryByRole('option', { name: /Hello 3/ })).toBeNull();
        expect(screen.queryByRole('alert')).toBeNull();
        expect(screen.queryByRole('button', { name: /加载更早消息|正在加载/ })).toBeNull();
        expect(screen.getByRole('status')).not.toHaveTextContent('正在加载更早消息');
        const input = openSearchInput();
        fireEvent.change(input, { target: { value: '跨会话' } });
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(revealMessage).toHaveBeenCalledWith(privateMessage);
    });

    it('opens the account archive without an active conversation and labels unknown sessions', () => {
        const message = { ...messages()[1], session: 'private:55' as const, key: 'private:55/2' };
        const reveal = vi.fn();
        render(
            <ChatSearch
                messages={[]}
                archivedMessages={[message]}
                currentSession={null}
                onReveal={reveal}
                onClose={vi.fn()}
            />,
        );
        expect(screen.getByRole('tab', { name: '当前会话' })).toBeDisabled();
        expect(screen.getByRole('tab', { name: '本账号' })).toHaveAttribute(
            'aria-selected',
            'true',
        );
        expect(screen.getByRole('option', { name: /私聊 55/ })).toBeInTheDocument();
        fireEvent.keyDown(openSearchInput(), { key: 'Enter' });
        expect(reveal).toHaveBeenCalledWith(message.key);
    });

    it('uses the requested initial scope and preserves keyword and category when switching scopes', () => {
        const items = messages(2).map((message) => ({
            ...message,
            recalled: false,
            segments: [{ type: 'file', data: { name: '资料.pdf' } }],
        }));
        render(
            <ChatSearch
                messages={[items[0]]}
                archivedMessages={[
                    items[0],
                    { ...items[1], session: 'private:55', key: 'private:55/2' },
                ]}
                currentSession="group:12"
                initialScope="account"
                onReveal={vi.fn()}
                onClose={vi.fn()}
            />,
        );
        const input = openSearchInput();
        fireEvent.change(input, { target: { value: '资料' } });
        fireEvent.click(screen.getByRole('tab', { name: '文件' }));
        expect(screen.getAllByRole('option')).toHaveLength(2);
        const accountTab = screen.getByRole('tab', { name: '本账号' });
        fireEvent.keyDown(accountTab, { key: 'ArrowLeft' });
        expect(input).toHaveValue('资料');
        expect(screen.getByRole('tab', { name: '文件' })).toHaveAttribute('aria-selected', 'true');
        expect(screen.getByRole('tab', { name: '当前会话' })).toHaveFocus();
        expect(screen.getAllByRole('option')).toHaveLength(1);
    });
});
