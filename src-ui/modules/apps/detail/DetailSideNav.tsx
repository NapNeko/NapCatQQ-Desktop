// 详情页左侧分组导航：框架给的分组 + 外壳追加的原始文件 / 日志（见 buildDetailNav）。
// 必须放在 Tabs 根里面，选中态和键盘上下切换都靠 Radix Tabs。

import { Fragment } from 'react';
import { TabsSideList, TabsSideTrigger, type TabsSideDot } from '../../../shared/ui';
import { cn } from '../../../shared/utils/cn';
import type { FrameworkNavGroup, NavBadgeTone, NavBadges } from './frameworkUi';

const DOT: Record<NavBadgeTone, TabsSideDot> = { next: 'brand', warn: 'warning', error: 'danger' };

export const DetailSideNav: React.FC<{
    groups: readonly FrameworkNavGroup[];
    badges: NavBadges;
}> = ({ groups, badges }) => (
    <div className="scrollbar-hide w-[8.5rem] shrink-0 overflow-y-auto border-r border-border-subtle py-3 pr-2">
        <TabsSideList aria-label="实例设置">
            {groups.map((g, i) => (
                <Fragment key={g.id}>
                    {g.label && (
                        <div
                            aria-hidden
                            className={cn(
                                'px-3 pb-1 text-2xs font-medium tracking-wider text-text-tertiary',
                                i > 0 && 'pt-3',
                            )}
                        >
                            {g.label}
                        </div>
                    )}
                    {g.items.map((t) => {
                        const tone = badges[t.value];
                        return (
                            <TabsSideTrigger
                                key={t.value}
                                value={t.value}
                                dot={tone ? DOT[tone] : undefined}
                            >
                                {t.label}
                            </TabsSideTrigger>
                        );
                    })}
                </Fragment>
            ))}
        </TabsSideList>
    </div>
);
