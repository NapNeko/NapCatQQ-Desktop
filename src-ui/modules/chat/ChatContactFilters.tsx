import { useRef, type KeyboardEvent } from 'react';
import type { Contact } from '../../core/domain/chat/model';
import { Select, type SelectItem } from '../../shared/ui/Select';
import './contactFilters.css';

export const friendCategoryKey = (contact: Contact) =>
    contact.categoryId ? `qq:${contact.categoryId}` : 'uncategorized';

export function friendCategoryItems(contacts: readonly Contact[]): SelectItem[] {
    const friends = contacts.filter((contact) => contact.type === 'private');
    if (!friends.some((contact) => contact.categoryId)) return [];
    const categories = new Map<string, string>();
    for (const contact of friends)
        categories.set(
            friendCategoryKey(contact),
            contact.categoryName || (contact.categoryId ? `分组 ${contact.categoryId}` : '未分组'),
        );
    return [
        { value: 'all', label: '全部分组' },
        ...[...categories].map(([value, label]) => ({ value, label })),
    ];
}

export function ChatContactFilters({
    contacts,
    type,
    category,
    categories,
    onTypeChange,
    onCategoryChange,
}: {
    contacts: readonly Contact[];
    type: Contact['type'];
    category: string;
    categories: readonly SelectItem[];
    onTypeChange: (type: Contact['type']) => void;
    onCategoryChange: (category: string) => void;
}) {
    const buttons = useRef<Partial<Record<Contact['type'], HTMLButtonElement | null>>>({});
    const keyDown = (event: KeyboardEvent<HTMLButtonElement>, current: Contact['type']) => {
        if (event.altKey || event.ctrlKey || event.metaKey || event.nativeEvent.isComposing) return;
        const next =
            event.key === 'Home'
                ? 'private'
                : event.key === 'End'
                  ? 'group'
                  : event.key === 'ArrowLeft' || event.key === 'ArrowRight'
                    ? current === 'private'
                        ? 'group'
                        : 'private'
                    : null;
        if (!next) return;
        event.preventDefault();
        onTypeChange(next);
        buttons.current[next]?.focus();
    };
    return (
        <div className="native-chat-contact-filters">
            <div role="tablist" aria-label="联系人类型" className="native-chat-contact-types">
                {(['private', 'group'] as const).map((value) => (
                    <button
                        key={value}
                        ref={(node) => {
                            buttons.current[value] = node;
                        }}
                        type="button"
                        role="tab"
                        aria-selected={type === value}
                        tabIndex={type === value ? 0 : -1}
                        onClick={() => onTypeChange(value)}
                        onKeyDown={(event) => keyDown(event, value)}
                    >
                        {value === 'private' ? '好友' : '群聊'}
                        <span aria-hidden>
                            {contacts.filter((contact) => contact.type === value).length}
                        </span>
                    </button>
                ))}
            </div>
            {type === 'private' && categories.length > 0 && (
                <Select
                    id="chat-friend-category"
                    label={<span className="sr-only">好友分组</span>}
                    className="native-chat-contact-category"
                    value={category}
                    items={categories}
                    onValueChange={onCategoryChange}
                />
            )}
        </div>
    );
}
