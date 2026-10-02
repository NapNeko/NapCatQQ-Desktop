// 会话草稿即时写回账号分区，异步选文件不改变发送目标。
import { useEffect, useRef, useState, type ClipboardEvent, type DragEvent } from 'react';
import { ArrowUp, AtSign, File, ImagePlus, Paperclip, Smile, X } from 'lucide-react';
import { EMPTY_DRAFT, type Contact, type Draft } from '../../core/domain/chat/model';
import { mentionLabel, mentionQueryAt, pruneMentions } from '../../core/domain/debug/composerModel';
import { errorText } from '../../core/domain/errors';
import { chatService } from '../../core/services/chat.service';
import { useChatSnapshot, type ChatAccountStore } from '../../hooks/chat/chatStore';

const EMOJI = ['😀', '😊', '🥰', '🤔', '😂', '😭', '👍', '👏', '🎉', '❤️', '🙏', '👀'];
export function ChatComposer({ store, contact, disabledReason }: { store: ChatAccountStore; contact: Contact; disabledReason: string }) {
    const snapshot = useChatSnapshot(store); const draft = snapshot.account.drafts[contact.key] ?? EMPTY_DRAFT;
    const input = useRef<HTMLTextAreaElement>(null); const composing = useRef(false);
    const [emoji, setEmoji] = useState(false); const [error, setError] = useState('');
    const [caret, setCaret] = useState(0); const [members, setMembers] = useState<{ id: string; name: string }[]>([]);
    const [memberError, setMemberError] = useState(''); const [memberLoading, setMemberLoading] = useState(false);
    const [mentionDismissed, dismissMention] = useState(false);
    const query = contact.type === 'group' && !mentionDismissed ? mentionQueryAt(draft.text, caret) : null;
    const mentionOpen = query !== null;
    useEffect(() => {
        if (!mentionOpen || members.length || disabledReason) return;
        let cancelled = false; setMemberLoading(true); setMemberError('');
        void store.members(contact.key).then(value => { if (!cancelled) setMembers(value); }).catch(e => { if (!cancelled) setMemberError(errorText(e)); }).finally(() => { if (!cancelled) setMemberLoading(false); });
        return () => { cancelled = true; };
    }, [mentionOpen, store, contact.key, disabledReason]);
    useEffect(() => { if (draft.reply) input.current?.focus(); }, [draft.reply]);
    const current = () => store.getSnapshot().account.drafts[contact.key] ?? EMPTY_DRAFT;
    const patch = (value: Partial<Draft>) => store.draft(contact.key, { ...current(), ...value });
    const insert = (value: string) => {
        const currentDraft = current(); const start = input.current?.selectionStart ?? currentDraft.text.length; const end = input.current?.selectionEnd ?? start;
        patch({ text: currentDraft.text.slice(0, start) + value + currentDraft.text.slice(end) });
        setCaret(start + value.length); requestAnimationFrame(() => { input.current?.focus(); input.current?.setSelectionRange(start + value.length, start + value.length); });
    };
    const pick = async (type: 'image' | 'file') => {
        setError('');
        try {
            const file = await chatService.pickFile(); if (!file) return;
            if (current().attachments.length >= 8) throw new Error('一次最多添加 8 个附件');
            if (type === 'image' && !/.(png|jpe?g|gif|webp|bmp)$/i.test(file.name)) throw new Error('请选择 PNG、JPG、GIF 或 WebP 图片');
            patch({ attachments: [...current().attachments, { ...file, key: crypto.randomUUID(), type }] });
        } catch (e) { setError(errorText(e)); }
    };
    const addImage = async (file: globalThis.File) => {
        try {
            if (file.size > 10 * 1024 * 1024) throw new Error('粘贴图片不能超过 10 MB');
            const data = await new Promise<string>((resolve, reject) => { const reader = new FileReader(); reader.onload = () => resolve(String(reader.result)); reader.onerror = () => reject(new Error('读取图片失败')); reader.readAsDataURL(file); });
            if (current().attachments.length >= 8) throw new Error('一次最多添加 8 个附件');
            patch({ attachments: [...current().attachments, { key: crypto.randomUUID(), path: `base64://${data.slice(data.indexOf(',') + 1)}`, name: file.name || '粘贴的图片.png', type: 'image' }] });
        } catch (e) { setError(errorText(e)); }
    };
    const paste = (event: ClipboardEvent) => { const images = Array.from(event.clipboardData.files).filter(f => f.type.startsWith('image/')); if (images.length) { event.preventDefault(); for (const file of images) void addImage(file); } };
    const drop = (event: DragEvent) => { event.preventDefault(); const files = Array.from(event.dataTransfer.files); const images = files.filter(f => f.type.startsWith('image/')); for (const file of images) void addImage(file); if (images.length !== files.length) setError('其他文件请用附件按钮选择'); };
    const sending = snapshot.account.messages.some(m => m.session === contact.key && m.status === 'sending');
    const canSend = !disabledReason && !sending && (!!draft.text.trim() || draft.attachments.length > 0);
    const send = () => { if (!canSend) return; setEmoji(false); setError(''); void store.send(contact.key).catch(e => setError(errorText(e))); };
    return <div className="native-chat-composer" onPaste={paste} onDragOver={e => e.preventDefault()} onDrop={drop}>
        {draft.reply && <div className="native-chat-reply"><span className="min-w-0 flex-1 truncate">回复 {draft.reply.name}：{draft.reply.preview}</span><button className="native-chat-icon" aria-label="取消引用" onClick={() => patch({ reply: null })}><X size={13} /></button></div>}
        {draft.attachments.length > 0 && <div className="native-chat-attachments">{draft.attachments.map(file => <div key={file.key}>{file.path.startsWith('base64://') ? <img src={`data:image/png;base64,${file.path.slice(9)}`} alt={file.name} /> : file.type === 'image' ? <ImagePlus size={16} /> : <File size={16} />}<span className="max-w-36 truncate">{file.name}</span><button aria-label={`移除${file.name}`} onClick={() => patch({ attachments: current().attachments.filter(f => f.key !== file.key) })}><X size={12} /></button></div>)}</div>}
        <div className="native-chat-composer-tools"><button className="native-chat-icon" aria-label="表情" aria-expanded={emoji} onClick={() => setEmoji(!emoji)}><Smile size={18} /></button><button className="native-chat-icon" aria-label="添加图片" onClick={() => void pick('image')}><ImagePlus size={18} /></button><button className="native-chat-icon" aria-label="添加文件" onClick={() => void pick('file')}><Paperclip size={18} /></button>{contact.type === 'group' && <button className="native-chat-icon" aria-label="提及成员" onClick={() => { dismissMention(false); insert('@'); }}><AtSign size={18} /></button>}</div>
        {emoji && <div className="native-chat-emoji" role="group" aria-label="选择表情">{EMOJI.map(value => <button key={value} aria-label={`插入${value}`} onClick={() => { insert(value); setEmoji(false); }}>{value}</button>)}</div>}
        {query && <div className="native-chat-mentions" role="group" aria-label="群成员建议">{memberLoading ? <p>正在读取群成员…</p> : memberError ? <p>{memberError}</p> : members.filter(m => `${m.name} ${m.id}`.toLocaleLowerCase().includes(query.query.toLocaleLowerCase())).slice(0, 30).map(member => <button key={member.id} onClick={() => {
            const label = mentionLabel(member.name, member.id, draft.mentions ?? [], members.filter(m => m.name === member.name).length);
            patch({ text: draft.text.slice(0, query.start) + label + ' ' + draft.text.slice(caret), mentions: [...(draft.mentions ?? []), { qq: member.id, label }] });
            const nextCaret = query.start + label.length + 1; setCaret(nextCaret); dismissMention(true); requestAnimationFrame(() => { input.current?.focus(); input.current?.setSelectionRange(nextCaret, nextCaret); });
        }}><span>{member.name}</span><span>{member.id}</span></button>)}</div>}
        <textarea ref={input} aria-label={`发送消息给${contact.name}`} placeholder="写点什么…" value={draft.text} rows={3} onChange={e => { patch({ text: e.target.value, mentions: pruneMentions(e.target.value, draft.mentions ?? []) }); setCaret(e.target.selectionStart); dismissMention(false); }} onSelect={e => setCaret(e.currentTarget.selectionStart)} onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }} onKeyDown={e => {
            if (e.key === 'Escape') { setEmoji(false); dismissMention(true); }
            if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing && !composing.current && e.keyCode !== 229) { e.preventDefault(); if (!query) send(); }
        }} />
        <footer><span role="status" className={error ? 'text-danger' : 'text-text-tertiary'}>{error || disabledReason || 'Enter 发送 · Shift + Enter 换行'}</span><button className="native-chat-send" aria-label="发送消息" title={disabledReason || '发送消息'} disabled={!canSend} onClick={send}><ArrowUp size={17} /><span>{sending ? '发送中' : '发送'}</span></button></footer>
    </div>;
}
