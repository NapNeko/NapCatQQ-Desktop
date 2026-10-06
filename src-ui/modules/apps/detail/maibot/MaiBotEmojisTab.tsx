// 麦麦的表情包：按收下 / 认识 / 不认识 / 丢弃看，搜标签，三种排序；点开一张看大图、改标签、收下丢弃，
// ← → 翻着过；勾选后批量。上传从系统对话框挑，或者直接往窗口里拖。

import { useRef, useState } from 'react';
import { ArchiveRestore, Check, Info, Smile, Trash2, X } from 'lucide-react';
import { Button, Select } from '../../../../shared/ui';
import type { EmojiMove } from '../../../../core/domain/apps/maibotEmoji';
import type {
    AppInstance,
    MaiBotEmoji,
    MaiBotEmojiFilter,
    MaiBotEmojiSort,
    MaiBotLocalImage,
    MaiBotRuntimeStatus,
} from '../../../../core/ipc/types';
import type { MaiBotEmojiConfig } from '../../../../core/ipc/generated/maibot/MaiBotEmojiConfig';
import {
    useMaiBotEmojiAction,
    useMaiBotEmojiFiles,
    useMaiBotEmojiOverview,
    useMaiBotEmojis,
    useMaiBotEmojiUpload,
} from '../../../../hooks/apps/useMaiBotEmojis';
import { pushInfoBar } from '../../../../hooks/ui/globalInfoBarStore';
import { useTauriFileDrop } from '../../../../hooks/ui/useTauriFileDrop';
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
import { EmojiDetail, EmojiTile } from './maibotEmojiParts';
import { EmojiDropOverlay, EmojiUploadDialog, UploadButton } from './maibotEmojiUpload';

const PAGE_SIZE = 40;
const SORTS: { value: MaiBotEmojiSort; label: string }[] = [
    { value: 'newest', label: '最新' },
    { value: 'most_used', label: '用得最多' },
    { value: 'recently_used', label: '最近用过' },
];

/** 同一张拖进来两次只留一张 */
function mergeFiles(
    prev: readonly MaiBotLocalImage[] | null,
    next: readonly MaiBotLocalImage[],
): MaiBotLocalImage[] {
    const out = [...(prev ?? [])];
    for (const f of next) if (!out.some((x) => x.path === f.path)) out.push(f);
    return out;
}

