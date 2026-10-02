// 头像与占位共用固定尺寸，加载失败不影响会话布局。
import { useState } from 'react';
import { Users } from 'lucide-react';
import type { Contact } from '../../core/domain/chat/model';
import { avatarUrl, initialOf } from '../../core/domain/debug/chatFormat';
import { cn } from '../../shared/utils/cn';

export function ChatAvatar({ contact, small = false }: { contact: Pick<Contact, 'name' | 'type'> & { id?: string }; small?: boolean }) {
    const url = contact.type === 'private' ? avatarUrl(Number(contact.id)) : null;
    const [failed, setFailed] = useState<string | null>(null);
    return <span aria-hidden className={cn('native-chat-avatar', small && 'is-small', contact.type === 'group' && 'is-group')}>
        {contact.type === 'group' ? <Users size={small ? 16 : 19} strokeWidth={1.65} /> : initialOf(contact.name)}
        {url && failed !== url && <img key={url} src={url} alt="" loading="lazy" decoding="async" referrerPolicy="no-referrer" draggable={false} onError={() => setFailed(url)} />}
    </span>;
}
