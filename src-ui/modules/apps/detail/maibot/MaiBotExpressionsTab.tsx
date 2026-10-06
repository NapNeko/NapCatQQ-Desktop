// 学到的表达方式：按聊天 / 已精选 / 未精选看，星标一键精选，勾选后批量；「逐条过一遍」快速精选。
// 默认配置下只有精选的才会用在回复里，页头把这件事说清楚。

import { useState } from 'react';
import { Info, MessageSquareQuote, Plus, Sparkles, Star, StarOff, Trash2 } from 'lucide-react';
import { Button, Select } from '../../../../shared/ui';
import type {
    AppInstance,
    MaiBotExpression,
    MaiBotExpressionFilter,
    MaiBotRuntimeStatus,
} from '../../../../core/ipc/types';
import {
    useMaiBotExpressionAction,
    useMaiBotExpressionOverview,
    useMaiBotExpressions,
} from '../../../../hooks/apps/useMaiBotLearning';
import { ConfirmDelete, EmptyHint } from '../entityParts';
import {
    Pager,
    ResourcePane,
    SearchBox,
    Segmented,
    SelectionBar,
    useDebounced,
    useSelection,
} from '../resourceParts';
import { MaiBotLiveGate, maibotLive } from './MaiBotLiveGate';
import {
    ExpressionDialog,
    ExpressionReview,
    ExpressionRow,
    type ExpressionDraft,
} from './maibotExpressionParts';

const PAGE_SIZE = 20;
const ALL_CHATS = '__all__';

