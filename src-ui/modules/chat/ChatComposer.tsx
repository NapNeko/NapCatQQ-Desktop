// 会话草稿即时写回账号分区，异步选文件不改变发送目标。
import { useEffect, useId, useLayoutEffect, useRef, useState, type ClipboardEvent, type DragEvent, type KeyboardEvent } from 'react';
import { ArrowUp, AtSign, Check, ChevronDown, File, ImagePlus, Paperclip, Smile, X } from 'lucide-react';
import { EMPTY_DRAFT, type Attachment, type Contact, type Draft } from '../../core/domain/chat/model';
import { mentionLabel, mentionQueryAt, pruneMentions } from '../../core/domain/debug/composerModel';
import { errorText } from '../../core/domain/errors';
import { chatService } from '../../core/services/chat.service';
import { useChatSnapshot, type ChatAccountStore } from '../../hooks/chat/chatStore';
import { Popover, PopoverAnchor, PopoverContent, PopoverTrigger } from '../../shared/ui/Popover';
import type { SendShortcut } from './chatPreferences';
import { ChatEmojiPicker } from './media/ChatEmojiPicker';
import { QQFace } from './media/QQFace';
import { ChatAvatar } from './ChatAvatar';

export function ChatComposer({ store, contact, disabledReason, sendShortcut = 'enter', onSendShortcutChange }: { store: ChatAccountStore; contact: Contact; disabledReason: string; sendShortcut?: SendShortcut; onSendShortcutChange?: (shortcut: SendShortcut) => void }) {
    const snapshot = useChatSnapshot(store); const draft = snapshot.account.drafts[contact.key] ?? EMPTY_DRAFT;
    const input = useRef<HTMLTextAreaElement>(null); const composing = useRef(false);
    const [emoji, setEmoji] = useState(false); const [error, setError] = useState('');
    const restoreAfterEmoji = useRef(false);
    const [settingsOpen, setSettingsOpen] = useState(false);
    const [pendingFiles, setPendingFiles] = useState(0);
    const [dragging, setDragging] = useState(false); const dragDepth = useRef(0);
    const [caret, setCaret] = useState(0); const [members, setMembers] = useState<{ id: string; name: string }[]>([]);
    const [memberError, setMemberError] = useState(''); const [memberLoading, setMemberLoading] = useState(false);
    const [mentionDismissed, dismissMention] = useState(false);
    const [memberIndex, setMemberIndex] = useState(0); const [memberRetry, retryMembers] = useState(0);
    const membersLoaded = useRef(false); const memberList = useRef<HTMLDivElement>(null); const memberListId = useId();
    const query = contact.type === 'group' && !mentionDismissed ? mentionQueryAt(draft.text, caret) : null;
    const mentionOpen = query !== null;
    const memberVisible = mentionOpen && !disabledReason;
    useEffect(() => {
        if (!mentionOpen || membersLoaded.current || disabledReason) return;
        let cancelled = false; setMemberLoading(true); setMemberError('');
        void store.members(contact.key).then(value => { if (!cancelled) { setMembers(value); membersLoaded.current = true; } }).catch(e => { if (!cancelled) setMemberError(errorText(e)); }).finally(() => { if (!cancelled) setMemberLoading(false); });
        return () => { cancelled = true; };
    }, [mentionOpen, store, contact.key, disabledReason, memberRetry]);
    const filteredMembers = query ? members.filter(m => `${m.name} ${m.id}`.toLocaleLowerCase().includes(query.query.toLocaleLowerCase())).slice(0, 30) : [];
    const selectedMember = Math.min(memberIndex, Math.max(0, filteredMembers.length - 1));
    useEffect(() => { setMemberIndex(0); }, [query?.query]);
    useEffect(() => { memberList.current?.querySelector('[aria-selected=true]')?.scrollIntoView?.({ block: 'nearest' }); }, [selectedMember]);
    useEffect(() => { if (draft.reply) input.current?.focus(); }, [draft.reply]);
    useLayoutEffect(() => {
        const element = input.current; if (!element) return;
        const resize = () => { element.style.height = 'auto'; element.style.height = `${Math.min(140, Math.max(52, element.scrollHeight))}px`; };
        resize(); let width = element.clientWidth;
        const observer = new ResizeObserver(() => { if (element.clientWidth !== width) { width = element.clientWidth; resize(); } });
        observer.observe(element); return () => observer.disconnect();
    }, [draft.text]);
    const current = () => store.getSnapshot().account.drafts[contact.key] ?? EMPTY_DRAFT;
    const patch = (value: Partial<Draft>) => store.draft(contact.key, { ...current(), ...value });
    const chooseEmoji = (attachment: Attachment) => {
        if (current().attachments.length >= 8) { setError('一次最多添加 8 个附件'); return; }
        patch({ attachments: [...current().attachments, attachment] });
        restoreAfterEmoji.current = true; setEmoji(false); setError('');
    };
    const insert = (value: string) => {
        const currentDraft = current(); const start = input.current?.selectionStart ?? currentDraft.text.length; const end = input.current?.selectionEnd ?? start;
        const text = currentDraft.text.slice(0, start) + value + currentDraft.text.slice(end);
        patch({ text, mentions: pruneMentions(text, currentDraft.mentions ?? []) });
        setCaret(start + value.length); requestAnimationFrame(() => { input.current?.focus(); input.current?.setSelectionRange(start + value.length, start + value.length); });
    };
    const pick = async (type: 'image' | 'file') => {
        setError(''); setPendingFiles(count => count + 1);
        try {
            const file = await chatService.pickFile(); if (!file) return;
            if (current().attachments.length >= 8) throw new Error('一次最多添加 8 个附件');
            if (type === 'image' && !/.(png|jpe?g|gif|webp|bmp)$/i.test(file.name)) throw new Error('请选择 PNG、JPG、GIF、WebP 或 BMP 图片');
            patch({ attachments: [...current().attachments, { ...file, key: crypto.randomUUID(), type }] });
        } catch (e) { setError(errorText(e)); }
        finally { setPendingFiles(count => count - 1); }
    };
    const addImage = async (file: globalThis.File) => {
        setPendingFiles(count => count + 1);
        try {
            if (file.size > 10 * 1024 * 1024) throw new Error('粘贴图片不能超过 10 MB');
            const data = await new Promise<string>((resolve, reject) => { const reader = new FileReader(); reader.onload = () => resolve(String(reader.result)); reader.onerror = () => reject(new Error('读取图片失败')); reader.readAsDataURL(file); });
            if (current().attachments.length >= 8) throw new Error('一次最多添加 8 个附件');
            patch({ attachments: [...current().attachments, { key: crypto.randomUUID(), path: `base64://${data.slice(data.indexOf(',') + 1)}`, name: file.name || '粘贴的图片.png', type: 'image' }] });
        } catch (e) { setError(errorText(e)); }
        finally { setPendingFiles(count => count - 1); }
    };
    const paste = (event: ClipboardEvent) => { const images = Array.from(event.clipboardData.files).filter(f => f.type.startsWith('image/')); if (images.length) { event.preventDefault(); for (const file of images) void addImage(file); } };
    const drop = (event: DragEvent) => { event.preventDefault(); dragDepth.current = 0; setDragging(false); setError(''); const files = Array.from(event.dataTransfer.files); const images = files.filter(f => f.type.startsWith('image/')); for (const file of images) void addImage(file); if (images.length !== files.length) setError('其他文件请用附件按钮选择'); };
    const sending = snapshot.account.messages.some(m => m.session === contact.key && m.status === 'sending');
    const canSend = !disabledReason && !sending && !pendingFiles && (!!draft.text.trim() || draft.attachments.length > 0);
    const send = () => { if (!canSend) return; setEmoji(false); setError(''); void store.send(contact.key).catch(e => setError(errorText(e))); };
    const chooseMember = (member: { id: string; name: string }) => {
        if (!query) return;
        const value = current();
        const label = mentionLabel(member.name, member.id, value.mentions ?? [], members.filter(m => m.name === member.name).length);
        patch({ text: value.text.slice(0, query.start) + label + ' ' + value.text.slice(caret), mentions: [...(value.mentions ?? []), { qq: member.id, label }] });
        const nextCaret = query.start + label.length + 1; setCaret(nextCaret); dismissMention(true);
        requestAnimationFrame(() => { input.current?.focus(); input.current?.setSelectionRange(nextCaret, nextCaret); });
    };
    const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
        if (event.nativeEvent.isComposing || composing.current || event.keyCode === 229) return;
        if (event.key === 'Escape') { dismissMention(true); setEmoji(false); return; }
        if (memberVisible && query) {
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
                event.preventDefault();
                if (filteredMembers.length) setMemberIndex((selectedMember + (event.key === 'ArrowDown' ? 1 : -1) + filteredMembers.length) % filteredMembers.length);
                return;
            }
            if ((event.key === 'Enter' || event.key === 'Tab') && !event.shiftKey) {
                event.preventDefault();
                if (filteredMembers[selectedMember]) chooseMember(filteredMembers[selectedMember]);
                else if (event.key === 'Tab') dismissMention(true);
                return;
            }
        }
        if (event.key === 'Enter' && !event.shiftKey && !event.altKey && (sendShortcut === 'ctrl-enter' ? event.ctrlKey || event.metaKey : !event.ctrlKey && !event.metaKey)) { event.preventDefault(); send(); }
    };
    return <div className="native-chat-composer" onPaste={paste} onDragOver={e => e.preventDefault()} onDrop={drop}
        onDragEnter={e => { if (Array.from(e.dataTransfer.types).includes('Files')) { dragDepth.current++; setDragging(true); } }}
        onDragLeave={() => { dragDepth.current = Math.max(0, dragDepth.current - 1); if (!dragDepth.current) setDragging(false); }}>
        <div className="native-chat-editor" data-dragging={dragging}>
        {draft.reply && <div className="native-chat-reply"><span className="min-w-0 flex-1 truncate">回复 {draft.reply.name}：{draft.reply.preview}</span><button className="native-chat-icon" aria-label="取消引用" onClick={() => patch({ reply: null })}><X size={13} /></button></div>}
        {draft.attachments.length > 0 && <div className="native-chat-attachments">{draft.attachments.map(file => <div key={file.key}><AttachmentPreview attachment={file} /><span className="max-w-36 truncate">{file.name}</span><button aria-label={`移除${file.name}`} onClick={() => patch({ attachments: current().attachments.filter(f => f.key !== file.key) })}><X size={12} /></button></div>)}</div>}
        <div className="native-chat-composer-tools">
            <Popover open={emoji} onOpenChange={open => { if (open) restoreAfterEmoji.current = false; setEmoji(open); }}>
                <PopoverTrigger asChild><button className="native-chat-icon" aria-label="表情" title="表情"><Smile size={18} /></button></PopoverTrigger>
                <PopoverContent side="top" align="start" className="native-chat-popover w-auto p-2" aria-label="选择表情" onCloseAutoFocus={e => { if (restoreAfterEmoji.current) { e.preventDefault(); input.current?.focus(); } }}><ChatEmojiPicker target={store.target} onSelect={chooseEmoji} disabledReason={disabledReason} /></PopoverContent>
            </Popover>
            <button className="native-chat-icon" aria-label="添加图片" title="添加图片" disabled={!!pendingFiles} onClick={() => void pick('image')}><ImagePlus size={18} /></button>
            <button className="native-chat-icon" aria-label="添加文件" title="添加文件" disabled={!!pendingFiles} onClick={() => void pick('file')}><Paperclip size={18} /></button>
            {contact.type === 'group' && <button className="native-chat-icon" aria-label="提及成员" title="提及成员" onClick={() => { dismissMention(false); insert('@'); }}><AtSign size={18} /></button>}
        </div>
        <Popover open={memberVisible} onOpenChange={open => { if (!open) dismissMention(true); }}>
            <PopoverAnchor asChild><div>
                <textarea ref={input} aria-label={`发送消息给${contact.name}`} placeholder="写点什么…" value={draft.text} rows={2}
                    aria-autocomplete={contact.type === 'group' ? 'list' : undefined}
                    aria-controls={memberVisible ? memberListId : undefined}
                    aria-activedescendant={memberVisible && filteredMembers[selectedMember] ? `${memberListId}-${filteredMembers[selectedMember].id}` : undefined}
                    onChange={e => { patch({ text: e.target.value, mentions: pruneMentions(e.target.value, draft.mentions ?? []) }); setCaret(e.target.selectionStart); dismissMention(false); }}
                    onSelect={e => setCaret(e.currentTarget.selectionStart)}
                    onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }} onKeyDown={onKeyDown} />
            </div></PopoverAnchor>
            <PopoverContent side="top" align="start" className="native-chat-popover native-chat-mentions"
                onOpenAutoFocus={e => e.preventDefault()} onCloseAutoFocus={e => e.preventDefault()}
                onInteractOutside={e => { if (e.detail.originalEvent.target === input.current) e.preventDefault(); }}
                onEscapeKeyDown={() => dismissMention(true)}>
                <div ref={memberList} id={memberListId} role="listbox" aria-label="群成员建议">
                    {memberLoading ? <p role="status">正在读取群成员…</p> : memberError ? <><p role="status">{memberError}</p><button onClick={() => retryMembers(n => n + 1)}>重试读取成员</button></> : !filteredMembers.length ? <p role="status">没有匹配的成员</p> : filteredMembers.map((member, index) => <button key={member.id} id={`${memberListId}-${member.id}`} role="option" aria-selected={index === selectedMember} tabIndex={-1} onMouseDown={e => e.preventDefault()} onClick={() => chooseMember(member)}><ChatAvatar contact={{ type: 'private', id: member.id, name: member.name || member.id }} small /><span className="min-w-0 flex-1 truncate">{member.name || member.id}</span><span>{member.id}</span></button>)}
                </div>
            </PopoverContent>
        </Popover>
        {dragging && <div className="native-chat-drop-hint">松开添加图片</div>}
        <footer>
            <span role="status" className={error ? 'text-danger' : 'text-text-tertiary'}>{error || (pendingFiles ? '正在读取附件…' : disabledReason) || (sendShortcut === 'enter' ? 'Enter 发送 · Shift + Enter 换行' : 'Ctrl + Enter 发送 · Enter 换行')}</span>
            <div className="native-chat-send-group"><button className="native-chat-send" aria-label="发送消息" title={disabledReason || '发送消息'} disabled={!canSend} onClick={send}><ArrowUp size={16} /><span>{sending ? '发送中' : '发送'}</span></button>
                {onSendShortcutChange && <Popover open={settingsOpen} onOpenChange={setSettingsOpen}>
                    <PopoverTrigger asChild><button className="native-chat-send-options" aria-label="发送设置" title="发送快捷键"><ChevronDown size={13} /></button></PopoverTrigger>
                    <PopoverContent side="top" align="end" className="native-chat-popover w-48 p-1.5" aria-label="发送快捷键">
                        {(['enter', 'ctrl-enter'] as const).map(value => <button key={value} aria-pressed={sendShortcut === value} className="native-chat-shortcut-option" onClick={() => { onSendShortcutChange(value); setSettingsOpen(false); }}><span>{value === 'enter' ? 'Enter 发送' : 'Ctrl + Enter 发送'}</span>{sendShortcut === value && <Check size={13} />}</button>)}
                    </PopoverContent>
                </Popover>}
            </div>
        </footer>
        </div>
    </div>;
}

function AttachmentPreview({ attachment }: { attachment: Attachment }) {
    if (attachment.type === 'face') return <QQFace id={attachment.id} />;
    if (attachment.type === 'file') return <File size={16} />;
    const source = attachment.path.startsWith('base64://') ? 'data:image/png;base64,' + attachment.path.slice(9) : /^https?:\/\//i.test(attachment.path) ? attachment.path : '';
    return source ? <img src={source} alt={attachment.name} referrerPolicy="no-referrer" /> : <ImagePlus size={16} />;
}
