// 资料使用主窗口的轻量弹层，不占用第三栏。
import { useState } from 'react';
import { Check, Copy, Info, Pin, X } from 'lucide-react';
import type { Conversation } from '../../core/domain/chat/model';
import { Popover, PopoverClose, PopoverContent, PopoverTrigger } from '../../shared/ui/Popover';
import { ChatAvatar } from './ChatAvatar';

export function ChatDetails({ contact, onPin }: { contact: Conversation; onPin: () => void }) {
    const [copyState, setCopyState] = useState('');
    return <Popover onOpenChange={() => setCopyState('')}>
        <PopoverTrigger asChild><button className="native-chat-icon" aria-label="会话资料" title="会话资料"><Info size={17} /></button></PopoverTrigger>
        <PopoverContent align="end" className="native-chat-popover w-64 p-4" aria-label="会话资料">
            <div className="flex items-center justify-between gap-2"><h3 className="text-[13px] font-semibold">会话资料</h3><PopoverClose asChild><button className="native-chat-icon" aria-label="关闭会话资料"><X size={15} /></button></PopoverClose></div>
            <div className="flex items-center gap-3 py-5"><ChatAvatar contact={contact} /><div className="min-w-0"><p className="break-words text-[14px] font-semibold">{contact.name}</p><p className="mt-1 text-[11px] text-text-tertiary">{contact.type === 'group' ? contact.members ? `${contact.members} 位成员` : '群聊' : '好友'}</p></div></div>
            <button className="native-chat-detail-action" onClick={() => { void navigator.clipboard.writeText(contact.id).then(() => setCopyState('已复制'), () => setCopyState('复制失败')); }} aria-label={`复制${contact.type === 'group' ? '群号' : 'QQ 号'}`}><span className="text-text-tertiary">{contact.type === 'group' ? '群号' : 'QQ'}</span><span className="ml-auto font-mono text-[11px]">{contact.id}</span>{copyState === '已复制' ? <Check size={13} /> : <Copy size={13} />}</button>
            <button className="native-chat-detail-action" aria-pressed={contact.pinned} onClick={onPin}><Pin size={14} />置顶会话{contact.pinned && <Check size={14} className="ml-auto text-brand" />}</button>
            {copyState && <p role="status" className="pt-2 text-[11px] text-text-tertiary">{copyState}</p>}
        </PopoverContent>
    </Popover>;
}
