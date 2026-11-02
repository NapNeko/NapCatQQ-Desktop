import { mergeRecentConversations } from '../../../core/domain/chat/archive';
import {
    parseContact,
    parseFriendCategories,
    record,
    id,
    text,
    type Contact,
    type SessionKey,
} from '../../../core/domain/chat/model';
import { errorText } from '../../../core/domain/errors';
import { dataOf } from './chatProtocol';
import type { ChatAccountState } from './ChatAccountState';

export class ChatDirectory {
    private contactsRequest: Promise<void> | null = null;
    private recentRequest: Promise<void> | null = null;
    private recentEpoch = -1;
    constructor(private state: ChatAccountState) {}
    interrupt() {
        this.contactsRequest = null;
        this.recentRequest = null;
    }
    loadRecent(retry = false): Promise<void> {
        if (this.recentRequest) return this.recentRequest;
        if (
            this.state.snapshot.connection.state !== 'connected' ||
            (!retry && this.recentEpoch === this.state.session.epoch)
        )
            return Promise.resolve();
        const epoch = this.state.session.epoch;
        this.state.update({ recentLoading: true, recentError: '' });
        const request = this.state.session.transport
            .call(this.state.session.target.bot_id, 'get_recent_contact', { count: 100 })
            .then(dataOf)
            .then((data) => {
                if (epoch !== this.state.session.epoch) return;
                if (!Array.isArray(data)) throw new Error('此协议未返回最近会话列表');
                const merged = mergeRecentConversations(this.state.snapshot.account, data);
                const account = { ...merged, conversations: { ...merged.conversations } };
                for (const contact of this.state.snapshot.contacts)
                    if (account.conversations[contact.key])
                        account.conversations[contact.key] = {
                            ...account.conversations[contact.key],
                            ...contact,
                        };
                this.state.account(account);
                this.recentEpoch = epoch;
            })
            .catch((error) => {
                if (epoch === this.state.session.epoch)
                    this.state.update({ recentError: `最近会话同步失败：${errorText(error)}` });
            })
            .finally(() => {
                if (this.recentRequest === request) {
                    this.recentRequest = null;
                    this.state.update({ recentLoading: false });
                }
            });
        this.recentRequest = request;
        return request;
    }
    loadContacts(): Promise<void> {
        if (this.contactsRequest) return this.contactsRequest;
        const epoch = this.state.session.epoch;
        this.state.update({ contactsLoading: true, error: '' });
        const request = (async () => {
            const friends = async (): Promise<Contact[]> => {
                try {
                    const grouped = parseFriendCategories(
                        dataOf(
                            await this.state.session.transport.call(
                                this.state.session.target.bot_id,
                                'get_friends_with_category',
                                {},
                            ),
                        ),
                    );
                    if (grouped) return grouped;
                } catch {
                    // 旧协议端可能没有分组接口；普通好友列表仍可用。
                }
                if (epoch !== this.state.session.epoch) return [];
                const data = dataOf(
                    await this.state.session.transport.call(
                        this.state.session.target.bot_id,
                        'get_friend_list',
                        {},
                    ),
                );
                if (!Array.isArray(data)) throw new Error('好友列表返回格式无法识别');
                return data.flatMap((row) => {
                    const contact = parseContact(row, 'private');
                    return contact ? [contact] : [];
                });
            };
            const results = await Promise.allSettled([
                this.state.session.transport
                    .call(this.state.session.target.bot_id, 'get_group_list', {})
                    .then(dataOf)
                    .then((data) => {
                        if (!Array.isArray(data)) throw new Error('群聊列表返回格式无法识别');
                        return data.flatMap((row) => {
                            const contact = parseContact(row, 'group');
                            return contact ? [contact] : [];
                        });
                    }),
                friends(),
            ]);
            if (epoch !== this.state.session.epoch) return;
            const contacts: Contact[] = [];
            const errors: string[] = [];
            results.forEach((result, index) => {
                const type = index === 0 ? 'group' : 'private';
                if (result.status === 'rejected') {
                    errors.push(errorText(result.reason));
                    contacts.push(...this.state.snapshot.contacts.filter((c) => c.type === type));
                } else contacts.push(...result.value);
            });
            const account = this.state.snapshot.account;
            const conversations = { ...account.conversations };
            for (const contact of contacts)
                if (conversations[contact.key])
                    conversations[contact.key] = { ...conversations[contact.key], ...contact };
            this.state.update({
                contacts,
                account: { ...account, conversations },
                error: errors.join('；'),
            });
        })().finally(() => {
            if (this.contactsRequest === request) {
                this.contactsRequest = null;
                this.state.update({ contactsLoading: false });
            }
        });
        this.contactsRequest = request;
        return request;
    }
    async members(key: SessionKey): Promise<{ id: string; name: string }[]> {
        const value = dataOf(
            await this.state.session.transport.call(
                this.state.session.target.bot_id,
                'get_group_member_list',
                {
                    group_id: this.state.session.peer(key),
                },
            ),
        );
        return Array.isArray(value)
            ? value
                  .map(record)
                  .map((row) => ({
                      id: id(row.user_id),
                      name: text(row.card) || text(row.nickname) || id(row.user_id),
                  }))
                  .filter((row) => row.id)
            : [];
    }
}
