// 群资料与成员浏览分开，管理仅使用已确认的双方角色。
import { useEffect, useMemo, useState } from 'react';
import {
    Check,
    Copy,
    MessageCircle,
    RefreshCw,
    Search,
    Volume2,
    VolumeX,
    UserMinus,
    Users,
    X,
} from 'lucide-react';
import type { Contact } from '../../core/domain/chat/model';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { chatProfileService, type ProfileMember } from '../../core/services/chat-profile.service';
import { groupMemberPermissions } from '../../core/services/group-member-permissions.service';
import { useGroupMemberPermission } from '../../hooks/chat/useGroupMemberPermission';
import { errorText } from '../../core/domain/errors';
import { useChatNotice } from '../../hooks/chat/useChatNotice';
import { Button } from '../../shared/ui/Button';
import { Dialog, DialogContent, DialogTitle } from '../../shared/ui/Dialog';
import { ChatAvatar } from './ChatAvatar';
import { GroupMemberList } from './GroupMemberList';
import {
    ChatMemberActions,
    canManageGroupMember,
    type GroupMemberAction,
    type GroupMemberResult,
} from './ChatMemberActions';
import './group-members.css';

export function GroupMembersDialog({
    open,
    onOpenChange,
    target,
    contact,
    connected,
    onMessage,
    initialMemberId,
}: {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    target: DebugTarget;
    contact: Contact;
    connected: boolean;
    onMessage: (contact: Contact) => void;
    initialMemberId?: string;
}) {
    const identity = `${target.bot_id}/${target.qq_id}/${contact.key}`;
    const [members, setMembers] = useState<ProfileMember[]>([]);
    const selfPermission = useGroupMemberPermission(
        target,
        contact.id,
        String(target.qq_id),
        open && connected,
    );
    const self = selfPermission?.member;
    const [selectedId, setSelectedId] = useState<string>();
    const [detail, setDetail] = useState<ProfileMember>();
    const [query, setQuery] = useState('');
    const [attempt, setAttempt] = useState(0);
    const [detailAttempt, setDetailAttempt] = useState(0);
    const [loading, setLoading] = useState(false);
    const [detailLoading, setDetailLoading] = useState(false);
    const [readError, setReadError] = useState('');
    const [detailError, setDetailError] = useState('');
    const [copyState, setCopyState] = useState('');
    const [action, setAction] = useState<GroupMemberAction | null>(null);
    const [result, setResult] = useState<GroupMemberResult>();
    useChatNotice(`${identity}:members`, `${contact.name} · 成员读取失败`, readError, () =>
        setAttempt((value) => value + 1),
    );
    useChatNotice(`${identity}:member-profile`, `${contact.name} · 名片读取失败`, detailError, () =>
        setDetailAttempt((value) => value + 1),
    );
    useChatNotice(
        `${identity}:member-action`,
        `${contact.name} · 成员管理`,
        result?.message,
        undefined,
        result?.tone,
    );
    useEffect(() => {
        setMembers([]);
        setSelectedId(undefined);
        setDetail(undefined);
        setQuery('');
        setReadError('');
        setDetailError('');
        setResult(undefined);
        setAction(null);
    }, [identity]);
    useEffect(() => {
        if (!open) {
            setAction(null);
            setQuery('');
            setReadError('');
            setDetailError('');
            return;
        }
        if (contact.type !== 'group') return;
        if (!connected) {
            groupMemberPermissions.clear(target, contact.id);
            setLoading(false);
            return;
        }
        let cancelled = false;
        setLoading(true);
        setReadError('');
        void Promise.allSettled([
            chatProfileService.members(target, contact.id),
            groupMemberPermissions.self(target, contact.id),
        ]).then(([list]) => {
            if (cancelled) return;
            if (list.status === 'fulfilled') {
                setMembers(list.value);
                groupMemberPermissions.seed(target, contact.id, list.value);
                setSelectedId((previous) => {
                    const requested = initialMemberId || previous;
                    return list.value.some((member) => member.id === requested)
                        ? requested
                        : undefined;
                });
            } else setReadError(errorText(list.reason));
            setLoading(false);
        });
        return () => {
            cancelled = true;
        };
    }, [open, identity, target.backend, attempt, initialMemberId, connected]);
    useEffect(() => {
        setCopyState('');
        setDetail(undefined);
        setDetailError('');
        if (!open || !connected || !selectedId || contact.type !== 'group') return;
        let cancelled = false;
        setDetailLoading(true);
        void chatProfileService
            .member(target, contact.id, selectedId)
            .then(
                (member) => {
                    if (!cancelled) {
                        setDetail(member);
                        groupMemberPermissions.seed(target, contact.id, [member]);
                    }
                },
                (error) => {
                    if (!cancelled) setDetailError(errorText(error));
                },
            )
            .finally(() => {
                if (!cancelled) setDetailLoading(false);
            });
        return () => {
            cancelled = true;
        };
    }, [open, identity, selectedId, detailAttempt, target.backend, connected]);
    const term = query.trim().toLocaleLowerCase();
    const filtered = useMemo(
        () =>
            members.filter(
                (member) =>
                    !term ||
                    `${member.name} ${member.nickname} ${member.id}`
                        .toLocaleLowerCase()
                        .includes(term),
            ),
        [members, term],
    );
    const selected =
        detail?.id === selectedId ? detail : members.find((member) => member.id === selectedId);
    const manageable =
        connected &&
        !!detail &&
        detail.id === selectedId &&
        canManageGroupMember(String(target.qq_id), self?.role, detail);
    const role =
        selected?.role === 'owner'
            ? '群主'
            : selected?.role === 'admin'
              ? '管理员'
              : selected?.role === 'member'
                ? '群成员'
                : '';
    const fields = selected
        ? [
              { label: '昵称', value: selected.nickname },
              { label: '身份', value: role },
              { label: '头衔', value: selected.title },
              { label: '加入于', value: selected.joined },
              { label: '最近发言', value: selected.lastSent },
              {
                  label: '禁言至',
                  value:
                      selected.mutedUntil && selected.mutedUntil > Date.now() / 1000
                          ? new Date(selected.mutedUntil * 1000).toLocaleString('zh-CN')
                          : undefined,
              },
          ].filter((field) => field.value)
        : [];
    const copy = async () => {
        if (!selected) return;
        try {
            await navigator.clipboard.writeText(selected.id);
            setCopyState('已复制');
        } catch {
            setCopyState('复制失败');
        }
    };
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="lg" className="native-chat-group-members-dialog">
                <DialogTitle className="native-chat-group-members-title">
                    <Users size={18} />
                    群成员
                    <span>{contact.name}</span>
                </DialogTitle>
                <div className="native-chat-group-members-toolbar">
                    <label className="native-chat-search">
                        <Search size={15} aria-hidden />
                        <input
                            aria-label="搜索群成员"
                            placeholder="搜索名片、昵称或 QQ 号"
                            value={query}
                            onChange={(event) => setQuery(event.target.value)}
                        />
                        {query && (
                            <button
                                type="button"
                                aria-label="清除成员搜索"
                                onClick={() => setQuery('')}
                            >
                                <X size={13} />
                            </button>
                        )}
                    </label>
                    <span className="native-chat-group-members-count">{members.length} 位成员</span>
                    <Button
                        variant="ghost"
                        size="icon"
                        aria-label="刷新群成员"
                        title="刷新群成员"
                        disabled={loading || !connected}
                        onClick={() => setAttempt((value) => value + 1)}
                    >
                        <RefreshCw size={15} />
                    </Button>
                </div>
                <div className="native-chat-group-members-grid">
                    <GroupMemberList
                        key={`${identity}/${term}`}
                        members={filtered}
                        selectedId={selectedId}
                        onSelect={(member) => setSelectedId(member.id)}
                    >
                        {loading && !members.length && (
                            <p className="native-chat-profile-status" role="status">
                                正在读取成员…
                            </p>
                        )}
                        {readError && (
                            <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => setAttempt((value) => value + 1)}
                            >
                                重新读取成员
                            </Button>
                        )}
                        {!loading && !readError && !filtered.length && (
                            <p className="native-chat-profile-status">
                                {query ? '没有找到成员' : '暂无成员'}
                            </p>
                        )}
                    </GroupMemberList>
                    <section className="native-chat-group-member-card" aria-label="群成员名片">
                        {selected ? (
                            <>
                                <div className="native-chat-details-person">
                                    <ChatAvatar contact={selected} />
                                    <div className="min-w-0">
                                        <h3>{selected.name}</h3>
                                        <p>{contact.name}</p>
                                    </div>
                                </div>
                                <button
                                    type="button"
                                    className="native-chat-detail-action"
                                    aria-label="复制 QQ 号"
                                    onClick={() => void copy()}
                                >
                                    <span>QQ</span>
                                    <span className="ml-auto font-mono text-[11px]">
                                        {selected.id}
                                    </span>
                                    {copyState === '已复制' ? (
                                        <Check size={13} />
                                    ) : (
                                        <Copy size={13} />
                                    )}
                                </button>
                                <dl className="native-chat-profile-fields">
                                    {fields.map((field) => (
                                        <div key={field.label}>
                                            <dt>{field.label}</dt>
                                            <dd>{field.value}</dd>
                                        </div>
                                    ))}
                                </dl>
                                {detailLoading && (
                                    <p className="native-chat-profile-status" role="status">
                                        正在读取名片…
                                    </p>
                                )}
                                <Button
                                    variant="secondary"
                                    size="sm"
                                    className="native-chat-group-member-message"
                                    onClick={() => {
                                        onOpenChange(false);
                                        onMessage(selected);
                                    }}
                                >
                                    <MessageCircle size={14} />
                                    发消息
                                </Button>
                                {manageable && (
                                    <div
                                        className="native-chat-group-member-management"
                                        role="group"
                                        aria-label="管理群成员"
                                    >
                                        <Button
                                            variant="secondary"
                                            size="sm"
                                            onClick={() => {
                                                setResult(undefined);
                                                setAction('ban');
                                            }}
                                        >
                                            <VolumeX size={13} />
                                            禁言
                                        </Button>
                                        <Button
                                            variant="secondary"
                                            size="sm"
                                            onClick={() => {
                                                setResult(undefined);
                                                setAction('unban');
                                            }}
                                        >
                                            <Volume2 size={13} />
                                            解除禁言
                                        </Button>
                                        <Button
                                            variant="secondary"
                                            size="sm"
                                            className="native-chat-group-member-remove"
                                            onClick={() => {
                                                setResult(undefined);
                                                setAction('kick');
                                            }}
                                        >
                                            <UserMinus size={13} />
                                            移出群
                                        </Button>
                                    </div>
                                )}
                                {copyState && (
                                    <p className="native-chat-profile-status" role="status">
                                        {copyState}
                                    </p>
                                )}
                            </>
                        ) : (
                            <div className="native-chat-group-member-empty">
                                <Users size={28} strokeWidth={1.3} />
                                <span>选择一位群成员</span>
                            </div>
                        )}
                    </section>
                </div>
                {selected && (
                    <ChatMemberActions
                        key={`${identity}/${selected.id}`}
                        target={target}
                        contact={contact}
                        member={selected}
                        action={action}
                        connected={connected}
                        onClose={() => setAction(null)}
                        onResult={setResult}
                        onApplied={(applied) => {
                            if (applied === 'kick') {
                                setMembers((values) =>
                                    values.filter((member) => member.id !== selected.id),
                                );
                                setSelectedId(undefined);
                            } else setDetailAttempt((value) => value + 1);
                        }}
                    />
                )}
            </DialogContent>
        </Dialog>
    );
}
