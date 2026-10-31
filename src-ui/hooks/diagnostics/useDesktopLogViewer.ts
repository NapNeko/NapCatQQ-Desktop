// 设置页日志 Tab：数据拉取 + 筛选/字号等 UI 状态（供内容区与顶栏操作共用）。

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { DesktopLogLevelFilterValue } from '../../core/domain/desktop-log';
import { filterLogs, serializeDesktopLogs } from '../../core/domain/events/log-buffer';
import { desktopLogService } from '../../core/services/desktop.service';
import { useDesktopLogStream } from './useDesktopLogStream';
import { pushErrorBar } from '../ui/pushErrorBar';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { errorText } from '../../core/domain/errors';
import { localOs } from '../../core/domain/runtime/env';

const MIN_FONT = 10;
const MAX_FONT = 18;
const DEFAULT_FONT = 12;

export function useDesktopLogViewer(enabled: boolean) {
    const [level, setLevel] = useState<DesktopLogLevelFilterValue>('ALL_');
    const { logs, loading, error, reload } = useDesktopLogStream(level, enabled);
    const [query, setQuery] = useState('');
    const [autoScroll, setAutoScroll] = useState(true);
    const [fontSize, setFontSize] = useState(DEFAULT_FONT);
    const [opening, setOpening] = useState(false);
    const [copyingLocation, setCopyingLocation] = useState(false);
    const viewportRef = useRef<HTMLDivElement>(null);

    const filtered = useMemo(() => filterLogs(logs, query, 'all', 'all'), [logs, query]);

    useEffect(() => {
        if (!autoScroll || !enabled) return;
        const el = viewportRef.current;
        if (el) el.scrollTop = el.scrollHeight;
    }, [filtered.length, autoScroll, enabled]);

    const onOpenLocation = useCallback(async () => {
        setOpening(true);
        try {
            await desktopLogService.openLogLocation();
        } catch (err) {
            if (localOs() === 'linux')
                pushErrorBar({
                    title: '打开日志目录失败',
                    raw: errorText(err),
                    content: '可点击“复制目录”，在文件管理器中打开。',
                });
            else console.warn('打开日志位置失败:', err);
        } finally {
            setOpening(false);
        }
    }, []);

    const onCopyLocation = useCallback(async () => {
        setCopyingLocation(true);
        try {
            const path = await desktopLogService.logDirectory();
            await navigator.clipboard.writeText(path);
            pushInfoBar({
                tone: 'success',
                title: '已复制日志目录',
                content: path,
                autoDismissMs: 4000,
            });
        } catch (err) {
            pushErrorBar({ title: '复制日志目录失败', raw: errorText(err) });
        } finally {
            setCopyingLocation(false);
        }
    }, []);

    const onCopy = useCallback(async () => {
        if (filtered.length === 0) return;
        try {
            await navigator.clipboard.writeText(serializeDesktopLogs(filtered));
        } catch (err) {
            console.warn('复制日志失败:', err);
        }
    }, [filtered]);

    const emptyKind: 'loading' | 'error' | 'empty-file' | 'no-match' | 'has' = loading
        ? 'loading'
        : error
          ? 'error'
          : logs.length === 0
            ? 'empty-file'
            : filtered.length === 0
              ? 'no-match'
              : 'has';

    return {
        level,
        setLevel,
        query,
        setQuery,
        autoScroll,
        setAutoScroll,
        fontSize,
        decFont: () => setFontSize((s) => Math.max(MIN_FONT, s - 1)),
        incFont: () => setFontSize((s) => Math.min(MAX_FONT, s + 1)),
        loading,
        error,
        reload,
        opening,
        onOpenLocation,
        onCopyLocation,
        copyingLocation,
        onCopy,
        filteredCount: filtered.length,
        copyDisabled: filtered.length === 0,
        emptyKind,
        filtered,
        viewportRef,
    };
}

export type DesktopLogViewer = ReturnType<typeof useDesktopLogViewer>;
