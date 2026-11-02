import type { chatService } from '../../../core/services/chat.service';
import type { chatArchiveService } from '../../../core/services/chat-archive.service';
import type { Account, Contact } from '../../../core/domain/chat/model';
import type { DebugReceiverState } from '../../../core/ipc/generated/debug/DebugReceiverState';
import type { DebugStreamProgress } from '../../../core/ipc/generated/debug/DebugStreamProgress';

export interface ChatSnapshot {
    account: Account;
    contacts: Contact[];
    sendProgress: Record<string, DebugStreamProgress>;
    connection: DebugReceiverState;
    error: string;
    contactsLoading: boolean;
    hydrated: boolean;
    archiveError: string;
    recentLoading: boolean;
    recentError: string;
    history: Record<string, { loading: boolean; loaded: boolean; done: boolean; error: string }>;
}
export type ChatTransport = Pick<typeof chatService, 'call' | 'subscribe' | 'unsubscribe'>;
export type ChatArchivePort = Pick<typeof chatArchiveService, 'load' | 'save'>;
