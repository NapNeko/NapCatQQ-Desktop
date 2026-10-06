// 学到的行为：在什么情境下、谁做了什么、结果怎样。按聊天 / 在用停用 / 怎么学到的看，按最近、分数、次数排；
// 点一条看情境分量和每次反馈。上游只给看，这页不改东西；场景图谱、检索调试这类分析工具留在麦麦 WebUI。

import { useState } from 'react';
import { ExternalLink, Info, Route } from 'lucide-react';
import { Button, Select } from '../../../../shared/ui';
import type {
    AppInstance,
    MaiBotBehavior,
    MaiBotBehaviorFilter,
    MaiBotBehaviorOrigin,
    MaiBotBehaviorSort,
    MaiBotRuntimeStatus,
} from '../../../../core/ipc/types';
import {
    useMaiBotBehavior,
    useMaiBotBehaviorOverview,
    useMaiBotBehaviors,
} from '../../../../hooks/apps/useMaiBotLearning';
import { errorText } from '../../../../core/domain/errors';
import { EmptyHint } from '../entityParts';
import { Pager, ResourcePane, SearchBox, Segmented, useDebounced } from '../resourceParts';
import { MaiBotLiveGate, maibotLive } from './MaiBotLiveGate';
import { BehaviorDetailDialog, BehaviorRow } from './maibotBehaviorParts';

const PAGE_SIZE = 20;
const ALL_CHATS = '__all__';
const SELECT_SM = '[&_button]:h-8 [&_button]:min-h-8 [&_button]:text-[12.5px]';

const ORIGINS: { value: MaiBotBehaviorOrigin; label: string }[] = [
    { value: 'all', label: '全部来源' },
    { value: 'observed', label: '看别人学的' },
    { value: 'self_reflection', label: '自己试出来的' },
];

const SORTS: { value: MaiBotBehaviorSort; label: string }[] = [
    { value: 'recent', label: '最近用到' },
    { value: 'score', label: '分数最高' },
    { value: 'seen', label: '见得最多' },
    { value: 'used', label: '用得最多' },
];

export const MaiBotBehaviorTab: React.FC<{
    instance: AppInstance;
    status: MaiBotRuntimeStatus | undefined;
    /** 配置里「启用行为学习」；关着时已经学到的照样会用，只是不再学新的 */
    learningOn: boolean;
    onGoTab: (tab: string) => void;
    onOpenWebUi: (path: string) => void;
    onStart: () => void;
    starting: boolean;
}> = ({ instance, status, learningOn, onGoTab, onOpenWebUi, onStart, starting }) => {
    const live = maibotLive(status);
    const [chatId, setChatId] = useState(ALL_CHATS);
    const [filter, setFilter] = useState<MaiBotBehaviorFilter>('all');
    const [origin, setOrigin] = useState<MaiBotBehaviorOrigin>('all');
    const [sort, setSort] = useState<MaiBotBehaviorSort>('recent');
    const [search, setSearch] = useState('');
    const [page, setPage] = useState(1);
    // 关掉详情框时留着这条，退场动画那几帧内容不空
    const [opened, setOpened] = useState<{ item: MaiBotBehavior; open: boolean } | null>(null);
    const q = useDebounced(search.trim());

    const query = {
        page,
        page_size: PAGE_SIZE,
        search: q,
        chat_id: chatId === ALL_CHATS ? '' : chatId,
        filter,
        origin,
        sort,
    };
    const list = useMaiBotBehaviors(instance.id, query, live);
    const overview = useMaiBotBehaviorOverview(instance.id, live);
    const detail = useMaiBotBehavior(instance.id, live && opened ? opened.item.id : null);

    if (!live)
        return (
            <MaiBotLiveGate
                status={status}
                what="学到的行为"
                onStart={onStart}
                starting={starting}
            />
        );

    const ov = overview.data;
    const chats = ov?.chats ?? [];
    const items = list.data?.items ?? [];
    const narrowed = !!q || filter !== 'all' || origin !== 'all' || chatId !== ALL_CHATS;
    const resetPage =
        <T,>(set: (v: T) => void) =>
        (v: T) => {
            set(v);
            setPage(1);
        };

    const toolbar = (
        <>
            <Select
                className={`w-40 ${SELECT_SM}`}
                items={[
                    { value: ALL_CHATS, label: '全部聊天' },
                    ...chats.map((c) => ({ value: c.chat_id, label: c.chat_name })),
                ]}
                value={chatId}
                onValueChange={resetPage(setChatId)}
            />
            <SearchBox
                className="w-52"
                placeholder="搜情境、做法或结果"
                value={search}
                onChange={resetPage(setSearch)}
            />
            <Segmented
                items={[
                    { value: 'all', label: '全部', count: ov?.total },
                    { value: 'enabled', label: '在用', count: ov?.enabled },
                    { value: 'disabled', label: '停用', count: ov?.disabled },
                ]}
                value={filter}
                onChange={resetPage(setFilter)}
            />
            <Select
                className={`w-32 ${SELECT_SM}`}
                items={ORIGINS}
                value={origin}
                onValueChange={resetPage(setOrigin)}
            />
            <Select
                className={`w-28 ${SELECT_SM}`}
                items={SORTS}
                value={sort}
                onValueChange={resetPage(setSort)}
            />
            <span className="flex-1" />
            <Button
                size="sm"
                variant="ghost"
                title="场景簇图谱、标签网络、检索调试这些分析工具在麦麦 WebUI 里"
                onClick={() => onOpenWebUi('/resource/behavior')}
            >
                <ExternalLink size={13} />
                场景图谱
            </Button>
        </>
    );

    const notice = !learningOn && (
        <p className="flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs text-text-tertiary">
            <Info size={13} className="shrink-0 text-info" />
            行为学习没开：麦麦不会再学新的，已经学到的照样会用。
            <button
                type="button"
                className="text-brand hover:underline"
                onClick={() => onGoTab('advanced')}
            >
                去打开
            </button>
        </p>
    );

    return (
        <ResourcePane
            toolbar={toolbar}
            notice={notice}
            footer={
                <Pager
                    page={page}
                    pageSize={PAGE_SIZE}
                    total={list.data?.total ?? 0}
                    onPage={setPage}
                />
            }
        >
            {items.length === 0 && !list.isFetching ? (
                <EmptyHint
                    icon={Route}
                    title={
                        narrowed
                            ? '没有对得上的经验'
                            : learningOn
                              ? '麦麦还没学到行为经验。它会在聊天里看什么情况下怎么回应效果好，攒够了才有。'
                              : '还没有学到的行为经验。打开行为学习后，麦麦会在聊天里慢慢攒。'
                    }
                    action={
                        !narrowed && !learningOn ? (
                            <Button
                                size="sm"
                                variant="secondary"
                                onClick={() => onGoTab('advanced')}
                            >
                                去打开行为学习
                            </Button>
                        ) : undefined
                    }
                />
            ) : (
                <div className="flex flex-col gap-1.5">
                    {items.map((item) => (
                        <BehaviorRow
                            key={item.id}
                            item={item}
                            showChat={chatId === ALL_CHATS}
                            onOpen={() => setOpened({ item, open: true })}
                        />
                    ))}
                </div>
            )}

            <BehaviorDetailDialog
                open={!!opened?.open}
                item={opened?.item}
                detail={detail.data}
                loading={detail.isLoading}
                error={detail.error ? errorText(detail.error) : undefined}
                onClose={() => setOpened((o) => o && { ...o, open: false })}
            />
        </ResourcePane>
    );
};
