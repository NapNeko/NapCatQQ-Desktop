// @ 段：全体成员是纯文字高亮，@个人带头像。

import { cn } from '../../utils/cn';
import { useChatView } from '../chatContext';
import { ChatAvatar } from '../ChatAvatar';

export function AtSeg({ qq, name, mine }: { qq: string; name: string; mine: boolean }) {
    const { nameOf } = useChatView();
    if (qq === 'all')
        return (
            <span className={cn('font-medium', mine ? 'text-brand' : 'text-info')}>@全体成员 </span>
        );
    const shown = name || nameOf(Number(qq)) || qq;
    return (
        <span
            title={qq}
            className={cn(
                'inline-flex items-center gap-1 align-middle font-medium',
                mine ? 'text-brand' : 'text-info',
            )}
        >
            <ChatAvatar contact={{ type: 'private', id: qq, name: shown }} inline />
            <span>@{shown} </span>
        </span>
    );
}
