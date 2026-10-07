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
import type { ProfileMember } from '../../core/domain/chat/profile';
import { useChatGroupMemberDetail, useChatGroupMembers } from '../../hooks/chat/useChatProfile';
import {
    clearGroupMemberPermissions,
    useGroupMemberPermission,
} from '../../hooks/chat/useChatGroupPermissions';
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
    // 列表数据由 react-query 持有;kick 要立即从界面剔除而服务器最终一致有延迟,
    // 所以镜像一份本地数组,下次 refetch 到达时覆盖回服务器数据(与原实现同构)。
    const [members, setMembers] = useState<ProfileMember[]>([]);
    const selfPermission = useGroupMemberPermission(
        target,
        contact.id,
        String(target.qq_id),
        open && connected,
    );
    const self = selfPermission?.member;
    const [selectedId, setSelectedId] = useState<string>();
    const [query, setQuery] = useState('');
    const memberList = useChatGroupMembers(
        target,
        contact.id,
        open && connected && contact.type === 'group',
    );
    const memberDetail = useChatGroupMemberDetail(
        target,
        contact.id,
        selectedId,
        open && connected && contact.type === 'group',
    );
    // react-query 在重取期间保留上次 error,原实现每次取前/关闭时清空,这里同构地隐藏;
    // 断连或未打开时不发起请求,同样不展示上一轮的失败。
    const readError =
        open && connected && memberList.error && !memberList.isFetching
            ? errorText(memberList.error)
            : '';
    const detailError =
        open && connected && memberDetail.error && !memberDetail.isFetching
            ? errorText(memberDetail.error)
            : '';
    const detail = memberDetail.member;
    const loading = memberList.isFetching;
    const detailLoading = memberDetail.isFetching;
    const [copyState, setCopyState] = useState('');
    const [action, setAction] = useState<GroupMemberAction | null>(null);
    const [result, setResult] = useState<GroupMemberResult>();
    useChatNotice(
        `${identity}:members`,
        `${contact.name} · 成员读取失败`,
        readError,
        () => void memberList.refresh(),
    );
    useChatNotice(
        `${identity}:member-profile`,
        `${contact.name} · 名片读取失败`,
        detailError,
        () => void memberDetail.refresh(),
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
        setQuery('');
        setResult(undefined);
        setAction(null);
    }, [identity]);
    useEffect(() => {
        if (!open) {
            setAction(null);
            setQuery('');
            return;
        }
        // 断连时清权限缓存条目;列表/详情的重取由 react-query 的 enabled 门控。
        if (contact.type === 'group' && !connected) clearGroupMemberPermissions(target, contact.id);
    }, [open, identity, target.backend, connected]);
    useEffect(() => {
        const list = memberList.members;
        if (!list) return;
        setMembers(list);
        setSelectedId((previous) => {
            const requested = initialMemberId || previous;
            return list.some((entry) => entry.id === requested) ? requested : undefined;
        });
    }, [memberList.members, initialMemberId]);
    useEffect(() => {
        setCopyState('');
    }, [open, identity, selectedId, target.backend, connected]);
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
                        onClick={() => void memberList.refresh()}
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
                                onClick={() => void memberList.refresh()}
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
                            // 名片重取由 useChatMemberModeration 失效详情查询触发,这里只管 kick 的列表剔除。
                            if (applied !== 'kick') return;
                            setMembers((values) =>
                                values.filter((member) => member.id !== selected.id),
                            );
                            setSelectedId(undefined);
                        }}
                    />
                )}
            </DialogContent>
        </Dialog>
    );
}
