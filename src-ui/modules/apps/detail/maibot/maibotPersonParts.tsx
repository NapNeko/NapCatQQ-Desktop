// 人物页的零件：头像、一行一人（称呼、昵称、账号、为什么这么叫）、改称呼的对话框。

import { useState } from 'react';
import { Pencil, Trash2 } from 'lucide-react';
import { Badge, Button, Switch, TextAreaField, TextField } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import type { MaiBotPerson } from '../../../../core/ipc/types';
import { FormDialog } from '../entityParts';
import { RowCheck } from '../resourceParts';
import { relativeTime } from './maibotPromptParts';

const QQ_LIKE = new Set(['qq', 'qqguild', 'napcat']);
const PALETTES = [
    'from-pink-300 to-rose-400',
    'from-amber-300 to-orange-400',
    'from-emerald-300 to-teal-400',
    'from-sky-300 to-indigo-400',
    'from-violet-300 to-fuchsia-400',
];

function paletteFor(key: string): string {
    let h = 0;
    for (const ch of key) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
    return PALETTES[h % PALETTES.length];
}

// 第二行没内容时也占住一行高，各行对齐
const NBSP = ' ';

/** 麦麦叫 TA 什么；还没起就用昵称 */
export const personLabel = (p: MaiBotPerson) => p.name || p.nickname || p.user_id;

/**
 * QQ 系的账号拉 qlogo 头像（和机器人卡片同一个源）；号码得像真 QQ 号，不以 0 开头。
 * 别的平台、拉不到的用首字
 */
export const PersonAvatar: React.FC<{ person: MaiBotPerson; size?: 'sm' | 'lg' }> = ({ person, size = 'sm' }) => {
    const [failed, setFailed] = useState(false);
    const qq = QQ_LIKE.has(person.platform.toLowerCase()) && /^[1-9]\d{4,11}$/.test(person.user_id);
    const initial = (personLabel(person).trim().charAt(0) || '?').toUpperCase();
    return (
        <span
            className={cn(
                'inline-flex shrink-0 items-center justify-center overflow-hidden rounded-full bg-gradient-to-br ring-1 ring-border-subtle',
                size === 'lg' ? 'h-12 w-12 text-lg' : 'h-9 w-9 text-sm',
                paletteFor(person.user_id || person.person_id),
            )}
        >
            {qq && !failed ? (
                <img
                    src={`https://q.qlogo.cn/headimg_dl?dst_uin=${person.user_id}&spec=100`}
                    alt=""
                    className="h-full w-full object-cover"
                    referrerPolicy="no-referrer"
                    draggable={false}
                    loading="lazy"
                    onError={() => setFailed(true)}
                />
            ) : (
                <span className="font-display font-semibold text-white/95">{initial}</span>
            )}
        </span>
    );
};

export const PersonRow: React.FC<{
    person: MaiBotPerson;
    selected: boolean;
    busy: boolean;
    onPick: (shift: boolean) => void;
    onEdit: () => void;
    onDelete: () => void;
}> = ({ person: p, selected, busy, onPick, onEdit, onDelete }) => (
    <div
        className={cn(
            'group flex items-center gap-3 rounded-md border px-3 py-2.5 transition-colors',
            selected ? 'border-brand/40 bg-brand-soft/30' : 'border-border-subtle bg-surface hover:border-border',
        )}
    >
        <RowCheck checked={selected} onPick={onPick} />
        <button type="button" onClick={onEdit} className="flex min-w-0 flex-1 items-center gap-3 text-left focus-visible:outline-none">
            <PersonAvatar person={p} />
            <span className="min-w-0 flex-1">
                <span className="flex min-w-0 items-center gap-1.5">
                    <span className={cn('truncate text-[13.5px] font-medium', p.is_known ? 'text-text' : 'text-text-secondary')}>
                        {personLabel(p)}
                    </span>
                    {p.name && p.nickname && p.nickname !== p.name && (
                        <span className="truncate text-xs text-text-tertiary">{p.nickname}</span>
                    )}
                    {!p.is_known && (
                        <Badge tone="neutral" className="shrink-0">
                            不认识
                        </Badge>
                    )}
                </span>
                <span className="mt-0.5 block truncate text-xs text-text-tertiary">
                    {p.name_reason || (p.group_cards.length > 0 ? `群名片：${p.group_cards.map((g) => g.card).join('、')}` : NBSP)}
                </span>
            </span>
        </button>
        <span className="hidden shrink-0 text-right text-2xs text-text-tertiary sm:block">
            <span className="block font-mono">
                {p.platform} {p.user_id}
            </span>
            <span className="block">{p.last_seen ? `${relativeTime(p.last_seen)}见过` : ''}</span>
        </span>
        <div className="flex shrink-0 items-center opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
            <Button variant="ghost" size="icon" className="h-7 w-7" aria-label="编辑" onClick={onEdit}>
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

export type PersonDraft = { person: MaiBotPerson; name: string; name_reason: string; is_known: boolean };

export const PersonDialog: React.FC<{
    draft: PersonDraft | null;
    busy: boolean;
    onChange: (next: PersonDraft) => void;
    onCancel: () => void;
    onConfirm: () => void;
}> = ({ draft, busy, onChange, onCancel, onConfirm }) => {
    if (!draft) return null;
    const p = draft.person;
    const set = (patch: Partial<PersonDraft>) => onChange({ ...draft, ...patch });
    return (
        <FormDialog
            open
            size="md"
            title={personLabel(p)}
            confirmLabel="保存"
            busy={busy}
            onCancel={onCancel}
            onConfirm={onConfirm}
        >
            <div className="flex items-center gap-3 rounded-md bg-inset/60 px-3 py-2.5">
                <PersonAvatar person={p} size="lg" />
                <div className="min-w-0 flex-1 text-xs text-text-tertiary">
                    <p className="truncate text-sm text-text">{p.nickname || '没有昵称'}</p>
                    <p className="mt-0.5 font-mono">
                        {p.platform} {p.user_id}
                    </p>
                    <p className="mt-0.5">
                        {[p.first_seen && `首次 ${relativeTime(p.first_seen)}`, p.last_seen && `最近 ${relativeTime(p.last_seen)}`]
                            .filter(Boolean)
                            .join(' · ')}
                    </p>
                </div>
            </div>
            <TextField
                label="麦麦怎么叫 TA"
                placeholder={p.nickname || '不填就叫昵称'}
                value={draft.name}
                onValueChange={(name) => set({ name })}
            />
            <TextAreaField
                label="为什么这么叫"
                minRows={2}
                placeholder="比如：群里大家都这么叫她"
                value={draft.name_reason}
                onValueChange={(name_reason) => set({ name_reason })}
            />
            <Switch
                label="认识 TA"
                hint="关掉后麦麦把 TA 当陌生人；TA 再说话，麦麦会重新认识"
                checked={draft.is_known}
                onCheckedChange={(is_known) => set({ is_known })}
            />
            {p.group_cards.length > 0 && (
                <div className="flex flex-col gap-1.5">
                    <span className="text-xs font-medium text-text-secondary">群名片</span>
                    <div className="flex flex-wrap gap-1.5">
                        {p.group_cards.map((g) => (
                            <span key={`${g.group_id}:${g.card}`} className="rounded-pill bg-inset px-2 py-0.5 text-xs text-text-secondary">
                                {g.card}
                                <span className="ml-1 font-mono text-2xs text-text-tertiary">{g.group_id}</span>
                            </span>
                        ))}
                    </div>
                </div>
            )}
        </FormDialog>
    );
};
