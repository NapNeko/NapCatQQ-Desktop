// 群头像只打开成员菜单，阻止右键冒泡到消息菜单。
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { AtSign, Hand, MessageCircle, UserMinus, UserRound, Volume2, VolumeX } from 'lucide-react';
import type { Contact, Message } from '../../core/domain/chat/model';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { ChatAccountStore } from '../../hooks/chat/chatStore';
import { useChatNotice } from '../../hooks/chat/useChatNotice';
import type { ProfileMember } from '../../core/services/chat-profile.service';
import { groupMemberPermissions } from '../../core/services/group-member-permissions.service';
import { useGroupMemberPermission } from '../../hooks/chat/useGroupMemberPermission';
import { errorText } from '../../core/domain/errors';
import {
    ContextMenu,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuSeparator,
    ContextMenuTrigger,
} from '../../shared/ui/ContextMenu';
import { ChatAvatarMenu } from './ChatAvatarMenu';
import { draftWithMention } from './messageActions';
import {
    ChatMemberActions,
    canManageGroupMember,
    type GroupMemberAction,
    type GroupMemberResult,
} from './ChatMemberActions';

export function ChatGroupMemberMenu({
    target,
    contact,
    message,
    store,
    onMessage,
    onViewMember,
    onFocusComposer,
    children,
}: {
    target: DebugTarget;
    contact: Contact;
    message: Message;
    store: ChatAccountStore;
    onMessage: (contact: Contact) => void;
    onViewMember: (memberId: string) => void;
    onFocusComposer?: () => void;
    children: ReactNode;
}) {
    const [open, setOpen] = useState(false);
    const [actionMember, setActionMember] = useState<ProfileMember>();
    const [action, setAction] = useState<GroupMemberAction | null>(null);
    const [result, setResult] = useState<GroupMemberResult>();
    const mounted = useRef(true);
    const identity = `${target.bot_id}/${target.qq_id}/${contact.key}/${message.senderId}`;
    const currentIdentity = useRef(identity);
    currentIdentity.current = identity;
    const connected =
        target.running &&
        target.online !== false &&
        store.getSnapshot().connection.state === 'connected';
    const eligible = connected && contact.type === 'group' && !message.mine && !!message.senderId;
    const selfPermission = useGroupMemberPermission(
        target,
        contact.id,
        String(target.qq_id),
        eligible,
    );
    const memberPermission = useGroupMemberPermission(
        target,
        contact.id,
        message.senderId,
        eligible,
    );
    const selfRole = selfPermission?.member?.role;
    const member = memberPermission?.member;
    const prepare = () => {
        if (eligible) void groupMemberPermissions.warm(target, contact.id, message.senderId);
    };
    const person: Contact = {
        key: `private:${message.senderId}`,
        type: 'private',
        id: message.senderId,
        name: message.senderName || message.senderId,
    };
    useChatNotice(
        `${identity}:member-menu`,
        `${contact.name} · 成员操作`,
        result?.message,
        undefined,
        result?.tone,
    );
    useEffect(() => {
        mounted.current = true;
        setAction(null);
        setActionMember(undefined);
        setResult(undefined);
        setOpen(false);
        return () => {
            mounted.current = false;
        };
    }, [identity]);
    useEffect(() => {
        if (!connected && contact.type === 'group') {
            groupMemberPermissions.clear(target, contact.id);
            return;
        }
        prepare();
    }, [open, connected, identity, target.backend]);
    if (contact.type !== 'group' || message.mine || !message.senderId)
        return (
            <ChatAvatarMenu
                store={store}
                contact={contact}
                message={message}
                onFocusComposer={onFocusComposer}
                onError={(error) => setResult({ message: error, tone: 'danger' })}
            >
                {children}
            </ChatAvatarMenu>
        );
    const manageable =
        member && canManageGroupMember(String(target.qq_id), selfRole, member) && connected;
    const readingPermission =
        !manageable &&
        (selfPermission?.status === 'loading' ||
            (['owner', 'admin'].includes(selfRole ?? '') &&
                memberPermission?.status === 'loading'));
    const manage = (next: GroupMemberAction) => {
        setResult(undefined);
        setActionMember(member);
        setAction(next);
    };
    const mention = () => {
        const account = store.getSnapshot().account;
        store.draft(
            contact.key,
            draftWithMention(account.messages, account.drafts, contact.key, message),
        );
        onFocusComposer?.();
    };
    return (
        <>
            <ContextMenu onOpenChange={setOpen}>
                <ContextMenuTrigger
                    onPointerEnter={prepare}
                    onFocusCapture={prepare}
                    onContextMenu={(event) => {
                        event.stopPropagation();
                        prepare();
                    }}
                >
                    {children}
                </ContextMenuTrigger>
                <ContextMenuContent aria-label="成员操作">
                    <ContextMenuItem onSelect={() => onViewMember(message.senderId)}>
                        <UserRound size={14} />
                        查看群名片
                    </ContextMenuItem>
                    <ContextMenuItem onSelect={() => onMessage(member ?? person)}>
                        <MessageCircle size={14} />
                        发起私聊
                    </ContextMenuItem>
                    <ContextMenuSeparator />
                    <ContextMenuItem onSelect={mention}>
                        <AtSign size={14} />
                        提及 {message.senderName}
                    </ContextMenuItem>
                    <ContextMenuItem
                        disabled={!connected}
                        onSelect={() => {
                            void store.poke(contact.key, message.senderId).catch((error) => {
                                if (mounted.current && currentIdentity.current === identity)
                                    setResult({ message: errorText(error), tone: 'danger' });
                            });
                        }}
                    >
                        <Hand size={14} />
                        拍一拍 {message.senderName}
                    </ContextMenuItem>
                    {manageable && (
                        <>
                            <ContextMenuSeparator />
                            <ContextMenuItem onSelect={() => manage('ban')}>
                                <VolumeX size={14} />
                                禁言
                            </ContextMenuItem>
                            <ContextMenuItem onSelect={() => manage('unban')}>
                                <Volume2 size={14} />
                                解除禁言
                            </ContextMenuItem>
                            <ContextMenuItem tone="danger" onSelect={() => manage('kick')}>
                                <UserMinus size={14} />
                                移出群
                            </ContextMenuItem>
                        </>
                    )}
                    {readingPermission && (
                        <>
                            <ContextMenuSeparator />
                            <ContextMenuItem disabled className="justify-center text-text-tertiary">
                                读取权限…
                            </ContextMenuItem>
                        </>
                    )}
                </ContextMenuContent>
            </ContextMenu>
            {actionMember && (
                <ChatMemberActions
                    target={target}
                    contact={contact}
                    member={actionMember}
                    action={action}
                    connected={connected}
                    onClose={() => setAction(null)}
                    onResult={setResult}
                    onApplied={() =>
                        groupMemberPermissions.invalidate(target, contact.id, actionMember.id)
                    }
                />
            )}
        </>
    );
}
