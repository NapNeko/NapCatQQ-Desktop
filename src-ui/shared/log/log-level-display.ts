// Bot / Desktop 日志行共用的等级色条与标签（暖色主题）。

import type { LogLevel } from '../../core/domain/events/log-buffer';

export const LOG_LEVEL_SHORT: Record<LogLevel, string> = {
    trace: 'TRC',
    debug: 'DBG',
    info: 'INF',
    success: 'OK',
    warn: 'WRN',
    error: 'ERR',
    fatal: 'FTL',
    unknown: '·',
};

export function levelBarColor(level: LogLevel): string {
    switch (level) {
        case 'fatal':
        case 'error':
            return 'var(--state-danger)';
        case 'warn':
            return 'var(--state-warning)';
        case 'success':
            return 'var(--state-success)';
        case 'info':
            return 'var(--state-info)';
        case 'debug':
        case 'trace':
            return 'var(--neutral-300, #d1c4b6)';
        default:
            return 'transparent';
    }
}

export function levelLabelColor(level: LogLevel): string {
    switch (level) {
        case 'fatal':
        case 'error':
            return 'var(--state-danger)';
        case 'warn':
            return 'var(--state-warning)';
        case 'success':
            return 'var(--state-success)';
        case 'info':
            return 'var(--state-info)';
        case 'debug':
        case 'trace':
            return 'var(--text-tertiary)';
        default:
            return 'var(--text-disabled)';
    }
}

export function lineTextColor(level: LogLevel): string {
    switch (level) {
        case 'fatal':
        case 'error':
            return 'var(--state-danger)';
        case 'warn':
            return 'var(--text-primary)';
        case 'success':
            return 'var(--state-success)';
        case 'info':
        case 'debug':
        case 'trace':
            return 'var(--text-secondary)';
        default:
            return 'var(--text-secondary)';
    }
}