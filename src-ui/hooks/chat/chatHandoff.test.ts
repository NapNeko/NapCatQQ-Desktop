import { describe, expect, it, vi } from 'vitest';
import { ChatAccountStore, releaseChatAccount, chatAccount, restoreChatView, loadChatView, prepareChatHandoff } from './chatStore';
import { chatDesktopService } from '../../core/services/chat-desktop.service';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
const target: DebugTarget = { bot_id: 'bot', qq_id: 99, name: '测试', running: false, online: false, backend: 'napcat', host: { kind: 'local' } };
const transport = () => ({ call: vi.fn(), subscribe: vi.fn(), unsubscribe: vi.fn() });
vi.mock('../../core/services/chat-desktop.service', () => ({ chatDesktopService: { saveView: vi.fn(async () => {}), loadView: vi.fn(), setReading: vi.fn(async () => {}), releaseAccount: vi.fn(async () => {}), flush: vi.fn(async () => {}) } }));

describe('chat window handoff', () => {
    it('retains unsent text, media, reply, mentions, active conversation and reading position', () => {
        const source = new ChatAccountStore(target, transport());
        source.open({ key: 'private:22', type: 'private', id: '22', name: '朋友' });
        source.draft('private:22', { text: '@朋友 还没发出的消息', mentions: [{ qq: '22', label: '@朋友' }], reply: { id: '100', name: '朋友', preview: '上次的话' }, attachments: [{ type: 'image', key: 'image', name: '图.png', path: 'C:/preview/图.png', subType: 1 }, { type: 'file', key: 'file', name: '资料.txt', path: 'C:/preview/资料.txt' }, { type: 'face', key: 'face', name: '表情', id: '14' }] });
        source.scroll('private:22', 640);
        source.readingPosition('private:22', { messageKey: 'private:22/42', messageId: '42', offset: 31, atBottom: false });
        const handoff = JSON.parse(JSON.stringify(source.view()));
        const destination = new ChatAccountStore(target, transport());
        destination.restoreView(handoff);
        expect(destination.getSnapshot().account.drafts).toEqual(source.getSnapshot().account.drafts);
        expect(destination.getSnapshot().account.active).toBe('private:22');
        expect(destination.scroll('private:22')).toBe(640);
        expect(destination.readingPosition('private:22')).toEqual({ messageKey: 'private:22/42', messageId: '42', offset: 31, atBottom: false });
    });
    it('captures the current timeline before serializing and leaves prior snapshots intact', () => {
        const source = new ChatAccountStore(target, transport());
        const release = source.captureTimeline('private:22', () => ({ messageKey: 'private:22/80', messageId: '80', offset: 17, atBottom: true }));
        source.scroll('private:22', 500);
        const saved = source.view();
        source.scroll('private:22', 800);
        expect(saved.scroll['private:22']).toBe(500);
        expect(saved.reading['private:22'].messageKey).toBe('private:22/80');
        expect(saved.reading['private:22'].atBottom).toBe(true);
        release();
    });
    it('applies the incoming view to an account that already exists in this webview', async () => {
        const identity = { ...target, bot_id: 'handoff-existing' };
        const existing = chatAccount(identity);
        existing.draft('private:22', { text: '旧草稿', reply: null, attachments: [] });
        const source = new ChatAccountStore(identity, transport());
        source.open({ key: 'group:33', type: 'group', id: '33', name: '群' });
        source.draft('group:33', { text: '独立窗新草稿', reply: null, attachments: [] });
        restoreChatView({ v: 1, revision: 8, selectedBot: identity.bot_id, accounts: [source.view()] });
        expect(existing.getSnapshot().account.active).toBe('group:33');
        expect(existing.getSnapshot().account.drafts['group:33'].text).toBe('独立窗新草稿');
        expect(existing.getSnapshot().account.drafts['private:22']).toBeUndefined();
        await releaseChatAccount(existing);
    });
    it('coalesces concurrent mount claims into one view revision', async () => {
        const view = { v: 1, revision: 9, selectedBot: null, accounts: [] };
        vi.mocked(chatDesktopService.loadView).mockResolvedValue(view);
        const first = loadChatView(); const second = loadChatView();
        expect(first).toBe(second);
        await expect(first).resolves.toEqual(view);
        expect(chatDesktopService.loadView).toHaveBeenCalledOnce();
    });
    it('does not restore a draft into another logged-in identity', () => {
        const source = new ChatAccountStore(target, transport());
        source.draft('private:22', { text: '只属于这个账号', reply: null, attachments: [] });
        const destination = new ChatAccountStore({ ...target, qq_id: 100 }, transport());
        destination.restoreView(source.view());
        expect(destination.getSnapshot().account.drafts).toEqual({});
    });
    it('opens a selected conversation at the latest message instead of reusing its handoff position', () => {
        const source = new ChatAccountStore(target, transport());
        const contact = { key: 'private:22' as const, id: '22', type: 'private' as const, name: '朋友' };
        source.open(contact); source.readingPosition(contact.key, { messageKey: 'private:22/42', messageId: '42', offset: 30, atBottom: false });
        const destination = new ChatAccountStore(target, transport()); destination.restoreView(source.view());
        expect(destination.initialReadingPosition(contact.key)).toBeDefined();
        destination.open(contact);
        expect(destination.initialReadingPosition(contact.key)).toBeUndefined();
    });
    it('saves an ordinary departed workspace with drafts and without a reading restoration', async () => {
        const store = chatAccount({ ...target, bot_id: 'normal-reentry' });
        store.restoreView({ botId: 'normal-reentry', selfId: '99', active: 'private:22', drafts: { 'private:22': { text: '保留草稿', attachments: [], reply: null, mentions: [] } }, reading: { 'private:22': { messageKey: 'private:22/42', messageId: '42', offset: 31, atBottom: false } }, scroll: { 'private:22': 800 } });
        await releaseChatAccount(store);
        const saved = vi.mocked(chatDesktopService.saveView).mock.calls.at(-1)![0].accounts.find(view => view.botId === 'normal-reentry')!;
        expect(saved.drafts['private:22'].text).toBe('保留草稿');
        expect(saved.reading).toEqual({}); expect(saved.scroll).toEqual({});
    });
    it('does not save a reading restoration when closing the chat window without embedding', async () => {
        const store = chatAccount({ ...target, bot_id: 'close-without-embed' });
        store.restoreView({ botId: 'close-without-embed', selfId: '99', active: 'private:22', drafts: {}, reading: { 'private:22': { messageKey: 'private:22/42', messageId: '42', offset: 31, atBottom: false } }, scroll: { 'private:22': 800 } });
        await prepareChatHandoff(store.target.bot_id, undefined, false);
        const saved = vi.mocked(chatDesktopService.saveView).mock.calls.at(-1)![0].accounts.find(view => view.botId === 'close-without-embed')!;
        expect(saved.reading).toEqual({}); expect(saved.scroll).toEqual({});
    });
    it('keeps a remounted workspace connected until its final viewer leaves', async () => {
        const wire = transport(); wire.subscribe.mockResolvedValue({ subscription_id: 'lease', receiver: { state: { state: 'connected' } } });
        const store = new ChatAccountStore({ ...target, running: true, online: true }, wire);
        await store.connect();
        const unlisten = store.subscribe(() => {});
        await releaseChatAccount(store);
        expect(wire.unsubscribe).not.toHaveBeenCalled();
        unlisten(); await releaseChatAccount(store);
        expect(wire.unsubscribe).toHaveBeenCalledWith('lease');
    });
    it('does not establish a late connection after leaving during archive restoration', async () => {
        let restore!: (archive: null) => void;
        const wire = transport();
        const archive = { load: vi.fn(() => new Promise<null>(done => { restore = done; })), save: vi.fn(async () => {}) };
        const store = new ChatAccountStore({ ...target, running: true, online: true }, wire, archive);
        const initializing = store.initialize(); const leaving = releaseChatAccount(store);
        restore(null); await Promise.all([initializing, leaving]);
        expect(wire.subscribe).not.toHaveBeenCalled();
    });
    it('releases a departed workspace after the pending send settles', async () => {
        let reject!: (error: Error) => void;
        const pending = new Promise<never>((_, fail) => { reject = fail; });
        const wire = transport(); wire.call.mockReturnValue(pending); wire.subscribe.mockResolvedValue({ subscription_id: 'sending-lease', receiver: { state: { state: 'connected' } } });
        const store = new ChatAccountStore({ ...target, running: true, online: true }, wire);
        await store.connect(); store.open({ key: 'private:22', id: '22', type: 'private', name: '朋友' });
        store.draft('private:22', { text: '发送中的消息', reply: null, attachments: [] });
        const sending = store.send('private:22');
        await releaseChatAccount(store);
        expect(wire.unsubscribe).not.toHaveBeenCalled();
        reject(new Error('连接超时')); await sending;
        await vi.waitFor(() => expect(wire.unsubscribe).toHaveBeenCalledWith('sending-lease'));
        expect(store.getSnapshot().account.messages[0].status).toBe('unknown');
        expect(chatDesktopService.releaseAccount).toHaveBeenCalledWith('bot', '99');
    });
});
