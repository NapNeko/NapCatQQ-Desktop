// 学到的黑话：按聊天 / 已确认 / 不算 / 固定 / 全局看，勾选后批量标记或删；点一行改含义和适用范围。

import { useState } from 'react';
import { BookA, Check, Plus, Trash2, X } from 'lucide-react';
import { Button, Select } from '../../../../shared/ui';
import type { AppInstance, MaiBotJargonFilter, MaiBotRuntimeStatus } from '../../../../core/ipc/types';
import { useMaiBotJargonAction, useMaiBotJargonOverview, useMaiBotJargons } from '../../../../hooks/apps/useMaiBotLearning';
import { ConfirmDelete, EmptyHint } from '../entityParts';
import { Pager, ResourcePane, SearchBox, Segmented, SelectionBar, useDebounced, useSelection } from '../resourceParts';
import { MaiBotLiveGate, maibotLive } from './MaiBotLiveGate';
import { JargonDialog, JargonRow, type JargonDraft } from './maibotJargonParts';

const PAGE_SIZE = 20;
const ALL_CHATS = '__all__';

export const MaiBotJargonTab: React.FC<{
    instance: AppInstance;
    status: MaiBotRuntimeStatus | undefined;
    onStart: () => void;
    starting: boolean;
}> = ({ instance, status, onStart, starting }) => {
    const live = maibotLive(status);
    const [chatId, setChatId] = useState(ALL_CHATS);
    const [filter, setFilter] = useState<MaiBotJargonFilter>('all');
    const [search, setSearch] = useState('');
    const [page, setPage] = useState(1);
    const [draft, setDraft] = useState<JargonDraft | null>(null);
    const [pendingDelete, setPendingDelete] = useState<number[] | null>(null);
    const sel = useSelection<number>();
    const q = useDebounced(search.trim());

    const query = { page, page_size: PAGE_SIZE, search: q, chat_id: chatId === ALL_CHATS ? '' : chatId, filter };
    const list = useMaiBotJargons(instance.id, query, live);
    const overview = useMaiBotJargonOverview(instance.id, live);
    const act = useMaiBotJargonAction(instance.id);

    if (!live) return <MaiBotLiveGate status={status} what="学到的黑话" onStart={onStart} starting={starting} />;

    const ov = overview.data;
    const chats = ov?.chats ?? [];
    const usedChats = chats.filter((c) => ov?.used_chat_ids.includes(c.chat_id));
    const items = list.data?.items ?? [];
    const resetPage = <T,>(set: (v: T) => void) => (v: T) => {
        set(v);
        setPage(1);
        sel.clear();
    };
    const pickedIds = [...sel.picked];
    const pageIds = items.map((j) => j.id);
    const pageAllPicked = pageIds.every((id) => sel.has(id));
    const setJargon = (is_jargon: boolean) =>
        void act.mutateAsync({ op: 'set_jargon', ids: pickedIds, is_jargon }).then(sel.clear);

    const toolbar = (
        <>
            <Select
                className="w-44 [&_button]:h-8 [&_button]:min-h-8 [&_button]:text-[12.5px]"
                items={[{ value: ALL_CHATS, label: '全部聊天' }, ...usedChats.map((c) => ({ value: c.chat_id, label: c.chat_name }))]}
                value={chatId}
                onValueChange={resetPage(setChatId)}
            />
            <SearchBox className="w-48" placeholder="搜黑话" value={search} onChange={resetPage(setSearch)} />
            <Segmented
                items={[
                    { value: 'all', label: '全部', count: ov?.total },
                    { value: 'confirmed', label: '已确认', count: ov?.confirmed },
                    { value: 'not_jargon', label: '不算', count: ov ? ov.total - ov.confirmed : undefined },
                    { value: 'pinned', label: '固定', count: ov?.pinned },
                    { value: 'global', label: '全局', count: ov?.global },
                ]}
                value={filter}
                onChange={resetPage(setFilter)}
            />
            <span className="flex-1" />
            <Button
                size="sm"
                variant="secondary"
                disabled={chats.length === 0}
                title={chats.length === 0 ? '麦麦还没见过聊天，黑话要挂在聊天上' : undefined}
                onClick={() =>
                    setDraft({
                        content: '',
                        meaning: '',
                        chat_ids: chatId === ALL_CHATS ? [] : [chatId],
                        is_global: false,
                        is_jargon: true,
                        pinned: true,
                    })
                }
            >
                <Plus size={13} />
                新建
            </Button>
        </>
    );

    return (
        <ResourcePane
            toolbar={toolbar}
            footer={
                <Pager
                    page={page}
                    pageSize={PAGE_SIZE}
                    total={list.data?.total ?? 0}
                    onPage={(p) => {
                        setPage(p);
                        sel.clear();
                    }}
                />
            }
            overlay={
                <SelectionBar
                    count={sel.picked.size}
                    onClear={sel.clear}
                    onSelectAll={pageAllPicked ? undefined : () => sel.setAll(pageIds, true)}
                >
                    <Button size="sm" variant="ghost" disabled={act.isPending} onClick={() => setJargon(true)}>
                        <Check size={13} />
                        算黑话
                    </Button>
                    <Button size="sm" variant="ghost" disabled={act.isPending} onClick={() => setJargon(false)}>
                        <X size={13} />
                        不算
                    </Button>
                    <Button
                        size="sm"
                        variant="ghost"
                        className="text-danger hover:bg-danger-soft hover:text-danger"
                        disabled={act.isPending}
                        onClick={() => setPendingDelete(pickedIds)}
                    >
                        <Trash2 size={13} />
                        删除
                    </Button>
                </SelectionBar>
            }
        >
            {items.length === 0 && !list.isFetching ? (
                <EmptyHint
                    icon={BookA}
                    title={
                        q || filter !== 'all' || chatId !== ALL_CHATS
                            ? '没有对得上的黑话'
                            : '麦麦还没学到黑话。群里反复出现、外人看不懂的词，它会自己记下来推意思。'
                    }
                />
            ) : (
                <div className="flex flex-col gap-1.5">
                    {items.map((item) => (
                        <JargonRow
                            key={item.id}
                            item={item}
                            selected={sel.has(item.id)}
                            busy={act.isPending}
                            onPick={(shift) => sel.pick(item.id, shift, pageIds)}
                            onEdit={() =>
                                setDraft({
                                    id: item.id,
                                    content: item.content,
                                    meaning: item.meaning,
                                    chat_ids: item.chat_ids,
                                    is_global: item.is_global,
                                    is_jargon: item.is_jargon,
                                    pinned: item.pinned,
                                })
                            }
                            onDelete={() => setPendingDelete([item.id])}
                        />
                    ))}
                </div>
            )}

            <JargonDialog
                draft={draft}
                chats={chats}
                busy={act.isPending}
                onChange={setDraft}
                onCancel={() => setDraft(null)}
                onConfirm={() => {
                    if (!draft) return;
                    const original = items.find((j) => j.id === draft.id);
                    const sameChats =
                        !!original &&
                        original.chat_ids.length === draft.chat_ids.length &&
                        original.chat_ids.every((c) => draft.chat_ids.includes(c));
                    const run =
                        draft.id === undefined
                            ? act.mutateAsync({
                                  op: 'create',
                                  content: draft.content,
                                  meaning: draft.meaning,
                                  chat_ids: draft.chat_ids,
                                  is_global: draft.is_global,
                                  toast: true,
                              })
                            : act.mutateAsync({
                                  op: 'update',
                                  id: draft.id,
                                  content: draft.content,
                                  meaning: draft.meaning,
                                  chat_ids: sameChats ? null : draft.chat_ids,
                                  is_global: draft.is_global,
                                  is_jargon: draft.is_jargon,
                                  pinned: draft.pinned,
                              });
                    void run.then(() => setDraft(null));
                }}
            />

            <ConfirmDelete
                open={pendingDelete !== null}
                title={pendingDelete && pendingDelete.length > 1 ? `删掉这 ${pendingDelete.length} 条黑话？` : '删掉这条黑话？'}
                description="删掉就找不回来了。这个词要是还常出现，麦麦可能又记回来。"
                busy={act.isPending}
                onCancel={() => setPendingDelete(null)}
                onConfirm={() => {
                    if (!pendingDelete) return;
                    void act.mutateAsync({ op: 'delete', ids: pendingDelete }).then(() => {
                        setPendingDelete(null);
                        sel.clear();
                    });
                }}
            />
        </ResourcePane>
    );
};
