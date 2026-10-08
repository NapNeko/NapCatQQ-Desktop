// ChatSearch 的搜索工具条：类型页签、关键词输入、范围切换与筛选弹层。
// 状态全部归 ChatSearch 持有，这里只负责渲染与事件回传。
import { type MutableRefObject, type RefObject, useRef } from 'react';
import { Filter, Search, X } from 'lucide-react';
import { categories, type Category } from '../../core/domain/chat/chatSearchModel';
import type { SessionKey } from '../../core/domain/chat/model';
import { Popover, PopoverContent, PopoverTrigger } from '../../shared/ui/Popover';

interface ChatSearchToolbarProps {
    id: string;
    field: RefObject<HTMLInputElement>;
    composing: MutableRefObject<boolean>;
    query: string;
    searchExpanded: boolean;
    initialSearchExpanded: boolean;
    category: Category;
    hasArchive: boolean;
    accountScope: boolean;
    session: SessionKey | null | undefined;
    matchesLength: number;
    activeIndex: number;
    hasActive: boolean;
    senders: [string, string][];
    sender: string;
    from: string;
    to: string;
    onExpandedChange: (expanded: boolean) => void;
    onCategoryChange: (value: Category) => void;
    onScopeChange: (value: 'conversation' | 'account') => void;
    resetQuery: (value: string) => void;
    onSelectIndex: (nextIndex: number) => void;
    onSubmitActive: () => void;
    onSenderChange: (value: string) => void;
    onFromChange: (value: string) => void;
    onToChange: (value: string) => void;
    onClearFilters: () => void;
}

