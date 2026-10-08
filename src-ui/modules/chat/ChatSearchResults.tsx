// ChatSearch 的结果区：状态摘要、按日期分组的命中列表（含高亮预览）与分页/翻历史页脚。
import { Fragment, type RefObject } from 'react';
import { ChevronDown, History, MapPin, Search } from 'lucide-react';
import { dateGroup, type SearchableMessage } from '../../core/domain/chat/chatSearchModel';
import type { Message } from '../../core/domain/chat/model';
import { dayLabel } from '../../core/domain/debug/chatFormat';
import { ChatAvatar } from '../../shared/chat/ChatAvatar';
import { Button } from '../../shared/ui/Button';
import { SearchContent } from './ChatSearchParts';

interface ChatSearchSummaryProps {
    id: string;
    resultsStatus: string;
    historyStatus: string;
    accountScope: boolean;
    historyError: string;
    searchableCount: number;
    conversationCount: number;
    hasArchive: boolean;
}

export function ChatSearchSummary({
    id,
    resultsStatus,
    historyStatus,
    accountScope,
    historyError,
    searchableCount,
    conversationCount,
    hasArchive,
}: ChatSearchSummaryProps) {
    return (
        <div className="native-chat-search-summary">
            <p id={`${id}-status`} role="status">
                {resultsStatus}
                {historyStatus && ` · ${historyStatus}`}
            </p>
            <p id={`${id}-scope`}>
                {accountScope
                    ? `本账号已保存的 ${searchableCount} 条消息 · ${conversationCount} 个会话`
                    : hasArchive
                      ? `当前会话已加载及已保存的 ${searchableCount} 条消息`
                      : `仅搜索当前会话已加载的 ${searchableCount} 条消息`}
                <span className="native-chat-search-key-hint"> · ↑↓ 选择 · Enter 定位</span>
            </p>
            {!accountScope && historyError && (
                <p role="alert" className="text-danger">
                    读取历史失败：{historyError}
                </p>
            )}
        </div>
    );
}

interface ChatSearchResultsProps {
    id: string;
    resultsRef: RefObject<HTMLDivElement>;
    visibleMatches: SearchableMessage[];
    activeMessageKey: string | undefined;
    accountScope: boolean;
    pattern: RegExp | null;
    term: string;
    onReveal: (message: Message) => void;
}

export function ChatSearchResults({
    id,
    resultsRef,
    visibleMatches,
    activeMessageKey,
    accountScope,
    pattern,
    term,
    onReveal,
}: ChatSearchResultsProps) {
    return (
        <div
            ref={resultsRef}
            id={`${id}-results`}
            role="listbox"
            aria-label="消息搜索结果"
            className="native-chat-search-results"
        >
            {visibleMatches.map(({ message, preview, conversation }, index) => (
                <Fragment key={message.key}>
                    {(index === 0 ||
                        dateGroup(visibleMatches[index - 1].message.at) !==
                            dateGroup(message.at)) && (
                        <div role="presentation" className="native-chat-search-date">
                            {dateGroup(message.at)}
                        </div>
                    )}
                    <div
                        role="option"
                        className="native-chat-search-result"
                        data-recalled={message.recalled || undefined}
                        id={`${id}-result-${index}`}
                        aria-selected={message.key === activeMessageKey}
                        aria-label={`${accountScope ? `${conversation} ` : ''}${message.mine ? '我' : message.senderName} ${dayLabel(message.at)} ${message.recalled ? '已撤回 ' : ''}${preview}`}
                        tabIndex={-1}
                        onMouseDown={(event) => event.preventDefault()}
                        onClick={() => onReveal(message)}
                    >
                        <ChatAvatar
                            contact={{
                                type: 'private',
                                id: message.senderId,
                                name: message.senderName,
                            }}
                            small
                        />
                        <div className="native-chat-search-result-body">
                            <div className="native-chat-search-result-meta">
                                <span className="native-chat-search-result-origin">
                                    <span>{message.mine ? '我' : message.senderName}</span>
                                    {message.recalled && (
                                        <span className="native-chat-search-recalled">已撤回</span>
                                    )}
                                    {accountScope && (
                                        <span
                                            className="native-chat-search-result-session"
                                            title={conversation}
                                        >
                                            {conversation}
                                        </span>
                                    )}
                                </span>
                                <time>
                                    {new Date(message.at).toLocaleTimeString('zh-CN', {
                                        hour: '2-digit',
                                        minute: '2-digit',
                                    })}
                                </time>
                            </div>
                            <SearchContent message={message} pattern={pattern} />
                        </div>
                        <button
                            type="button"
                            className="native-chat-search-locate"
                            aria-label="定位这条消息"
                            title="在会话中定位"
                            onClick={(event) => {
                                event.stopPropagation();
                                onReveal(message);
                            }}
                        >
                            <MapPin size={15} />
                        </button>
                    </div>
                </Fragment>
            ))}
            {!visibleMatches.length && (
                <div className="native-chat-search-empty">
                    <Search size={24} strokeWidth={1.3} />
                    <p>{term ? '换个关键词试试' : '暂无这类消息'}</p>
                </div>
            )}
        </div>
    );
}

interface ChatSearchFooterProps {
    remaining: number;
    canExpandHistory: boolean;
    historyLoading: boolean;
    historyError: string;
    canLoadEarlier: boolean;
    onLoadEarlier?: () => void;
    onShowMore: () => void;
}

export function ChatSearchFooter({
    remaining,
    canExpandHistory,
    historyLoading,
    historyError,
    canLoadEarlier,
    onLoadEarlier,
    onShowMore,
}: ChatSearchFooterProps) {
    if (!remaining && !canExpandHistory) return null;
    return (
        <div className="native-chat-search-footer">
            {!!remaining && (
                <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    className="native-chat-search-more"
                    aria-label={`显示更多结果（还有 ${remaining} 条）`}
                    onClick={onShowMore}
                >
                    <ChevronDown size={14} aria-hidden />
                    显示更多结果
                    <span className="native-chat-search-remaining">{remaining}</span>
                </Button>
            )}
            {canExpandHistory && (
                <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="native-chat-search-earlier"
                    disabled={historyLoading || !canLoadEarlier}
                    onClick={onLoadEarlier}
                >
                    <History size={14} aria-hidden />
                    {historyLoading
                        ? '正在加载…'
                        : historyError
                          ? '重试加载更早消息'
                          : '加载更早消息'}
                </Button>
            )}
        </div>
    );
}
