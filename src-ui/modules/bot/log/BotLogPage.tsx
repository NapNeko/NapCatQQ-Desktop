import React, { useState, useEffect, useRef, useMemo } from 'react';
import {
    Button,
    Input,
    Text,
    Badge,
    Tooltip,
} from '@fluentui/react-components';
import {
    ArrowLeftRegular,
    BroomRegular,
    CopyRegular,
    FolderZipRegular,
    SearchRegular,
} from '@fluentui/react-icons';
import { useBotLogStream } from '../../../hooks/bot/useBotLogStream';
import { filterLogs, serializeLogs, type LogChannel } from '../../../core/domain/events/log-buffer';
import './BotLogPage.css';

interface BotLogPageProps {
    botId: string;
    onBack: () => void;
}

export const BotLogPage: React.FC<BotLogPageProps> = ({ botId, onBack }) => {
    const { logs, clear } = useBotLogStream(botId);
    const [searchQuery, setSearchQuery] = useState('');
    const [channelFilter, setChannelFilter] = useState<'all' | LogChannel>('all');
    const [autoScroll, setAutoScroll] = useState(true);

    const containerRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (autoScroll && containerRef.current) {
            containerRef.current.scrollTop = containerRef.current.scrollHeight;
        }
    }, [logs, autoScroll]);

    const filteredLogs = useMemo(
        () => filterLogs(logs, searchQuery, channelFilter),
        [logs, searchQuery, channelFilter],
    );

    const handleCopyLogs = async () => {
        if (filteredLogs.length === 0) return;
        try {
            await navigator.clipboard.writeText(serializeLogs(filteredLogs));
            alert('已复制当前可见日志到剪贴板！');
        } catch (err) {
            console.error('无法复制日志到剪贴板:', err);
        }
    };

    return (
        <div className="ndf-log-page-container">
            <div className="ndf-log-header">
                <div className="ndf-log-header-left">
                    <Button icon={<ArrowLeftRegular />} onClick={onBack} appearance="subtle" />
                    <div>
                        <Text size={400} weight="semibold">实例 [{botId}] 运行日志</Text>
                        <Text block size={100} className="ndf-log-header-subtitle">
                            实时订阅、过滤控制台标准输出输出。崩溃日志一键导出功能属于后续版本。
                        </Text>
                    </div>
                </div>

                <div className="ndf-log-header-right">
                    <Button
                        icon={<BroomRegular />}
                        appearance="secondary"
                        size="small"
                        onClick={clear}
                        disabled={logs.length === 0}
                    >
                        清空面板
                    </Button>
                    <Button
                        icon={<CopyRegular />}
                        appearance="secondary"
                        size="small"
                        onClick={handleCopyLogs}
                        disabled={filteredLogs.length === 0}
                    >
                        复制日志
                    </Button>
                    <Tooltip content="导出 Crash Bundle 崩溃诊断包属于 M6 版本迭代" relationship="label">
                        <Button
                            icon={<FolderZipRegular />}
                            appearance="secondary"
                            size="small"
                            disabled
                        >
                            导出诊断包
                        </Button>
                    </Tooltip>
                </div>
            </div>

            <div className="ndf-log-filter-bar">
                <div className="ndf-log-search-wrapper">
                    <Input
                        contentBefore={<SearchRegular />}
                        value={searchQuery}
                        onChange={(_, val) => setSearchQuery(val.value)}
                        placeholder="搜索关键字过滤运行日志..."
                        style={{ width: '100%' }}
                    />
                </div>

                <div className="ndf-log-channel-tabs">
                    <Button
                        appearance={channelFilter === 'all' ? 'primary' : 'secondary'}
                        size="small"
                        onClick={() => setChannelFilter('all')}
                    >
                        全部
                    </Button>
                    <Button
                        appearance={channelFilter === 'stdout' ? 'primary' : 'secondary'}
                        size="small"
                        onClick={() => setChannelFilter('stdout')}
                    >
                        stdout
                    </Button>
                    <Button
                        appearance={channelFilter === 'stderr' ? 'primary' : 'secondary'}
                        size="small"
                        onClick={() => setChannelFilter('stderr')}
                    >
                        stderr
                    </Button>
                </div>

                <div className="ndf-log-autoscroll-toggle">
                    <label style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '12px', cursor: 'pointer' }}>
                        <input
                            type="checkbox"
                            checked={autoScroll}
                            onChange={(e) => setAutoScroll(e.target.checked)}
                            style={{ cursor: 'pointer' }}
                        />
                        自动滚动到底部
                    </label>
                </div>
            </div>

            <div className="ndf-log-terminal-viewport" ref={containerRef}>
                {logs.length === 0 ? (
                    <div className="ndf-log-empty-state">
                        <Text weight="semibold" size={200} style={{ color: 'var(--colorNeutralForeground4)' }}>
                            暂无运行日志输出
                        </Text>
                        <Text size={100} style={{ color: 'var(--colorNeutralForeground4)', marginTop: '4px' }}>
                            实例可能尚未启动，或暂时未触发任何标准 stdout/stderr 打印流。
                        </Text>
                    </div>
                ) : filteredLogs.length === 0 ? (
                    <div className="ndf-log-empty-state">
                        <Text weight="semibold" size={200} style={{ color: 'var(--colorNeutralForeground4)' }}>
                            没有匹配关键字的日志行
                        </Text>
                        <Text size={100} style={{ color: 'var(--colorNeutralForeground4)', marginTop: '4px' }}>
                            请尝试精简或更换你的过滤关键字。
                        </Text>
                    </div>
                ) : (
                    <div className="ndf-log-lines-container">
                        {filteredLogs.map((log) => (
                            <div key={log.id} className={`ndf-log-line channel-${log.channel}`}>
                                <span className="ndf-log-time">[{log.timestamp}]</span>
                                <span className="ndf-log-badge">
                                    <Badge color={log.channel === 'stderr' ? 'danger' : log.channel === 'stdout' ? 'success' : 'subtle'} size="small" appearance="filled">
                                        {log.channel}
                                    </Badge>
                                </span>
                                <span className="ndf-log-text">{log.text}</span>
                            </div>
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
};

export default BotLogPage;
