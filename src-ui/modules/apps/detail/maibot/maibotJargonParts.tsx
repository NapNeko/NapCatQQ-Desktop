// 黑话页的零件：一行一条（词、含义、遇见次数、全局 / 固定标记）、新建 / 编辑对话框。

import { Globe, Pencil, Pin, Trash2 } from 'lucide-react';
import { Badge, Button, Switch, TextAreaField, TextField } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import type { MaiBotJargon, MaiBotLearningChat } from '../../../../core/ipc/types';
import { FormDialog, PickList } from '../entityParts';
import { RowCheck } from '../resourceParts';

export const JargonRow: React.FC<{
    item: MaiBotJargon;
    selected: boolean;
    busy: boolean;
    onPick: (shift: boolean) => void;
    onEdit: () => void;
    onDelete: () => void;
}> = ({ item, selected, busy, onPick, onEdit, onDelete }) => {
    const used = item.is_jargon && !!item.meaning;
    return (
        <div
            className={cn(
                'group flex items-center gap-2.5 rounded-md border px-3 py-2.5 transition-colors',
                selected
                    ? 'border-brand/40 bg-brand-soft/30'
                    : 'border-border-subtle bg-surface hover:border-border',
            )}
        >
            <RowCheck checked={selected} onPick={onPick} />
            <button
                type="button"
                onClick={onEdit}
                className="min-w-0 flex-1 text-left focus-visible:outline-none"
            >
                <span className="flex min-w-0 items-center gap-1.5">
                    <span
                        className={cn(
                            'truncate text-[13.5px] font-medium',
                            used ? 'text-text' : 'text-text-secondary',
                        )}
                    >
                        {item.content}
                    </span>
                    {item.is_global && (
                        <Globe size={12} className="shrink-0 text-info" aria-label="全局" />
                    )}
                    {item.pinned && (
                        <Pin size={12} className="shrink-0 text-brand" aria-label="固定含义" />
                    )}
                    {!used && (
                        <Badge tone="neutral" className="shrink-0">
                            {item.meaning ? '不算黑话' : '还没推出含义'}
                        </Badge>
                    )}
                </span>
                {item.meaning && (
                    <span className="mt-0.5 block truncate text-xs text-text-secondary">
                        {item.meaning}
                    </span>
                )}
            </button>
            <span className="hidden shrink-0 text-right text-2xs text-text-tertiary sm:block">
                <span className="block max-w-[12rem] truncate">
                    {item.is_global ? '所有聊天' : item.chat_names.join('、')}
                </span>
                <span className="block">遇见 {item.count} 次</span>
            </span>
            <div className="flex shrink-0 items-center opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
                <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7"
                    aria-label="编辑"
                    onClick={onEdit}
                >
                    <Pencil size={13} />
                </Button>
                <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 text-danger hover:text-danger"
                    aria-label="删除"
                    disabled={busy}
                    onClick={onDelete}
                >
                    <Trash2 size={13} />
                </Button>
            </div>
        </div>
    );
};

export type JargonDraft = {
    id?: number;
    content: string;
    meaning: string;
    chat_ids: string[];
    is_global: boolean;
    is_jargon: boolean;
    pinned: boolean;
};

export const JargonDialog: React.FC<{
    draft: JargonDraft | null;
    chats: readonly MaiBotLearningChat[];
    busy: boolean;
    onChange: (next: JargonDraft) => void;
    onCancel: () => void;
    onConfirm: () => void;
}> = ({ draft, chats, busy, onChange, onCancel, onConfirm }) => {
    if (!draft) return null;
    const editing = draft.id !== undefined;
    const set = (patch: Partial<JargonDraft>) => onChange({ ...draft, ...patch });
    const ok = !!draft.content.trim() && draft.chat_ids.length > 0;
    return (
        <FormDialog
            open
            size="md"
            title={editing ? '改黑话' : '加一条黑话'}
            description="麦麦看到这个词，会按这里的含义去理解。"
            confirmLabel={editing ? '保存' : '加上'}
            confirmDisabled={!ok}
            busy={busy}
            onCancel={onCancel}
            onConfirm={onConfirm}
        >
            <TextField
                label="词"
                autoFocus
                placeholder="比如：yyds"
                value={draft.content}
                onValueChange={(content) => set({ content })}
            />
            <TextAreaField
                label="含义"
                minRows={2}
                placeholder="比如：永远的神，夸某样东西特别好"
                value={draft.meaning}
                onValueChange={(meaning) => set({ meaning })}
            />
            <Switch
                label="全局"
                hint="所有聊天都按这个意思理解"
                checked={draft.is_global}
                onCheckedChange={(is_global) => set({ is_global })}
            />
            {/* 上游每条都要挂在至少一个聊天上，全局的也是；全局时这里只管归档，不管在哪用 */}
            <PickList
                label={draft.is_global ? '记在哪些聊天下' : '用在哪些聊天里'}
                options={chats.map((c) => ({ value: c.chat_id, label: c.chat_name }))}
                value={draft.chat_ids}
                onChange={(chat_ids) => set({ chat_ids })}
                empty="麦麦还没见过聊天"
                hint={draft.chat_ids.length === 0 ? '至少挑一个' : undefined}
            />
            {editing && (
                <>
                    <Switch
                        label="算黑话"
                        hint="关掉后麦麦不再按这个意思理解它"
                        checked={draft.is_jargon}
                        onCheckedChange={(is_jargon) => set({ is_jargon })}
                    />
                    <Switch
                        label="固定含义"
                        hint="麦麦以后不会再自己改这条的含义"
                        checked={draft.pinned}
                        onCheckedChange={(pinned) => set({ pinned })}
                    />
                </>
            )}
        </FormDialog>
    );
};
