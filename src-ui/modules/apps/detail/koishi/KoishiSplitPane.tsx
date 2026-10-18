import type { ReactNode } from 'react';

/** 按正文实际宽度切换树与内容的排列，侧栏收窄后不再挤掉编辑区。 */
export function KoishiSplitPane({ children }: { children: ReactNode }) {
    return (
        <div className="@container/koishi flex min-h-0 min-w-0 flex-1">
            <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-4 pb-3 @min-[44rem]/koishi:flex-row">
                {children}
            </div>
        </div>
    );
}
