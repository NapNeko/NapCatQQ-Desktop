// 麦麦认识的人：按认识 / 不认识看，搜称呼、昵称、账号；点一行改麦麦怎么叫 TA，勾选后批量删。

import { useState } from 'react';
import { Trash2, Users } from 'lucide-react';
import { Button } from '../../../../shared/ui';
import type { AppInstance, MaiBotPersonFilter, MaiBotRuntimeStatus } from '../../../../core/ipc/types';
import { useMaiBotPersonAction, useMaiBotPersonOverview, useMaiBotPersons } from '../../../../hooks/apps/useMaiBotPersons';
import { ConfirmDelete, EmptyHint } from '../entityParts';
import { Pager, ResourcePane, SearchBox, Segmented, SelectionBar, useDebounced, useSelection } from '../resourceParts';
import { MaiBotLiveGate, maibotLive } from './MaiBotLiveGate';
import { PersonDialog, PersonRow, personLabel, type PersonDraft } from './maibotPersonParts';

const PAGE_SIZE = 20;

export const MaiBotPersonsTab: React.FC<{
    instance: AppInstance;
    status: MaiBotRuntimeStatus | undefined;
    onStart: () => void;
    starting: boolean;
}> = ({ instance, status, onStart, starting }) => {
    const live = maibotLive(status);
    const [filter, setFilter] = useState<MaiBotPersonFilter>('all');
    const [search, setSearch] = useState('');
    const [page, setPage] = useState(1);
    const [draft, setDraft] = useState<PersonDraft | null>(null);
    const [pendingDelete, setPendingDelete] = useState<{ ids: string[]; label: string } | null>(null);
    const sel = useSelection<string>();
    const q = useDebounced(search.trim());

    const list = useMaiBotPersons(instance.id, { page, page_size: PAGE_SIZE, search: q, filter }, live);
    const overview = useMaiBotPersonOverview(instance.id, live);
    const act = useMaiBotPersonAction(instance.id);

    if (!live) return <MaiBotLiveGate status={status} what="认识的人" onStart={onStart} starting={starting} />;

    const ov = overview.data;
    const items = list.data?.items ?? [];
    const pageIds = items.map((p) => p.person_id);
    const pageAllPicked = pageIds.every((id) => sel.has(id));
    const resetPage = <T,>(set: (v: T) => void) => (v: T) => {
        set(v);
        setPage(1);
        sel.clear();
    };

    const toolbar = (
        <>
            <SearchBox className="w-64" placeholder="搜称呼、昵称、账号" value={search} onChange={resetPage(setSearch)} />
            <Segmented
                items={[
                    { value: 'all', label: '全部', count: ov?.total },
                    { value: 'known', label: '认识', count: ov?.known },
                    { value: 'unknown', label: '不认识', count: ov?.unknown },
                ]}
                value={filter}
                onChange={resetPage(setFilter)}
            />
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
                    <Button
                        size="sm"
                        variant="ghost"
                        className="text-danger hover:bg-danger-soft hover:text-danger"
                        disabled={act.isPending}
                        onClick={() => setPendingDelete({ ids: [...sel.picked], label: `这 ${sel.picked.size} 个人` })}
                    >
                        <Trash2 size={13} />
                        删除
                    </Button>
                </SelectionBar>
            }
        >
            {items.length === 0 && !list.isFetching ? (
                <EmptyHint
                    icon={Users}
                    title={q || filter !== 'all' ? '没有对得上的人' : '还没有人跟麦麦说过话。有人在群里或私聊里跟它聊过，这里就会有 TA。'}
                />
            ) : (
                <div className="flex flex-col gap-1.5">
                    {items.map((p) => (
                        <PersonRow
                            key={p.person_id}
                            person={p}
                            selected={sel.has(p.person_id)}
                            busy={act.isPending}
                            onPick={(shift) => sel.pick(p.person_id, shift, pageIds)}
                            onEdit={() => setDraft({ person: p, name: p.name, name_reason: p.name_reason, is_known: p.is_known })}
                            onDelete={() => setPendingDelete({ ids: [p.person_id], label: personLabel(p) })}
                        />
                    ))}
                </div>
            )}

            <PersonDialog
                draft={draft}
                busy={act.isPending}
                onChange={setDraft}
                onCancel={() => setDraft(null)}
                onConfirm={() => {
                    if (!draft) return;
                    void act
                        .mutateAsync({
                            op: 'update',
                            person_id: draft.person.person_id,
                            name: draft.name,
                            name_reason: draft.name_reason,
                            is_known: draft.is_known,
                        })
                        .then(() => setDraft(null));
                }}
            />

            <ConfirmDelete
                open={pendingDelete !== null}
                title={`忘掉${pendingDelete?.label ?? ''}？`}
                description="麦麦会忘了怎么称呼。TA 下次说话，麦麦会重新认识，称呼按昵称重来。"
                confirmLabel="忘掉"
                busy={act.isPending}
                onCancel={() => setPendingDelete(null)}
                onConfirm={() => {
                    if (!pendingDelete) return;
                    void act.mutateAsync({ op: 'delete', person_ids: pendingDelete.ids }).then(() => {
                        setPendingDelete(null);
                        sel.clear();
                    });
                }}
            />
        </ResourcePane>
    );
};
