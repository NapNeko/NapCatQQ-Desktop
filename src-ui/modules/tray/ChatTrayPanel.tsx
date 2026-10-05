// 账号右键菜单和未读预览；展示摘要不会改变已读状态。
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from 'react';
import { Bell, Check, EyeOff, MessageCircle, Monitor, Radio, X } from 'lucide-react';
import { chatTrayService } from '../../core/services/chat-tray.service';
import type { ChatTrayPanelData } from '../../core/ipc/generated/chat/ChatTrayPanelData';
import type { ChatTrayPanelAction } from '../../core/ipc/generated/chat/ChatTrayPanelAction';
import { errorText } from '../../core/domain/errors';
import { ChatAvatar } from '../chat/ChatAvatar';
import { TrayPanelAction, TrayPanelHeader, TrayPanelSeparator, TrayPanelSurface } from './trayPanelParts';
import './chat-tray.css';

function connectionText(data: ChatTrayPanelData) {
    const { account } = data.snapshot;
    if (!account.target.running || account.target.online === false) return '离线';
    if (!account.preference.background) return '按需接收';
    switch (account.connection.state) {
        case 'connected': return '在线';
        case 'connecting': return '连接中';
        case 'reconnecting': return '重新连接中';
        default: return '接收已暂停';
    }
}

