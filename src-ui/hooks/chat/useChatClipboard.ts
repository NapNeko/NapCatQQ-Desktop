import { useCallback } from 'react';
import type { Contact, Message } from '../../core/domain/chat/model';
import { copyChatMessage } from '../../core/services/chat-clipboard.service';
import type { ChatAccountStore } from './chatStore';

export function useChatClipboard(store: ChatAccountStore) {
    return useCallback(
        (contact: Contact, message: Message) => copyChatMessage(store.target, contact, message),
        [store],
    );
}
