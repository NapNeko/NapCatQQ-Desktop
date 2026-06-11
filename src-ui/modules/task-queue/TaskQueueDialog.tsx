// 全局任务队列对话框。

import React, { useEffect, useState } from 'react';
import { ArrowLeft, ListTodo } from 'lucide-react';
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from '../../shared/ui';
import { MotionIcon } from '../../shared/ui/motion';
import type { TaskQueueItem } from '../../core/domain/task-queue/types';
import { TaskQueueListItem } from './TaskQueueListItem';
import { TaskDetailPanel } from './TaskDetailPanel';

export interface TaskQueueDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    items: TaskQueueItem[];
    onOpenSettingsLog: () => void;
}

export const TaskQueueDialog: React.FC<TaskQueueDialogProps> = ({
    open,
    onOpenChange,
    items,
    onOpenSettingsLog,
}) => {
    const [selectedId, setSelectedId] = useState<string | null>(null);

    useEffect(() => {
        if (!open) {
            setSelectedId(null);
            return;
        }
        if (selectedId && items.some((i) => i.id === selectedId)) return;
        const firstActive = items.find(
            (i) =>
                i.status === 'running' ||
                i.status === 'installing' ||
                i.status === 'pending' ||
                i.status === 'paused',
        );
        setSelectedId(firstActive?.id ?? items[0]?.id ?? null);
    }, [open, items, selectedId]);

    const selected = items.find((i) => i.id === selectedId) ?? null;

    const handleOpenSettingsLog = () => {
        onOpenChange(false);
        onOpenSettingsLog();
    };

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="taskQueue" className="flex h-full min-h-0 flex-1 flex-col gap-0 overflow-hidden p-0">
                <DialogHeader className="shrink-0 border-b border-border-subtle px-5 py-4">
                    <DialogTitle className="flex items-center gap-2 font-display text-lg">
                        <MotionIcon icon={ListTodo} motion="none" playEnter={false} size={18} />
                        任务队列
                    </DialogTitle>
                    <DialogDescription className="text-[12px]">
                        组件安装、Docker 安装与部署；日志来自本机 Desktop 会话。
                    </DialogDescription>
                </DialogHeader>

                {items.length === 0 ? (
                    <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 py-16 text-center">
                        <p className="text-[13px] text-text-secondary">当前没有任务记录</p>
                        <p className="max-w-sm text-[12px] text-text-tertiary">
                            在组件页安装组件、安装 Docker 或部署容器后，会出现在这里。
                        </p>
                    </div>
                ) : selected ? (
                    <div className="flex min-h-0 flex-1 flex-col overflow-hidden md:flex-row">
                        <aside className="flex max-h-[38%] min-h-0 shrink-0 flex-col gap-2 overflow-y-auto border-b border-border-subtle p-3 md:max-h-none md:h-full md:w-[34%] md:min-w-[280px] md:border-b-0 md:border-r">
                            {items.map((item) => (
                                <TaskQueueListItem
                                    key={item.id}
                                    item={item}
                                    selected={item.id === selectedId}
                                    onSelect={() => setSelectedId(item.id)}
                                />
                            ))}
                        </aside>
                        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden p-4 md:min-h-0">
                            <button
                                type="button"
                                className="mb-2 inline-flex items-center gap-1 text-[12px] text-text-secondary hover:text-text md:hidden"
                                onClick={() => setSelectedId(null)}
                            >
                                <ArrowLeft size={14} />
                                返回列表
                            </button>
                            <TaskDetailPanel
                                item={selected}
                                logEnabled={open}
                                onOpenSettingsLog={handleOpenSettingsLog}
                            />
                        </div>
                    </div>
                ) : (
                    <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto p-4 md:hidden">
                        {items.map((item) => (
                            <TaskQueueListItem
                                key={item.id}
                                item={item}
                                selected={false}
                                onSelect={() => setSelectedId(item.id)}
                            />
                        ))}
                    </div>
                )}
            </DialogContent>
        </Dialog>
    );
};