export function ChatTrayPanel() {
    const [data, setData] = useState<ChatTrayPanelData | null>(null);
    const [error, setError] = useState('');
    const [busy, setBusy] = useState(false);
    const root = useRef<HTMLDivElement>(null);
    const current = useRef<ChatTrayPanelData | null>(null);
    const sequence = useRef(0);
    const focusedGeneration = useRef<number | null>(null);
    const reload = useCallback(async () => {
        const ticket = ++sequence.current;
        try {
            const next = await chatTrayService.data();
            if (ticket !== sequence.current) return;
            if (current.current?.generation !== next?.generation) setError('');
            current.current = next; setData(next);
        } catch (cause) { if (ticket === sequence.current) setError(errorText(cause)); }
    }, []);
    useEffect(() => {
        let alive = true;
        const subscription = chatTrayService.onChanged(() => { if (alive) void reload(); });
        void subscription.then(() => { if (alive) void reload(); }).catch(cause => { if (alive) setError(errorText(cause)); });
        return () => { alive = false; sequence.current++; void subscription.then(unlisten => unlisten()).catch(() => {}); };
    }, [reload]);
    useLayoutEffect(() => {
        if (!data || !root.current) return;
        let alive = true;
        const size = () => {
            const height = Math.ceil(root.current?.getBoundingClientRect().height ?? 0);
            if (height < 80) return;
            void chatTrayService.ready(data.generation, height).then(() => {
                if (!alive || !data.menu || focusedGeneration.current === data.generation) return;
                focusedGeneration.current = data.generation;
                root.current?.querySelector<HTMLButtonElement>('[role="menuitem"]')?.focus();
            }).catch(cause => { if (alive) setError(errorText(cause)); });
        };
        size();
        const observer = new ResizeObserver(size);
        observer.observe(root.current);
        return () => { alive = false; observer.disconnect(); };
    }, [data]);
    const act = async (action: ChatTrayPanelAction, session?: string) => {
        if (!data || busy) return;
        const generation = data.generation;
        setBusy(true); setError('');
        try {
            await chatTrayService.action(generation, action, session);
            if (current.current?.generation === generation && action === 'background') await reload();
        } catch (cause) { if (current.current?.generation === generation) setError(errorText(cause)); }
        finally { setBusy(false); }
    };
    const keyboard = (event: KeyboardEvent<HTMLDivElement>) => {
        if (!data) return;
        if (event.key === 'Escape') { event.preventDefault(); void chatTrayService.hide(data.generation).catch(cause => setError(errorText(cause))); return; }
        if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
        const items = Array.from(root.current?.querySelectorAll<HTMLButtonElement>('[data-tray-item]:not(:disabled)') ?? []);
        if (!items.length) return;
        event.preventDefault();
        const index = items.indexOf(document.activeElement as HTMLButtonElement);
        const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (index + (event.key === 'ArrowUp' ? -1 : 1) + items.length) % items.length;
        items[next]?.focus();
    };
    if (!data) return <TrayPanelSurface ref={root} className="chat-tray"><p className="chat-tray-empty" role="status">{error || '正在读取账号…'}</p></TrayPanelSurface>;
    const { account, conversations, conversationCount } = data.snapshot;
    const { target, preference } = account;
    const status = connectionText(data);
    const online = status === '在线';
    return <TrayPanelSurface ref={root} className="chat-tray" onKeyDown={keyboard} aria-label={data.menu ? '账号托盘菜单' : '未读消息'}>
            <TrayPanelHeader title={target.name} icon={<span className="chat-tray-self"><ChatAvatar contact={{ type: 'private', id: preference.selfId, name: target.name }} /></span>}>
                <span aria-hidden className={'h-1.5 w-1.5 shrink-0 rounded-full ' + (online ? 'bg-success' : 'bg-text-disabled')} />
                <span className="truncate">{status} · QQ: {preference.selfId}</span>
            </TrayPanelHeader>
            <TrayPanelSeparator />
            {data.menu ? <div role="menu" aria-label="账号操作">
                <div className="flex flex-col px-1 py-0.5">
                    <TrayPanelAction icon={<MessageCircle size={14} strokeWidth={1.9} />} title="打开聊天" data-tray-item disabled={busy} onClick={() => void act('open')} trailing={account.notificationUnread > 0 && <span className="chat-tray-count">{account.notificationUnread > 99 ? '99+' : account.notificationUnread}</span>} />
                    <TrayPanelAction icon={<Monitor size={14} strokeWidth={1.9} />} title="打开控制台" data-tray-item disabled={busy} onClick={() => void act('console')} />
                </div>
                <TrayPanelSeparator />
                <div className="flex flex-col px-1 py-0.5">
                    <TrayPanelAction icon={<Radio size={14} strokeWidth={1.9} />} title="后台接收消息" role="menuitemcheckbox" aria-checked={preference.background} data-tray-item disabled={busy} onClick={() => void act('background')} trailing={preference.background && <Check size={14} strokeWidth={1.9} className="text-brand" aria-hidden />} />
                </div>
                <TrayPanelSeparator />
                <div className="flex flex-col px-1 pb-1 pt-0.5">
                    <TrayPanelAction icon={<EyeOff size={14} strokeWidth={1.9} />} title="隐藏此账号托盘" data-tray-item disabled={busy} onClick={() => void act('hide')} />
                </div>
            </div> : <>
                <div className="chat-tray-heading"><span>未读消息</span>{conversationCount > 0 && <span>{conversationCount} 个会话</span>}</div>
                {conversations.length ? <div className="chat-tray-conversations" aria-label="未读会话">
                    {conversations.map(conversation => <button type="button" key={conversation.key} data-tray-item disabled={busy} className="chat-tray-conversation" onClick={() => void act('open', conversation.key)}>
                        <ChatAvatar contact={conversation} />
                        <span className="chat-tray-message"><strong>{conversation.name}</strong><span>{conversation.preview || '新消息'}</span></span>
                        <span className="chat-tray-count">{conversation.unread > 99 ? '99+' : conversation.unread}</span>
                    </button>)}
                </div> : <div className="chat-tray-empty"><Bell size={23} strokeWidth={1.5} /><span>暂无新消息</span></div>}
                <TrayPanelSeparator />
                <div className="flex flex-col px-1 pb-1 pt-0.5"><TrayPanelAction role="button" icon={<MessageCircle size={14} strokeWidth={1.9} />} title={conversationCount > conversations.length ? '查看全部会话' : '打开聊天'} data-tray-item disabled={busy} onClick={() => void act('open')} /></div>
            </>}
            {error && <div className="chat-tray-error mx-2 mb-1 rounded-md bg-danger-soft px-2 py-1 text-[11px] leading-snug text-danger" role="alert"><span>{error}</span><button type="button" aria-label="关闭错误提示" onClick={() => setError('')}><X size={13} /></button></div>}
    </TrayPanelSurface>;
}
