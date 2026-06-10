// 桌面端会话日志（设置页）等级筛选，对齐 legacy LogLevel 名。

export type DesktopLogLevelFilterValue =
    | 'ALL_'
    | 'CRIT'
    | 'EROR'
    | 'WARN'
    | 'INFO'
    | 'DBUG'
    | 'TRCE';

export const DESKTOP_LOG_LEVEL_OPTIONS: { value: DesktopLogLevelFilterValue; label: string }[] =
    [
        { value: 'ALL_', label: '全部等级' },
        { value: 'CRIT', label: '严重' },
        { value: 'EROR', label: '错误' },
        { value: 'WARN', label: '警告' },
        { value: 'INFO', label: '信息' },
        { value: 'DBUG', label: '调试' },
        { value: 'TRCE', label: '跟踪' },
    ];

export function desktopLevelToIpcFilter(
    value: DesktopLogLevelFilterValue,
): { level?: string } | undefined {
    if (value === 'ALL_') return undefined;
    return { level: value };
}