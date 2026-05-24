// 把 DomainEvent.kind 映射成 EventPanel 用的中文 / Badge 颜色，纯函数。

import type { DomainEvent } from '../../ipc/types';
import type { BadgeColor } from '../bot/status';

export interface DomainEventDescriptor {
    /// 中文 Badge 文案。
    label: string;
    /// Fluent UI Badge 颜色。
    color: BadgeColor;
    /// 控制台一行展示文案。
    message: string;
}

export function describeEvent(event: DomainEvent): DomainEventDescriptor {
    switch (event.kind) {
        case 'bot_state_changed':
            return {
                label: '状态改变',
                color: 'brand',
                message: `Bot ${event.snapshot.bot_id} 状态转移至 ${event.snapshot.state}${event.reason ? `，原因: ${event.reason}` : ''
                    }`,
            };
        case 'bot_status_changed': {
            const rss = event.status.memory_rss_bytes;
            const rssText = rss ? `${Math.floor(rss / 1024 / 1024)}MB` : '无';
            return {
                label: '指标更新',
                color: 'success',
                message: `Bot ${event.status.bot_id} 运行时指标刷新 (RSS: ${rssText})`,
            };
        }
        case 'bot_log_appended':
            return {
                label: '日志流',
                color: 'informative',
                message: `[Log] Bot ${event.bot_id}: ${event.line}`,
            };
        case 'bot_error':
            return {
                label: '运行报错',
                color: 'danger',
                message: `Bot ${event.bot_id} 异常报错: ${event.message}${event.hint ? ` (排查建议: ${event.hint})` : ''
                    }`,
            };
        case 'task_progress':
            return {
                label: '任务进度',
                color: 'warning',
                message: `[Task] ${event.task_id} 进度: ${event.progress}% - ${event.message}`,
            };
        default:
            return {
                label: event.kind,
                color: 'informative',
                message: `收到底层 DomainEvent: ${JSON.stringify(event)}`,
            };
    }
}