export const MaiBotExpressionsTab: React.FC<{
    instance: AppInstance;
    status: MaiBotRuntimeStatus | undefined;
    /** 配置里「使用精选表达」开着：只有精选的会用在回复里 */
    curatedOnly: boolean;
    onStart: () => void;
    starting: boolean;
}> = ({ instance, status, curatedOnly, onStart, starting }) => {
    const live = maibotLive(status);
    const [chatId, setChatId] = useState(ALL_CHATS);
    const [filter, setFilter] = useState<MaiBotExpressionFilter>('all');
    const [search, setSearch] = useState('');
    const [page, setPage] = useState(1);
    const [draft, setDraft] = useState<ExpressionDraft | null>(null);
    const [pendingDelete, setPendingDelete] = useState<number[] | null>(null);
    const [reviewing, setReviewing] = useState(false);
    const sel = useSelection<number>();
    const q = useDebounced(search.trim());

    const query = {
        page,
        page_size: PAGE_SIZE,
        search: q,
        chat_id: chatId === ALL_CHATS ? '' : chatId,
        filter,
    };
    const list = useMaiBotExpressions(instance.id, query, live);
    const overview = useMaiBotExpressionOverview(instance.id, live);
    const reviewQueue = useMaiBotExpressions(
        instance.id,
        { page: 1, page_size: 50, search: '', chat_id: '', filter: 'uncurated' },
        live && reviewing,
    );
    const act = useMaiBotExpressionAction(instance.id);

    if (!live)
        return (
            <MaiBotLiveGate
                status={status}
                what="学到的表达方式"
                onStart={onStart}
                starting={starting}
            />
        );

    const ov = overview.data;
    const chats = ov?.chats ?? [];
    const usedChats = chats.filter((c) => ov?.used_chat_ids.includes(c.chat_id));
    const items = list.data?.items ?? [];
    const total = list.data?.total ?? 0;
    const resetPage =
        <T,>(set: (v: T) => void) =>
        (v: T) => {
            set(v);
            setPage(1);
            sel.clear();
        };
    const curate = (ids: number[], curated: boolean) =>
        act.mutateAsync({ op: 'curate', ids, curated });
    const pickedIds = [...sel.picked];
    const pageIds = items.map((e) => e.id);
    const pageAllPicked = pageIds.every((id) => sel.has(id));

    const toolbar = (
        <>
            <Select
                className="w-44 [&_button]:h-8 [&_button]:min-h-8 [&_button]:text-[12.5px]"
                items={[
                    { value: ALL_CHATS, label: '全部聊天' },
                    ...usedChats.map((c) => ({ value: c.chat_id, label: c.chat_name })),
                ]}
                value={chatId}
                onValueChange={resetPage(setChatId)}
            />
            <SearchBox
                className="w-56"
                placeholder="搜情境或说法"
                value={search}
                onChange={resetPage(setSearch)}
            />
            <Segmented
                items={[
                    { value: 'all', label: '全部', count: ov?.total },
                    { value: 'curated', label: '已精选', count: ov?.curated },
                    { value: 'uncurated', label: '未精选', count: ov?.uncurated },
                ]}
                value={filter}
                onChange={resetPage(setFilter)}
            />
            <span className="flex-1" />
            {(ov?.uncurated ?? 0) > 0 && (
                <Button size="sm" variant="ghost" onClick={() => setReviewing(true)}>
                    <Sparkles size={13} />
                    逐条过一遍
                </Button>
            )}
            <Button
                size="sm"
                variant="secondary"
                onClick={() =>
                    setDraft({
                        situation: '',
                        style: '',
                        chat_id: chatId === ALL_CHATS ? '' : chatId,
                    })
                }
            >
                <Plus size={13} />
                新建
            </Button>
        </>
    );

    const notice = curatedOnly && (ov?.uncurated ?? 0) > 0 && (
        <p className="flex items-center gap-1.5 text-xs text-text-tertiary">
            <Info size={13} className="shrink-0 text-info" />
            只有精选过的才会用在回复里，新学到的在「未精选」里等你挑。
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
                    total={total}
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
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={act.isPending}
                        onClick={() => void curate(pickedIds, true).then(sel.clear)}
                    >
                        <Star size={13} />
                        精选
                    </Button>
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={act.isPending}
                        onClick={() => void curate(pickedIds, false).then(sel.clear)}
                    >
                        <StarOff size={13} />
                        取消精选
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
                    icon={MessageSquareQuote}
                    title={
                        q || filter !== 'all' || chatId !== ALL_CHATS
                            ? '没有对得上的表达方式'
                            : '麦麦还没学到表达方式。多聊一阵它会自己学，也可以手动加一条。'
                    }
                />
            ) : (
                <div className="flex flex-col gap-1.5">
                    {items.map((item: MaiBotExpression) => (
                        <ExpressionRow
                            key={item.id}
                            item={item}
                            selected={sel.has(item.id)}
                            showChat={chatId === ALL_CHATS}
                            busy={act.isPending}
                            onPick={(shift) => sel.pick(item.id, shift, pageIds)}
                            onToggleCurated={() => void curate([item.id], !item.curated)}
                            onEdit={() =>
                                setDraft({
                                    id: item.id,
                                    situation: item.situation,
                                    style: item.style,
                                    chat_id: item.chat_id,
                                })
                            }
                            onDelete={() => setPendingDelete([item.id])}
                        />
                    ))}
                </div>
            )}

            <ExpressionDialog
                draft={draft}
                chats={chats}
                busy={act.isPending}
                onChange={setDraft}
                onCancel={() => setDraft(null)}
                onConfirm={() => {
                    if (!draft) return;
                    const original = items.find((e) => e.id === draft.id);
                    const run =
                        draft.id === undefined
                            ? act.mutateAsync({
                                  op: 'create',
                                  situation: draft.situation,
                                  style: draft.style,
                                  chat_id: draft.chat_id,
                                  toast: true,
                              })
                            : act.mutateAsync({
                                  op: 'update',
                                  id: draft.id,
                                  situation: draft.situation,
                                  style: draft.style,
                                  chat_id:
                                      original && draft.chat_id !== original.chat_id
                                          ? draft.chat_id
                                          : null,
                              });
                    void run.then(() => setDraft(null));
                }}
            />

            <ConfirmDelete
                open={pendingDelete !== null}
                title={
                    pendingDelete && pendingDelete.length > 1
                        ? `删掉这 ${pendingDelete.length} 条表达方式？`
                        : '删掉这条表达方式？'
                }
                description="删掉就找不回来了。以后聊到类似的，麦麦可能又学回来。"
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

            <ExpressionReview
                open={reviewing}
                queue={reviewQueue.data?.items ?? []}
                loading={reviewQueue.isLoading}
                onCurate={(e) => curate([e.id], true)}
                onDelete={(e) => act.mutateAsync({ op: 'delete', ids: [e.id] })}
                onClose={() => setReviewing(false)}
            />
        </ResourcePane>
    );
};
