// 成员列表复用虚拟滚动，搜索对完整列表生效。
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { ChevronRight } from 'lucide-react';
import type { ProfileMember } from '../../core/domain/chat/profile';
import { ChatAvatar } from './ChatAvatar';

export function GroupMemberList({
    members,
    selectedId,
    onSelect,
    children,
}: {
    members: ProfileMember[];
    selectedId?: string;
    onSelect: (member: ProfileMember) => void;
    children?: ReactNode;
}) {
    const scroll = useRef<HTMLDivElement>(null);
    const [limit, setLimit] = useState(60);
    const count = Math.min(limit, members.length);
    const virtual = useVirtualizer({
        count,
        getScrollElement: () => scroll.current,
        getItemKey: (index) => members[index].id,
        estimateSize: () => 48,
        overscan: 4,
        initialRect: { width: 300, height: 240 },
    });
    const rows = virtual.getVirtualItems();
    const lastIndex = rows.at(-1)?.index ?? 0;
    useEffect(() => {
        if (count < members.length && lastIndex >= count - 5)
            setLimit(Math.min(count + 60, members.length));
    }, [lastIndex, count, members.length]);
    return (
        <div
            ref={scroll}
            className="native-chat-member-list"
            role="region"
            aria-label="群成员列表"
            tabIndex={0}
        >
            {children}
            <div style={{ height: virtual.getTotalSize(), position: 'relative' }}>
                {rows.map((row) => {
                    const member = members[row.index];
                    return (
                        <button
                            type="button"
                            key={row.key}
                            className="native-chat-detail-member"
                            style={{ position: 'absolute', top: row.start, height: row.size }}
                            aria-label={`查看${member.name}的群资料`}
                            aria-current={selectedId === member.id ? 'true' : undefined}
                            onClick={() => onSelect(member)}
                        >
                            <ChatAvatar contact={member} small />
                            <span className="min-w-0 flex-1">
                                <span className="block truncate">{member.name}</span>
                                <span className="native-chat-member-id">{member.id}</span>
                            </span>
                            {['owner', 'admin'].includes(member.role) && (
                                <span className="native-chat-member-role">
                                    {member.role === 'owner' ? '群主' : '管理员'}
                                </span>
                            )}
                            <ChevronRight size={12} />
                        </button>
                    );
                })}
            </div>
        </div>
    );
}
