// AstrBot 各 Tab 共用的展示件。跨框架通用的在 ../entityParts，这里只留认 AstrBot 页名的跳转链接。

import type { ReactNode } from 'react';
import { ArrowRight } from 'lucide-react';
import { cn } from '../../../../shared/utils/cn';
import { ASTRBOT_TAB_LABEL } from './astrbotNav';

export { ConfirmDelete, EmptyHint, EntityRow, FormDialog, PickList, Pill, SubHeader } from '../entityParts';

/** 「去『模型』页」这类内联跳转。空态 / 占位文字里的死胡同都换成它。 */
export const JumpLink: React.FC<{
    tab: string;
    onGo: (tab: string) => void;
    children?: ReactNode;
    className?: string;
}> = ({ tab, onGo, children, className }) => (
    <button
        type="button"
        onClick={() => onGo(tab)}
        className={cn(
            'inline-flex items-center gap-0.5 rounded-xs text-brand underline-offset-2 hover:underline',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
            className,
        )}
    >
        {children ?? `去「${ASTRBOT_TAB_LABEL[tab] ?? tab}」页`}
        <ArrowRight size={11} strokeWidth={2.2} />
    </button>
);
