// 任务队列列表项：按 kind 的图标与底衬（只用语义 token，适配亮/暗/Catppuccin）。

import type { LucideIcon } from 'lucide-react';
import { Box, Container, Download } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import type { TaskQueueItem } from '../../core/domain/task-queue/types';

export type TaskKindVisual = {
    Icon: LucideIcon;
    /** 选中时底衬 */
    tileSelected: string;
    /** 未选中时底衬（略抬对比，避免 tertiary 图标糊在浅底上） */
    tileIdle: string;
    /** 选中时图标色 */
    glyphSelected: string;
    /** 未选中时图标色 */
    glyphIdle: string;
};

export const TASK_KIND_VISUAL: Record<TaskQueueItem['kind'], TaskKindVisual> = {
    component_action: {
        Icon: Box,
        tileSelected: 'bg-brand-soft ring-1 ring-inset ring-brand/25',
        tileIdle: 'bg-inset ring-1 ring-inset ring-border-subtle',
        glyphSelected: 'text-brand',
        glyphIdle: 'text-text-secondary',
    },
    docker_install: {
        Icon: Download,
        tileSelected: 'bg-info-soft ring-1 ring-inset ring-info/30',
        tileIdle: 'bg-inset ring-1 ring-inset ring-border-subtle',
        glyphSelected: 'text-info',
        glyphIdle: 'text-text-secondary',
    },
    docker_deploy: {
        Icon: Container,
        tileSelected: 'bg-warning-soft ring-1 ring-inset ring-warning/28',
        tileIdle: 'bg-inset ring-1 ring-inset ring-border-subtle',
        glyphSelected: 'text-warning',
        glyphIdle: 'text-text-secondary',
    },
};

export function taskKindIconClasses(
    kind: TaskQueueItem['kind'],
    selected: boolean,
): { tile: string; glyph: string } {
    const v = TASK_KIND_VISUAL[kind];
    return {
        tile: cn(v.tileIdle, selected && v.tileSelected),
        glyph: cn(selected ? v.glyphSelected : v.glyphIdle),
    };
}