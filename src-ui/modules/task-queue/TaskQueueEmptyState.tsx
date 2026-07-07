// 任务队列空状态。

import React from 'react';
import { ListTodo } from 'lucide-react';
import { Button } from '../../shared/ui';
import { PagePlaceholder } from '../../shared/ui/PagePlaceholder';
import { MotionIcon } from '../../shared/ui/motion';
import type { AppRoute } from '../../shared/components/next/Sidebar';
import { TASK_KIND_VISUAL } from './taskQueueKindVisual';

export interface TaskQueueEmptyStateProps {
    variant: 'no-tasks' | 'no-filter-match';
    onNavigate?: (route: AppRoute) => void;
    showDocker?: boolean;
}

export const TaskQueueEmptyState: React.FC<TaskQueueEmptyStateProps> = ({
    variant,
    onNavigate,
    showDocker = true,
}) => {
    const isGlobal = variant === 'no-tasks';

    return (
        <PagePlaceholder className="gap-5">
            <MotionIcon
                icon={ListTodo}
                motion="none"
                playEnter={false}
                size={28}
                strokeWidth={1.5}
                className="text-text-tertiary"
                aria-hidden
            />
            <div className="max-w-md space-y-2">
                <p className="font-display text-[15px] font-semibold text-text">
                    {isGlobal ? '还没有任务记录' : '当前筛选下为空'}
                </p>
                <p className="text-[13px] leading-relaxed text-text-secondary">
                    {isGlobal
                        ? '在组件页安装或更新 QQ、NapCat 等；在远端安装 Docker；或通过 Docker 部署框架后，进度会集中显示在这里。'
                        : '试试切换上方的「全部」或「进行中」查看其它任务。'}
                </p>
            </div>
            {isGlobal && onNavigate && (
                <div className="flex flex-wrap items-center justify-center gap-2">
                    <Button size="sm" variant="secondary" onClick={() => onNavigate('components')}>
                        <EmptyNavIcon kind="component_action" />
                        前往组件
                    </Button>
                    {showDocker && (
                        <Button size="sm" variant="ghost" onClick={() => onNavigate('docker')}>
                            <EmptyNavIcon kind="docker_deploy" />
                            容器管理
                        </Button>
                    )}
                </div>
            )}
        </PagePlaceholder>
    );
};

function EmptyNavIcon({ kind }: { kind: keyof typeof TASK_KIND_VISUAL }) {
    const v = TASK_KIND_VISUAL[kind];
    return (
        <MotionIcon
            icon={v.Icon}
            motion="none"
            playEnter={false}
            size={14}
            strokeWidth={2}
            className={`mr-1.5 ${v.glyphSelected}`}
        />
    );
}