export function ChatSearchToolbar({
    id,
    field,
    composing,
    query,
    searchExpanded,
    initialSearchExpanded,
    category,
    hasArchive,
    accountScope,
    session,
    matchesLength,
    activeIndex,
    hasActive,
    senders,
    sender,
    from,
    to,
    onExpandedChange,
    onCategoryChange,
    onScopeChange,
    resetQuery,
    onSelectIndex,
    onSubmitActive,
    onSenderChange,
    onFromChange,
    onToChange,
    onClearFilters,
}: ChatSearchToolbarProps) {
    const searchToggle = useRef<HTMLButtonElement>(null);
    return (
        <div className="native-chat-search-toolbar" data-search-open={searchExpanded}>
            <div role="tablist" aria-label="消息类型" className="native-chat-search-categories">
                {categories.map(([value, label], index) => (
                    <button
                        type="button"
                        key={value}
                        role="tab"
                        aria-selected={category === value}
                        tabIndex={category === value ? 0 : -1}
                        onClick={() => onCategoryChange(value)}
                        onKeyDown={(event) => {
                            if (!['ArrowLeft', 'ArrowRight'].includes(event.key)) return;
                            event.preventDefault();
                            const next =
                                (index +
                                    (event.key === 'ArrowRight' ? 1 : -1) +
                                    categories.length) %
                                categories.length;
                            onCategoryChange(categories[next][0]);
                            event.currentTarget.parentElement
                                ?.querySelectorAll<HTMLButtonElement>('[role=tab]')
                                ?.[next]?.focus();
                        }}
                    >
                        {label}
                    </button>
                ))}
            </div>
            <button
                ref={searchToggle}
                type="button"
                autoFocus={!initialSearchExpanded}
                className="native-chat-icon native-chat-search-toggle"
                aria-label="搜索消息"
                aria-expanded={searchExpanded}
                aria-controls={`${id}-query`}
                title={searchExpanded ? '收起搜索' : '搜索消息'}
                onClick={() => {
                    if (searchExpanded) {
                        resetQuery('');
                        searchToggle.current?.focus();
                    }
                    onExpandedChange(!searchExpanded);
                }}
            >
                <Search size={15} aria-hidden />
            </button>
            {searchExpanded && (
                <div id={`${id}-query`} className="native-chat-search native-chat-search-query">
                    <Search size={14} aria-hidden />
                    <input
                        ref={field}
                        role="combobox"
                        aria-label={
                            hasArchive
                                ? accountScope
                                    ? '搜索本账号已保存消息'
                                    : '搜索当前会话消息'
                                : '搜索已加载消息'
                        }
                        aria-autocomplete="list"
                        aria-expanded={matchesLength > 0}
                        aria-controls={`${id}-results`}
                        aria-describedby={`${id}-status ${id}-scope`}
                        aria-activedescendant={
                            hasActive ? `${id}-result-${activeIndex}` : undefined
                        }
                        placeholder={accountScope ? '搜索本账号已保存记录' : '搜索当前会话'}
                        value={query}
                        onChange={(event) => resetQuery(event.target.value)}
                        onCompositionStart={() => {
                            composing.current = true;
                        }}
                        onCompositionEnd={() => {
                            composing.current = false;
                        }}
                        onKeyDown={(event) => {
                            if (
                                event.nativeEvent.isComposing ||
                                composing.current ||
                                event.keyCode === 229
                            )
                                return;
                            if (
                                (event.key === 'ArrowDown' || event.key === 'ArrowUp') &&
                                matchesLength
                            ) {
                                event.preventDefault();
                                const nextIndex = Math.max(
                                    0,
                                    Math.min(
                                        matchesLength - 1,
                                        activeIndex + (event.key === 'ArrowDown' ? 1 : -1),
                                    ),
                                );
                                onSelectIndex(nextIndex);
                            } else if (event.key === 'Enter' && hasActive) {
                                event.preventDefault();
                                onSubmitActive();
                            }
                        }}
                    />
                    {query && (
                        <button
                            type="button"
                            aria-label="清除消息搜索"
                            onClick={() => {
                                resetQuery('');
                                field.current?.focus();
                            }}
                        >
                            <X size={13} />
                        </button>
                    )}
                </div>
            )}
            {hasArchive && (
                <div role="tablist" aria-label="搜索范围" className="native-chat-search-scope">
                    {(['conversation', 'account'] as const).map((value, index) => (
                        <button
                            type="button"
                            key={value}
                            role="tab"
                            aria-selected={accountScope === (value === 'account')}
                            disabled={value === 'conversation' && !session}
                            tabIndex={accountScope === (value === 'account') ? 0 : -1}
                            onClick={() => onScopeChange(value)}
                            onKeyDown={(event) => {
                                if (!session || !['ArrowLeft', 'ArrowRight'].includes(event.key))
                                    return;
                                event.preventDefault();
                                const next = index === 0 ? 1 : 0;
                                onScopeChange(next === 0 ? 'conversation' : 'account');
                                event.currentTarget.parentElement
                                    ?.querySelectorAll<HTMLButtonElement>('[role=tab]')
                                    ?.[next]?.focus();
                            }}
                        >
                            {value === 'conversation' ? '当前会话' : '本账号'}
                        </button>
                    ))}
                </div>
            )}
            <Popover>
                <PopoverTrigger asChild>
                    <button
                        type="button"
                        className="native-chat-search-filter-trigger"
                        aria-label="筛选"
                        data-active={!!sender || !!from || !!to}
                    >
                        <Filter size={14} />
                        <span>筛选</span>
                    </button>
                </PopoverTrigger>
                <PopoverContent align="end" className="native-chat-search-filters">
                    <label>
                        发送者
                        <select
                            aria-label="筛选发送者"
                            value={sender}
                            onChange={(event) => onSenderChange(event.target.value)}
                        >
                            <option value="">所有人</option>
                            {senders.map(([senderId, name]) => (
                                <option key={senderId} value={senderId}>
                                    {name}
                                </option>
                            ))}
                        </select>
                    </label>
                    <div className="native-chat-search-date-fields">
                        <label>
                            起始日期
                            <input
                                type="date"
                                aria-label="起始日期"
                                value={from}
                                onChange={(event) => onFromChange(event.target.value)}
                            />
                        </label>
                        <label>
                            结束日期
                            <input
                                type="date"
                                aria-label="结束日期"
                                value={to}
                                onChange={(event) => onToChange(event.target.value)}
                            />
                        </label>
                    </div>
                    <button
                        type="button"
                        className="native-chat-text-button"
                        onClick={onClearFilters}
                    >
                        清除筛选
                    </button>
                </PopoverContent>
            </Popover>
        </div>
    );
}
