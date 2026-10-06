// 群成员头像右键菜单：提及 / 拍一拍。自己的消息不接管头像右键，留给消息菜单。
import type { ReactNode } from 'react';
import { AtSign, Hand } from 'lucide-react';
import type { Contact, Message } from '../../core/domain/chat/model';
import type { ChatAccountStore } from '../../hooks/chat/chatStore';
import {
    ContextMenu,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuTrigger,
} from '../../shared/ui/ContextMenu';
import { draftWithMention } from './messageActions';
import { errorText } from '../../core/domain/errors';

export function ChatAvatarMenu({
    store,
    contact,
    message,
    onFocusComposer,
    onError,
    children,
}: {
    store: ChatAccountStore;
    contact: Contact;
    message: Message;
    onFocusComposer?: () => void;
    onError: (error: string) => void;
    children: ReactNode;
}) {
    if (message.mine || !message.senderId) return <>{children}</>;
    const mention = () => {
        const snapshot = store.getSnapshot();
        store.draft(
            contact.key,
            draftWithMention(
                snapshot.account.messages,
                snapshot.account.drafts,
                contact.key,
                message,
            ),
        );
        onFocusComposer?.();
    };
    const pokeDisabled =
        !store.target.running ||
        store.target.online === false ||
        store.getSnapshot().connection.state !== 'connected';
    const poke = () => {
        void store.poke(contact.key, message.senderId).catch((error) => onError(errorText(error)));
    };
    return (
        <ContextMenu>
            <ContextMenuTrigger onContextMenu={(event) => event.stopPropagation()}>
                {children}
            </ContextMenuTrigger>
            <ContextMenuContent aria-label="成员操作">
                {contact.type === 'group' && (
                    <ContextMenuItem onSelect={mention}>
                        <AtSign size={14} />
                        提及 {message.senderName}
                    </ContextMenuItem>
                )}
                <ContextMenuItem disabled={pokeDisabled} onSelect={poke}>
                    <Hand size={14} />
                    拍一拍 {message.senderName}
                </ContextMenuItem>
            </ContextMenuContent>
        </ContextMenu>
    );
}
