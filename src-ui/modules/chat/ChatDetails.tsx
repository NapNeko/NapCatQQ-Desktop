// 资料使用主窗口的轻量弹层，不占用第三栏。
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { ArrowLeft, Check, ChevronRight, Copy, Info, MessageCircle, Pin, RefreshCw, Search, X } from 'lucide-react';
import type { Contact, Conversation } from '../../core/domain/chat/model';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { chatProfileService, type ChatProfile, type ProfileField, type ProfileMember } from '../../core/services/chat-profile.service';
import { Popover, PopoverClose, PopoverContent, PopoverTrigger } from '../../shared/ui/Popover';
import { ChatAvatar } from './ChatAvatar';
import { useMotion } from '../../hooks/preferences/useMotion';

interface Props { contact: Conversation; target?: DebugTarget; onPin: () => void; onMessage?: (contact: Contact) => void; onSearch?: () => void; open?: boolean; onOpenChange?: (open: boolean) => void }
export function ChatDetails({ contact, target, onPin, onMessage, onSearch, open, onOpenChange }: Props) {
    const [copyState, setCopyState] = useState('');
    const [internalOpen, setInternalOpen] = useState(false);
    const [profile, setProfile] = useState<ChatProfile>();
    const [members, setMembers] = useState<ProfileMember[]>([]);
    const [selectedMember, setSelectedMember] = useState<ProfileMember>();
    const [loading, setLoading] = useState(false);
    const [membersLoading, setMembersLoading] = useState(false);
    const [error, setError] = useState('');
    const [memberError, setMemberError] = useState('');
    const [attempt, setAttempt] = useState(0);
    const [query, setQuery] = useState('');
    const [limit, setLimit] = useState(60);
    const leaving = useRef(false);
    const visible = open ?? internalOpen;
    const change = (next: boolean) => { setCopyState(''); setInternalOpen(next); onOpenChange?.(next); if (!next) { setSelectedMember(undefined); setQuery(''); setLimit(60); } };
    const leave = (action: () => void) => { leaving.current = true; change(false); action(); };
    useEffect(() => {
        if (!visible || !target) return;
        let cancelled = false;
        setError(''); setMemberError(''); setLoading(true);
        void chatProfileService.info(target, contact).then(value => { if (!cancelled) setProfile(value); }, reason => { if (!cancelled) setError(reason instanceof Error ? reason.message : '资料读取失败'); }).finally(() => { if (!cancelled) setLoading(false); });
        if (contact.type === 'group') {
            setMembersLoading(true);
            void chatProfileService.members(target, contact.id).then(value => { if (!cancelled) setMembers(value); }, reason => { if (!cancelled) setMemberError(reason instanceof Error ? reason.message : '成员读取失败'); }).finally(() => { if (!cancelled) setMembersLoading(false); });
        }
        return () => { cancelled = true; };
    }, [visible, target?.bot_id, target?.backend, contact.id, contact.type, attempt]);
    const term = query.trim().toLocaleLowerCase();
    const filtered = members.filter(member => !term || `${member.name} ${member.nickname} ${member.id}`.toLocaleLowerCase().includes(term));
    const displayed = selectedMember ?? { ...contact, name: profile?.name || contact.name };
    const copy = async () => {
        try { await navigator.clipboard.writeText(displayed.id); setCopyState('已复制'); } catch { setCopyState('复制失败'); }
    };
    const role = (member: ProfileMember) => member.role === 'owner' ? '群主' : member.role === 'admin' ? '管理员' : '群成员';
    const memberFields: ProfileField[] = selectedMember ? [
        { label: '昵称', value: selectedMember.nickname }, { label: '身份', value: role(selectedMember) },
        { label: '头衔', value: selectedMember.title }, { label: '加入于', value: selectedMember.joined ?? '' },
        { label: '最近发言', value: selectedMember.lastSent ?? '' },
    ].filter(field => field.value) : [];
    return <Popover open={visible} onOpenChange={change}>
        <PopoverTrigger asChild><button type="button" className="native-chat-icon" aria-label="会话资料" title="会话资料"><Info size={17} /></button></PopoverTrigger>
        <PopoverContent align="end" className="native-chat-popover native-chat-details" aria-label="会话资料" onCloseAutoFocus={event => { if (leaving.current) { event.preventDefault(); leaving.current = false; } }}>
            <div className="native-chat-details-heading">{selectedMember && <button type="button" className="native-chat-icon" aria-label="返回群资料" onClick={() => { setSelectedMember(undefined); setCopyState(''); }}><ArrowLeft size={15} /></button>}<h3>{selectedMember ? '群成员' : contact.type === 'group' ? '群资料' : '个人资料'}</h3>{target && !selectedMember && <button type="button" className="native-chat-icon" aria-label="刷新会话资料" disabled={loading || membersLoading} onClick={() => setAttempt(value => value + 1)}><RefreshCw size={13} /></button>}<PopoverClose asChild><button type="button" className="native-chat-icon" aria-label="关闭会话资料"><X size={15} /></button></PopoverClose></div>
            <ProfileBody step={selectedMember?.id ?? 'profile'}>
                <div className="native-chat-details-person"><ChatAvatar contact={displayed} /><div className="min-w-0"><p className="break-words text-[14px] font-semibold">{displayed.name}</p><p className="mt-1 text-[11px] text-text-tertiary">{selectedMember ? contact.name : contact.type === 'group' ? '群聊' : 'QQ 用户'}</p></div></div>
                <button type="button" className="native-chat-detail-action" onClick={() => void copy()} aria-label={`复制${displayed.type === 'group' ? '群号' : 'QQ 号'}`}><span className="text-text-tertiary">{displayed.type === 'group' ? '群号' : 'QQ'}</span><span className="ml-auto font-mono text-[11px]">{displayed.id}</span>{copyState === '已复制' ? <Check size={13} /> : <Copy size={13} />}</button>
                <dl className="native-chat-profile-fields">{(selectedMember ? memberFields : profile?.fields ?? []).map(field => <div key={field.label}><dt>{field.label}</dt><dd>{field.value}</dd></div>)}</dl>
                {!selectedMember && loading && !profile && <p className="native-chat-profile-status" role="status">正在读取资料…</p>}
                {!selectedMember && error && <p className="native-chat-profile-status" role="status">{error}<button type="button" onClick={() => setAttempt(value => value + 1)}>重试</button></p>}
                {selectedMember ? onMessage && <button type="button" className="native-chat-detail-action" onClick={() => leave(() => onMessage(selectedMember))}><MessageCircle size={14} />发消息<ChevronRight size={13} className="ml-auto" /></button> : <>
                    <button type="button" className="native-chat-detail-action" aria-pressed={contact.pinned} onClick={onPin}><Pin size={14} />置顶会话{contact.pinned && <Check size={14} className="ml-auto text-brand" />}</button>
                    {onSearch && <button type="button" className="native-chat-detail-action" onClick={() => leave(onSearch)}><Search size={14} />查找聊天记录<ChevronRight size={13} className="ml-auto" /></button>}
                    {contact.type === 'group' && target && <section className="native-chat-detail-members" aria-label="群成员">
                        <div className="native-chat-members-heading"><h4>群成员</h4><span>{members.length || contact.members || ''}</span></div>
                        <label className="native-chat-search"><Search size={14} aria-hidden /><input aria-label="搜索群成员" placeholder="搜索成员" value={query} onChange={event => { setQuery(event.target.value); setLimit(60); }} />{query && <button type="button" aria-label="清除成员搜索" onClick={() => { setQuery(''); setLimit(60); }}><X size={13} /></button>}</label>
                        <div className="native-chat-member-list" role="region" aria-label="群成员列表" tabIndex={0}>
                        {membersLoading && <p className="native-chat-profile-status" role="status">正在读取成员…</p>}
                        {memberError && <p className="native-chat-profile-status" role="status">{memberError}<button type="button" aria-label="重新读取成员" onClick={() => setAttempt(value => value + 1)}>重试</button></p>}
                        {!membersLoading && !memberError && !filtered.length && <p className="native-chat-profile-status">{query ? '没有找到成员' : '暂无成员'}</p>}
                        {filtered.slice(0, limit).map(member => <button type="button" key={member.id} className="native-chat-detail-member" aria-label={`查看${member.name}的群资料`} onClick={() => { setSelectedMember(member); setCopyState(''); }}><ChatAvatar contact={member} small /><span className="min-w-0 flex-1"><span className="block truncate">{member.name}</span><span className="native-chat-member-id">{member.id}</span></span>{member.role !== 'member' && ['owner', 'admin'].includes(member.role) && <span className="native-chat-member-role">{role(member)}</span>}<ChevronRight size={12} /></button>)}
                        {filtered.length > limit && <button type="button" className="native-chat-text-button" onClick={() => setLimit(value => value + 60)}>更多成员</button>}
                        </div>
                    </section>}
                </>}
            </ProfileBody>
            {copyState && <p role="status" className="pt-2 text-[11px] text-text-tertiary">{copyState}</p>}
        </PopoverContent>
    </Popover>;
}

function ProfileBody({ step, children }: { step: string; children: ReactNode }) {
    const scroll = useRef<HTMLDivElement>(null);
    const content = useRef<HTMLDivElement>(null);
    const [height, setHeight] = useState<number>();
    const motion = useMotion();
    useLayoutEffect(() => {
        const element = content.current;
        if (!element) return;
        const measure = () => {
            const ceiling = scroll.current ? parseFloat(getComputedStyle(scroll.current).maxHeight) : Infinity;
            setHeight(Math.min(element.scrollHeight, Number.isFinite(ceiling) ? ceiling : Infinity));
        };
        measure();
        const observer = new ResizeObserver(measure); observer.observe(element);
        return () => observer.disconnect();
    }, [step]);
    useLayoutEffect(() => { if (scroll.current) scroll.current.scrollTop = 0; }, [step]);
    return <div ref={scroll} className="native-chat-details-body" data-motion={motion.enabled} style={{ height }}>
        <div ref={content} key={step} className="native-chat-details-content">{children}</div>
    </div>;
}