export const MaiBotEmojisTab: React.FC<{
    instance: AppInstance;
    status: MaiBotRuntimeStatus | undefined;
    /** 配置里的表情包小节：发送池上限、满了换不换、维护间隔、自动收集 */
    config: MaiBotEmojiConfig | undefined;
    onGoTab: (tab: string) => void;
    onStart: () => void;
    starting: boolean;
}> = ({ instance, status, config, onGoTab, onStart, starting }) => {
    const live = maibotLive(status);
    const [filter, setFilter] = useState<MaiBotEmojiFilter>('all');
    const [sort, setSort] = useState<MaiBotEmojiSort>('newest');
    const [search, setSearch] = useState('');
    const [page, setPage] = useState(1);
    const [openId, setOpenId] = useState<number | null>(null);
    const lastOpen = useRef<{ emoji: MaiBotEmoji; index: number } | null>(null);
    const [pendingDelete, setPendingDelete] = useState<number[] | null>(null);
    const [files, setFiles] = useState<MaiBotLocalImage[] | null>(null);
    const [uploadTags, setUploadTags] = useState<string[]>([]);
    const sel = useSelection<number>();
    const q = useDebounced(search.trim());

    const list = useMaiBotEmojis(
        instance.id,
        { page, page_size: PAGE_SIZE, search: q, filter, sort },
        live,
    );
    const overview = useMaiBotEmojiOverview(instance.id, live);
    const act = useMaiBotEmojiAction(instance.id);
    const upload = useMaiBotEmojiUpload(instance.id);
    const emojiFiles = useMaiBotEmojiFiles();

    const addFiles = async (paths: string[]) => {
        if (paths.length === 0) return;
        const seen = await emojiFiles.read(paths);
        // 读失败已经弹过错误条，别再开一个空的上传框
        if (seen.length > 0) setFiles((prev) => mergeFiles(prev, seen));
    };
    const pickFiles = async () => addFiles(await emojiFiles.pick());
    const { dragging } = useTauriFileDrop(live, (paths) => void addFiles(paths));

    if (!live)
        return (
            <MaiBotLiveGate status={status} what="表情包" onStart={onStart} starting={starting} />
        );

    const ov = overview.data;
    const items = list.data?.items ?? [];
    const pageIds = items.map((e) => e.id);
    const pageAllPicked = pageIds.every((id) => sel.has(id));
    const pickedIds = [...sel.picked];
    const resetPage =
        <T,>(set: (v: T) => void) =>
        (v: T) => {
            set(v);
            setPage(1);
            sel.clear();
        };
    // 收下的不会马上发：上游等下一轮表情包维护才把它放进发送池
    const poolNote = config
        ? `约 ${config.check_interval} 分钟内开始发（下一轮表情包维护）`
        : '下一轮表情包维护后开始发';
    const move = (ids: number[], m: EmojiMove) =>
        act.mutateAsync({ op: m, ids, toast: true, note: m === 'adopt' ? poolNote : undefined });

    // 详情翻页：当前这张改了状态被筛掉时，补到原位置上的那张就是「下一张」
    const openIndex = openId === null ? -1 : items.findIndex((e) => e.id === openId);
    if (openIndex >= 0) lastOpen.current = { emoji: items[openIndex], index: openIndex };
    const shown =
        openId === null
            ? null
            : openIndex >= 0
              ? items[openIndex]
              : (lastOpen.current?.emoji ?? null);
    const base = openIndex >= 0 ? openIndex : (lastOpen.current?.index ?? 0);
    const nextIdx = openIndex >= 0 ? openIndex + 1 : base;
    const onNext =
        openId !== null && nextIdx < items.length ? () => setOpenId(items[nextIdx].id) : undefined;
    const onPrev = openId !== null && base > 0 ? () => setOpenId(items[base - 1].id) : undefined;

    const full = !!config && !!ov && config.max_reg_num > 0 && ov.adopted >= config.max_reg_num;
    const notice = full && (
        <p className="flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs text-text-tertiary">
            <Info size={13} className="shrink-0 text-warning" />
            收下的到上限了（{config.max_reg_num} 张）：
            {config.do_replace ? '再收新的会顶掉旧的。' : '麦麦不会再收新的。'}
            <button
                type="button"
                className="text-brand hover:underline"
                onClick={() => onGoTab('talk')}
            >
                去改上限
            </button>
        </p>
    );

    const toolbar = (
        <>
            <SearchBox
                className="w-44"
                placeholder="搜标签"
                value={search}
                onChange={resetPage(setSearch)}
            />
            <Segmented
                items={[
                    { value: 'all', label: '全部', count: ov?.total },
                    { value: 'adopted', label: '收下', count: ov?.adopted },
                    { value: 'known', label: '认识', count: ov?.known },
                    { value: 'unknown', label: '不认识', count: ov?.unknown },
                    { value: 'discarded', label: '丢弃', count: ov?.discarded },
                ]}
                value={filter}
                onChange={resetPage(setFilter)}
            />
            <span className="flex-1" />
            <Select
                className="w-28 [&_button]:h-8 [&_button]:min-h-8 [&_button]:text-[12.5px]"
                items={SORTS}
                value={sort}
                onValueChange={resetPage(setSort)}
            />
            <UploadButton onClick={() => void pickFiles()} />
        </>
    );

    const bulk = (m: EmojiMove) => () => void move(pickedIds, m).then(sel.clear);

    return (
        <ResourcePane
            toolbar={toolbar}
            notice={notice}
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
                <>
                    <SelectionBar
                        count={sel.picked.size}
                        onClear={sel.clear}
                        onSelectAll={pageAllPicked ? undefined : () => sel.setAll(pageIds, true)}
                    >
                        {filter === 'discarded' ? (
                            <Button
                                size="sm"
                                variant="ghost"
                                disabled={act.isPending}
                                onClick={bulk('restore')}
                            >
                                <ArchiveRestore size={13} />
                                捡回来
                            </Button>
                        ) : filter === 'adopted' ? (
                            <Button
                                size="sm"
                                variant="ghost"
                                disabled={act.isPending}
                                onClick={bulk('unadopt')}
                            >
                                <X size={13} />
                                不再发
                            </Button>
                        ) : (
                            <Button
                                size="sm"
                                variant="ghost"
                                disabled={act.isPending}
                                onClick={bulk('adopt')}
                            >
                                <Check size={13} />
                                收下
                            </Button>
                        )}
                        {filter !== 'discarded' && (
                            <Button
                                size="sm"
                                variant="ghost"
                                disabled={act.isPending}
                                onClick={bulk('discard')}
                            >
                                <X size={13} />
                                丢弃
                            </Button>
                        )}
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
                    <EmojiDropOverlay visible={dragging} />
                </>
            }
        >
            {items.length === 0 && !list.isFetching ? (
                <EmptyHint
                    icon={Smile}
                    title={
                        q || filter !== 'all'
                            ? '没有对得上的表情包'
                            : config?.steal_emoji === false
                              ? '还没有表情包。麦麦没开「自动收集」，可以先传几张，或者去「回复设置」里打开。'
                              : '还没有表情包。群里有人发表情，麦麦会自己收；也可以先传几张。'
                    }
                    action={<UploadButton onClick={() => void pickFiles()} />}
                />
            ) : (
                <div className="grid grid-cols-[repeat(auto-fill,minmax(8.5rem,1fr))] gap-2.5">
                    {items.map((e) => (
                        <EmojiTile
                            key={e.id}
                            instanceId={instance.id}
                            emoji={e}
                            selected={sel.has(e.id)}
                            selecting={sel.picked.size > 0}
                            onPick={(shift) => sel.pick(e.id, shift, pageIds)}
                            onOpen={() => setOpenId(e.id)}
                        />
                    ))}
                </div>
            )}

            <EmojiDetail
                instanceId={instance.id}
                emoji={shown}
                position={openIndex >= 0 ? { index: openIndex, total: items.length } : null}
                busy={act.isPending}
                onPrev={onPrev}
                onNext={onNext}
                onTags={(tags) => shown && act.mutate({ op: 'tag', id: shown.id, tags })}
                onMove={(m) => shown && void move([shown.id], m)}
                onDelete={() => shown && setPendingDelete([shown.id])}
                onClose={() => setOpenId(null)}
            />

            <EmojiUploadDialog
                files={files}
                tags={uploadTags}
                busy={upload.isPending}
                onTags={setUploadTags}
                onRemove={(path) =>
                    setFiles((prev) => prev?.filter((f) => f.path !== path) ?? null)
                }
                onAddMore={() => void pickFiles()}
                onCancel={() => setFiles(null)}
                onConfirm={() => {
                    const ok = (files ?? []).filter((f) => !f.problem).map((f) => f.path);
                    void upload.mutateAsync({ paths: ok, tags: uploadTags }).then((res) => {
                        setFiles(null);
                        setUploadTags([]);
                        // 回到能看见新图的地方：全部、最新、第一页
                        setFilter('all');
                        setSort('newest');
                        setPage(1);
                        const got = res.uploaded + res.existed;
                        const lines = [
                            res.existed > 0 ? `${res.existed} 张本来就有，重新收下了` : '',
                            res.failed.length > 0
                                ? `${res.failed.length} 张没传：${res.failed.map((f) => `${f.name}（${f.reason}）`).join('、')}`
                                : '',
                            got > 0 ? poolNote : '',
                        ].filter(Boolean);
                        pushInfoBar({
                            key: `maibotEmojiUpload:${instance.id}`,
                            tone: got > 0 ? 'success' : 'warning',
                            title: got > 0 ? `传上去 ${got} 张` : '一张也没传上去',
                            content: lines.join('；'),
                            autoDismissMs: res.failed.length > 0 ? 8000 : 4000,
                        });
                    });
                }}
            />

            <ConfirmDelete
                open={pendingDelete !== null}
                title={
                    pendingDelete && pendingDelete.length > 1
                        ? `删掉这 ${pendingDelete.length} 张表情包？`
                        : '删掉这张表情包？'
                }
                description="图也会一起删掉，找不回来。只是不想让麦麦发的话，丢弃就行。"
                busy={act.isPending}
                onCancel={() => setPendingDelete(null)}
                onConfirm={() => {
                    if (!pendingDelete) return;
                    void act.mutateAsync({ op: 'delete', ids: pendingDelete }).then(() => {
                        if (openId !== null && pendingDelete.includes(openId)) setOpenId(null);
                        setPendingDelete(null);
                        sel.clear();
                    });
                }}
            />
        </ResourcePane>
    );
};
