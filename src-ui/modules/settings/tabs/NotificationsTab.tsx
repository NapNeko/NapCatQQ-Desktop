// 通知设置：掉线时如何通知你。
// 主列表只留开关 + 通道摘要；通道详情 / 消息模板收敛进 Dialog（对齐连接配置交互）。
// 颜色只走语义 token，随主题切换。

import { useEffect, useRef, useState, type ReactNode } from 'react';
import { Eye, EyeOff, History, Pencil, Plus, Trash2, X } from 'lucide-react';
import { DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED } from '../../../core/domain/ui/infoBarDismiss';
import {
    createBlankWebhookChannel,
    DEFAULT_ONEBOT_MESSAGE,
    WEBHOOK_PRESETS,
    type WebhookChannelDraft,
    type WebhookPresetId,
} from '../../../core/domain/settings/offline-notify-defaults';
import {
    detectWebhookService,
    fieldsFromPresetBody,
    parseVisualFields,
    serializeVisualFields,
    TEMPLATE_VARS,
} from '../../../core/domain/settings/webhook-message-visual';
import { settingsService } from '../../../core/services/settings.service';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import {
    Badge,
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    NumberField,
    Select,
    Switch,
    TextField,
} from '../../../shared/ui';
import { ActionMotionIcon } from '../../../shared/ui/motion';
import { cn } from '../../../shared/utils/cn';
import type { SettingsDraft } from '../settings-draft';
import {
    FieldRow,
    InfoBarDismissDurationSlider,
    InfoBarDismissSliderPresence,
    SettingsSection,
    SettingsTabSections,
} from '../_shared';
import { WebhookMessageBuilder } from './WebhookMessageBuilder';

interface Props {
    draft: SettingsDraft | null;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
}

type ChannelEditorState =
    | { mode: 'create'; draft: WebhookChannelDraft }
    | { mode: 'edit'; id: string; draft: WebhookChannelDraft };

const METHOD_ITEMS = [
    { value: 'POST', label: 'POST' },
    { value: 'GET', label: 'GET' },
] as const;

const ENCRYPTION_ITEMS = [
    { value: 'SSL', label: 'SSL' },
    { value: 'TLS', label: 'TLS' },
    { value: '无加密', label: '无加密' },
] as const;

const ONEBOT_TARGET_ITEMS = [
    { value: 'private', label: '私聊' },
    { value: 'group', label: '群聊' },
] as const;

const EMAIL_PRESETS = [
    { label: 'QQ 邮箱', server: 'smtp.qq.com', port: 465, encryption: 'SSL' },
    { label: '163 邮箱', server: 'smtp.163.com', port: 465, encryption: 'SSL' },
    { label: 'Gmail', server: 'smtp.gmail.com', port: 465, encryption: 'SSL' },
    {
        label: 'Outlook',
        server: 'smtp.office365.com',
        port: 587,
        encryption: 'TLS',
    },
] as const;

const EMAIL_PRESET_ITEMS = EMAIL_PRESETS.map((preset) => ({
    value: preset.label,
    label: preset.label,
}));

type EmailEditorDraft = Pick<
    SettingsDraft,
    | 'emailSender'
    | 'emailReceiver'
    | 'emailToken'
    | 'emailSmtpServer'
    | 'emailSmtpPort'
    | 'emailEncryption'
>;

type OneBotEditorDraft = Pick<
    SettingsDraft,
    | 'onebotMessengerBotIds'
    | 'onebotTargetType'
    | 'onebotTargetIds'
    | 'onebotMessageTemplate'
>;

/** 左侧服务卡片副文案：尽量短 */
const PRESET_HINTS: Record<WebhookPresetId, string> = {
    serverchan: 'title / desp',
    dingtalk: 'markdown',
    feishu: 'text',
    discord: 'Embed',
    bark: '推送',
};

function newChannelId(existing: WebhookChannelDraft[]): string {
    const stamp = Date.now().toString(36);
    let n = existing.length + 1;
    let id = `channel-${n}-${stamp}`;
    while (existing.some((c) => c.id === id)) {
        n += 1;
        id = `channel-${n}-${stamp}`;
    }
    return id;
}

function channelDisplayName(ch: WebhookChannelDraft): string {
    const n = ch.name.trim();
    if (n) return n;
    if (ch.url.trim()) {
        try {
            return new URL(ch.url).hostname || ch.id;
        } catch {
            return ch.url.trim().slice(0, 28) || ch.id;
        }
    }
    return '未命名通道';
}

function channelSummary(ch: WebhookChannelDraft): string {
    if (ch.url.trim()) {
        try {
            return new URL(ch.url).host;
        } catch {
            return ch.url.trim();
        }
    }
    return '未填写地址';
}

function emailIsReady(email: EmailEditorDraft): boolean {
    return Boolean(
        email.emailSender.trim() &&
        email.emailReceiver.trim() &&
        email.emailToken.trim() &&
        email.emailSmtpServer.trim() &&
        email.emailSmtpPort > 0,
    );
}

function emailSummary(email: EmailEditorDraft): string {
    if (!email.emailSmtpServer.trim()) return '尚未选择邮箱服务';
    if (!email.emailSender.trim() || !email.emailReceiver.trim()) {
        return `${email.emailSmtpServer} · 待填写收发地址`;
    }
    if (!email.emailToken.trim()) return `${email.emailSmtpServer} · 缺授权码`;
    return `${email.emailSender.trim()} → ${email.emailReceiver.trim()}`;
}

function oneBotIsReady(oneBot: OneBotEditorDraft): boolean {
    return (
        oneBot.onebotMessengerBotIds.some((id) => id.trim()) &&
        oneBot.onebotTargetIds.some((id) => id > 0)
    );
}

function oneBotSummary(oneBot: OneBotEditorDraft): string {
    const messengers = oneBot.onebotMessengerBotIds.filter((id) => id.trim());
    const targets = oneBot.onebotTargetIds.filter((id) => id > 0);
    if (messengers.length === 0) return '尚未选择发送方 Bot';
    if (targets.length === 0) return '待填写接收目标';
    const messengerLabel =
        messengers.length === 1
            ? messengers[0]
            : `${messengers[0]} 等 ${messengers.length} 个`;
    const targetLabel =
        targets.length === 1
            ? String(targets[0])
            : `${targets[0]} 等 ${targets.length} 个`;
    return `${messengerLabel} → ${oneBot.onebotTargetType === 'group' ? '群' : '私聊'} ${targetLabel}`;
}

function parseTargetTokens(raw: string): number[] {
    return raw
        .split(/[,，;；\s]+/)
        .map((part) => Number(part.trim()))
        .filter((n) => Number.isFinite(n) && n > 0)
        .map((n) => Math.round(n));
}

function historyKindLabel(kind: string): string {
    switch (kind) {
        case 'auto_restart':
        case 'manual':
            return '掉线';
        case 'kicked':
            return '被踢下线';
        case 'process_crashed':
            return '异常退出';
        case 'recovered':
            return '恢复上线';
        default:
            return kind;
    }
}

function DeliveryResultBadge({
    label,
    value,
}: {
    label: string;
    value: 'ok' | 'failed' | 'skipped';
}) {
    if (value === 'ok') {
        return <Badge tone="success" appearance="soft">{label} 已发送</Badge>;
    }
    if (value === 'failed') {
        return <Badge tone="danger" appearance="soft">{label} 失败</Badge>;
    }
    return <Badge tone="neutral" appearance="soft">{label} 未投递</Badge>;
}

function SecretField({
    value,
    onValueChange,
    className,
    placeholder,
    name,
}: {
    value: string;
    onValueChange: (v: string) => void;
    className?: string;
    placeholder?: string;
    name?: string;
}) {
    const [reveal, setReveal] = useState(false);
    return (
        <div className={cn('relative w-full min-w-0', className)}>
            <input
                type={reveal ? 'text' : 'password'}
                name={name}
                autoComplete="off"
                spellCheck={false}
                value={value}
                placeholder={placeholder}
                onChange={(e) => onValueChange(e.target.value)}
                className={cn(
                    'block w-full rounded-sm bg-field py-2 pl-3 pr-9 text-sm text-text',
                    'border border-border-subtle outline-none transition-colors duration-150',
                    'placeholder:text-text-tertiary',
                    'disabled:cursor-not-allowed disabled:bg-inset disabled:text-text-disabled',
                    'focus:border-brand focus:ring-2 focus:ring-brand focus:ring-inset',
                )}
            />
            <button
                type="button"
                onClick={() => setReveal((r) => !r)}
                className="absolute right-1 top-1/2 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-sm text-text-tertiary transition-colors hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                aria-label={reveal ? '隐藏密钥' : '显示密钥'}
            >
                {reveal ? (
                    <ActionMotionIcon icon={EyeOff} size={15} />
                ) : (
                    <ActionMotionIcon icon={Eye} size={15} />
                )}
            </button>
        </div>
    );
}

function renderTemplatePreview(template: string): string {
    return TEMPLATE_VARS.reduce(
        (text, variable) =>
            text.replaceAll(`{${variable.key}}`, variable.sample),
        template,
    );
}

function Chip({
    label,
    onRemove,
    tone = 'neutral',
}: {
    label: string;
    onRemove: () => void;
    tone?: 'neutral' | 'success' | 'warning';
}) {
    return (
        <span
            className={cn(
                'inline-flex max-w-full items-center gap-1 rounded-sm border px-2 py-1 text-[12px]',
                tone === 'success' &&
                'border-success/30 bg-success-soft text-text',
                tone === 'warning' &&
                'border-warning/30 bg-warning-soft text-text',
                tone === 'neutral' &&
                'border-border-subtle bg-inset text-text-secondary',
            )}
        >
            <span className="truncate">{label}</span>
            <button
                type="button"
                aria-label={`移除 ${label}`}
                onClick={onRemove}
                className="rounded-xs p-0.5 text-text-tertiary transition-colors hover:bg-field hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
            >
                <X size={12} />
            </button>
        </span>
    );
}

function TargetIdChipsInput({
    values,
    onChange,
    placeholder,
    className,
}: {
    values: number[];
    onChange: (next: number[]) => void;
    placeholder?: string;
    className?: string;
}) {
    const [draft, setDraft] = useState('');
    const inputRef = useRef<HTMLInputElement | null>(null);

    const commitDraft = (raw: string) => {
        const tokens = parseTargetTokens(raw);
        if (tokens.length === 0) {
            setDraft('');
            return;
        }
        const next = [...values];
        for (const token of tokens) {
            if (!next.includes(token)) next.push(token);
        }
        onChange(next);
        setDraft('');
    };

    return (
        <div
            className={cn(
                'flex min-h-10 w-full content-start items-start gap-1.5 overflow-y-auto rounded-sm border border-border-subtle bg-field px-2 py-1.5',
                'flex-wrap focus-within:border-brand focus-within:ring-2 focus-within:ring-brand focus-within:ring-inset',
                className,
            )}
            onClick={() => inputRef.current?.focus()}
        >
            {values.map((id) => (
                <Chip
                    key={id}
                    label={String(id)}
                    onRemove={() => onChange(values.filter((item) => item !== id))}
                />
            ))}
            <input
                ref={inputRef}
                value={draft}
                placeholder={values.length === 0 ? placeholder : '继续输入，逗号确认'}
                onChange={(event) => {
                    const next = event.target.value;
                    if (/[,，;；\n]/.test(next)) {
                        commitDraft(next);
                        return;
                    }
                    setDraft(next);
                }}
                onKeyDown={(event) => {
                    if (event.key === 'Enter' || event.key === ',' || event.key === '，') {
                        event.preventDefault();
                        commitDraft(draft);
                        return;
                    }
                    if (event.key === 'Backspace' && !draft && values.length > 0) {
                        onChange(values.slice(0, -1));
                    }
                }}
                onBlur={() => {
                    if (draft.trim()) commitDraft(draft);
                }}
                className="min-w-[9rem] flex-1 bg-transparent py-0.5 text-sm text-text outline-none placeholder:text-text-tertiary"
            />
        </div>
    );
}

type OneBotCandidate = Awaited<
    ReturnType<typeof settingsService.listOneBotMessengerCandidates>
>[number];

function backendLabel(backend: string): string {
    return backend === 'snowluma' ? 'SnowLuma' : 'NapCat';
}

function stateLabel(state: string): string {
    switch (state) {
        case 'running':
            return '运行中';
        case 'starting':
            return '启动中';
        case 'stopping':
            return '停止中';
        case 'crashed':
            return '异常退出';
        case 'repairing':
            return '修复中';
        default:
            return '已停止';
    }
}

function OneBotMessengerPicker({
    selected,
    candidates,
    loading,
    enablingId,
    onChange,
    onEnsureHttp,
}: {
    selected: string[];
    candidates: OneBotCandidate[];
    loading: boolean;
    enablingId: string | null;
    onChange: (next: string[]) => void;
    onEnsureHttp: (botId: string) => void;
}) {
    const [query, setQuery] = useState('');
    const selectedSet = new Set(selected);
    const selectedMissing = selected.filter(
        (id) => !candidates.some((item) => item.bot_id === id),
    );
    const eligibleCount = candidates.filter((item) => item.eligible).length;

    const filtered = (() => {
        const q = query.trim().toLowerCase();
        const list = !q
            ? candidates
            : candidates.filter((item) => {
                const hay =
                    `${item.name} ${item.bot_id} ${item.backend_type} ${item.state}`.toLowerCase();
                return hay.includes(q);
            });
        return [...list].sort((a, b) => {
            const aSelected = selectedSet.has(a.bot_id) ? 1 : 0;
            const bSelected = selectedSet.has(b.bot_id) ? 1 : 0;
            if (aSelected !== bSelected) return bSelected - aSelected;
            if (a.eligible !== b.eligible) return Number(b.eligible) - Number(a.eligible);
            if (a.has_local_http !== b.has_local_http) {
                return Number(b.has_local_http) - Number(a.has_local_http);
            }
            const aRunning = a.state === 'running' ? 1 : 0;
            const bRunning = b.state === 'running' ? 1 : 0;
            if (aRunning !== bRunning) return bRunning - aRunning;
            return (a.name || a.bot_id).localeCompare(b.name || b.bot_id, 'zh-CN');
        });
    })();

    const toggle = (botId: string) => {
        if (selectedSet.has(botId)) {
            onChange(selected.filter((id) => id !== botId));
            return;
        }
        onChange([...selected, botId]);
    };

    const moveSelected = (botId: string, direction: -1 | 1) => {
        const index = selected.indexOf(botId);
        if (index < 0) return;
        const nextIndex = index + direction;
        if (nextIndex < 0 || nextIndex >= selected.length) return;
        const next = [...selected];
        const [item] = next.splice(index, 1);
        next.splice(nextIndex, 0, item);
        onChange(next);
    };

    return (
        <section className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-sm border border-border-subtle bg-field">
            <div className="shrink-0 space-y-2.5 border-b border-border-subtle px-3 py-3">
                <div className="flex items-center justify-between gap-2">
                    <div className="min-w-0">
                        <div className="flex items-center gap-2">
                            <p className="text-[13px] font-medium text-text">
                                发送方 Bot
                            </p>
                            <Badge tone="neutral" appearance="soft">
                                已选 {selected.length}
                            </Badge>
                        </div>
                        <p className="mt-0.5 text-[11.5px] leading-relaxed text-text-tertiary">
                            多选时按顺序尝试；当前掉线的会自动跳过
                            {eligibleCount > 0
                                ? ` · ${eligibleCount} 个可发送`
                                : ''}
                        </p>
                    </div>
                </div>

                {selected.length > 0 ? (
                    <div className="flex flex-wrap gap-1.5">
                        {selected.map((id, index) => {
                            const candidate = candidates.find(
                                (item) => item.bot_id === id,
                            );
                            const label = candidate?.name || id;
                            return (
                                <span
                                    key={id}
                                    className={cn(
                                        'inline-flex max-w-full items-center gap-1 rounded-sm border px-1.5 py-1 text-[11.5px]',
                                        candidate?.eligible
                                            ? 'border-success/30 bg-success-soft text-text'
                                            : 'border-warning/30 bg-warning-soft text-text',
                                    )}
                                >
                                    <span className="shrink-0 rounded-xs bg-field/70 px-1 py-px font-mono text-[10px] text-text-tertiary">
                                        {index + 1}
                                    </span>
                                    <span className="truncate">{label}</span>
                                    {selected.length > 1 ? (
                                        <span className="flex shrink-0 items-center">
                                            <button
                                                type="button"
                                                aria-label={`上移 ${label}`}
                                                disabled={index === 0}
                                                onClick={() => moveSelected(id, -1)}
                                                className="rounded-xs px-0.5 text-text-tertiary transition-colors hover:text-text disabled:opacity-30"
                                            >
                                                ↑
                                            </button>
                                            <button
                                                type="button"
                                                aria-label={`下移 ${label}`}
                                                disabled={index === selected.length - 1}
                                                onClick={() => moveSelected(id, 1)}
                                                className="rounded-xs px-0.5 text-text-tertiary transition-colors hover:text-text disabled:opacity-30"
                                            >
                                                ↓
                                            </button>
                                        </span>
                                    ) : null}
                                    <button
                                        type="button"
                                        aria-label={`移除 ${label}`}
                                        onClick={() =>
                                            onChange(
                                                selected.filter((item) => item !== id),
                                            )
                                        }
                                        className="rounded-xs p-0.5 text-text-tertiary transition-colors hover:bg-field hover:text-text"
                                    >
                                        <X size={11} />
                                    </button>
                                </span>
                            );
                        })}
                    </div>
                ) : (
                    <div className="rounded-sm border border-dashed border-border-subtle bg-inset/25 px-2.5 py-2 text-[11.5px] text-text-tertiary">
                        从下方列表勾选至少一个本机 Bot 作为发送方
                    </div>
                )}

                <TextField
                    name="onebot-messenger-search"
                    value={query}
                    placeholder="搜索名称 / QQ / 后端"
                    onValueChange={setQuery}
                />
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto">
                {loading ? (
                    <p className="px-3 py-6 text-center text-[12px] text-text-tertiary">
                        正在加载本机 Bot…
                    </p>
                ) : filtered.length === 0 && selectedMissing.length === 0 ? (
                    <div className="flex h-full min-h-[10rem] flex-col items-center justify-center gap-1 px-4 py-8 text-center">
                        <p className="text-[13px] text-text-secondary">
                            {candidates.length === 0
                                ? '还没有本机 Bot'
                                : '没有匹配的 Bot'}
                        </p>
                        <p className="text-[11.5px] text-text-tertiary">
                            {candidates.length === 0
                                ? '先在 Bot 列表添加本机实例，再回来配置通知'
                                : '试试换个关键词'}
                        </p>
                    </div>
                ) : (
                    <ul className="divide-y divide-border-subtle/70">
                        {filtered.map((candidate) => {
                            const checked = selectedSet.has(candidate.bot_id);
                            const enabling = enablingId === candidate.bot_id;
                            const order = selected.indexOf(candidate.bot_id);
                            return (
                                <li
                                    key={candidate.bot_id}
                                    className={cn(
                                        'group relative',
                                        checked && 'bg-brand/6',
                                    )}
                                >
                                    <div className="flex items-stretch gap-0">
                                        <button
                                            type="button"
                                            onClick={() => toggle(candidate.bot_id)}
                                            className="flex min-w-0 flex-1 items-start gap-2.5 px-3 py-2.5 text-left transition-colors hover:bg-inset/40"
                                        >
                                            <span
                                                className={cn(
                                                    'mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-xs border text-[10px] font-medium',
                                                    checked
                                                        ? 'border-brand bg-brand text-white'
                                                        : 'border-border-subtle bg-field text-transparent',
                                                )}
                                                aria-hidden
                                            >
                                                {checked && order >= 0
                                                    ? order + 1
                                                    : '✓'}
                                            </span>
                                            <div className="min-w-0 flex-1">
                                                <div className="flex min-w-0 items-center gap-1.5">
                                                    <span className="truncate text-[13px] font-medium text-text">
                                                        {candidate.name ||
                                                            candidate.bot_id}
                                                    </span>
                                                    <Badge
                                                        tone="neutral"
                                                        appearance="soft"
                                                    >
                                                        {backendLabel(
                                                            candidate.backend_type,
                                                        )}
                                                    </Badge>
                                                </div>
                                                <p className="mt-0.5 truncate text-[11px] text-text-tertiary">
                                                    {candidate.bot_id}
                                                    {' · '}
                                                    {stateLabel(candidate.state)}
                                                    {candidate.has_local_http
                                                        ? ` · :${candidate.http_port || '?'}`
                                                        : ' · 缺 HTTP'}
                                                </p>
                                            </div>
                                        </button>

                                        <div className="flex shrink-0 items-center pr-3">
                                            {candidate.eligible ? (
                                                <Badge
                                                    tone="success"
                                                    appearance="soft"
                                                >
                                                    可发送
                                                </Badge>
                                            ) : candidate.can_enable_http ? (
                                                <Button
                                                    type="button"
                                                    variant="secondary"
                                                    size="sm"
                                                    disabled={enabling}
                                                    onClick={() =>
                                                        onEnsureHttp(
                                                            candidate.bot_id,
                                                        )
                                                    }
                                                >
                                                    {enabling
                                                        ? '配置中…'
                                                        : '启用 HTTP'}
                                                </Button>
                                            ) : (
                                                <Badge
                                                    tone="warning"
                                                    appearance="soft"
                                                >
                                                    暂不可用
                                                </Badge>
                                            )}
                                        </div>
                                    </div>
                                </li>
                            );
                        })}
                        {selectedMissing.map((id) => (
                            <li
                                key={`missing-${id}`}
                                className="flex items-center justify-between gap-2 px-3 py-2.5"
                            >
                                <div className="min-w-0">
                                    <div className="truncate text-[13px] font-medium text-text">
                                        {id}
                                    </div>
                                    <p className="text-[11px] text-text-tertiary">
                                        已保存，但不在本机候选中
                                    </p>
                                </div>
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    onClick={() =>
                                        onChange(
                                            selected.filter((item) => item !== id),
                                        )
                                    }
                                >
                                    移除
                                </Button>
                            </li>
                        ))}
                    </ul>
                )}
            </div>
        </section>
    );
}

function OneBotMessageEditor({
    value,
    onChange,
    onReset,
}: {
    value: string;
    onChange: (value: string) => void;
    onReset: () => void;
}) {
    const editorRef = useRef<HTMLTextAreaElement | null>(null);

    const insertVariable = (token: string) => {
        const editor = editorRef.current;
        const start = editor?.selectionStart ?? value.length;
        const end = editor?.selectionEnd ?? value.length;
        const next = `${value.slice(0, start)}${token}${value.slice(end)}`;
        onChange(next);
        requestAnimationFrame(() => {
            editor?.focus();
            editor?.setSelectionRange(start + token.length, start + token.length);
        });
    };

    const preview = renderTemplatePreview(value);

    return (
        <section className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-sm border border-border-subtle bg-field">
            <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border-subtle px-3 py-2">
                <p className="text-[13px] font-medium text-text">消息内容</p>
                <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="shrink-0 whitespace-nowrap"
                    onClick={onReset}
                >
                    恢复默认
                </Button>
            </div>

            <div className="flex shrink-0 flex-wrap items-center gap-1 border-b border-border-subtle bg-inset/30 px-3 py-1.5">
                <span className="mr-0.5 text-[11px] text-text-tertiary">
                    变量
                </span>
                {TEMPLATE_VARS.map((variable) => (
                    <button
                        key={variable.key}
                        type="button"
                        title={`插入 {${variable.key}}`}
                        onClick={() => insertVariable(`{${variable.key}}`)}
                        className={cn(
                            'rounded-sm border border-border-subtle bg-field px-1.5 py-0.5',
                            'text-[11px] text-text-secondary transition-colors',
                            'hover:border-brand/40 hover:bg-brand/10 hover:text-text',
                            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                        )}
                    >
                        {variable.label}
                    </button>
                ))}
            </div>

            <textarea
                ref={editorRef}
                aria-label="OneBot 消息模板"
                value={value}
                onChange={(event) => onChange(event.target.value)}
                spellCheck={false}
                autoComplete="off"
                placeholder="输入掉线时要发送的消息…"
                className={cn(
                    'block min-h-[7.5rem] w-full flex-1 resize-none bg-transparent px-3 py-2.5',
                    'font-mono text-[12.5px] leading-relaxed text-text',
                    'outline-none placeholder:text-text-tertiary',
                )}
            />

            <div className="shrink-0 border-t border-border-subtle bg-inset/25 px-3 py-2">
                <div className="mb-1 flex items-center justify-between gap-2">
                    <p className="text-[11px] font-medium text-text-secondary">
                        预览
                    </p>
                    <span className="text-[10.5px] text-text-tertiary">
                        示例数据
                    </span>
                </div>
                <div className="max-h-20 overflow-y-auto whitespace-pre-wrap text-[12px] leading-relaxed text-text-secondary">
                    {preview || (
                        <span className="text-text-tertiary">消息内容为空</span>
                    )}
                </div>
            </div>
        </section>
    );
}

function DialogField({
    label,
    hint,
    children,
    trailing,
}: {
    label: ReactNode;
    hint?: ReactNode;
    children: ReactNode;
    /** 标签行右侧（如启用开关），与控件垂直节奏分开 */
    trailing?: ReactNode;
}) {
    return (
        <div className="min-w-0 space-y-1.5">
            <div className="flex items-start justify-between gap-3">
                <div className="min-w-0 space-y-0.5">
                    <div className="text-xs font-medium text-text-secondary">
                        {label}
                    </div>
                    {hint ? (
                        <p className="text-[11.5px] leading-relaxed text-text-tertiary">
                            {hint}
                        </p>
                    ) : null}
                </div>
                {trailing ? (
                    <div className="flex shrink-0 items-center pt-0.5">
                        {trailing}
                    </div>
                ) : null}
            </div>
            {children}
        </div>
    );
}

export function NotificationsTab({ draft, patchDraft }: Props) {
    const [testing, setTesting] = useState<string | null>(null);
    const [editor, setEditor] = useState<ChannelEditorState | null>(null);
    const [editorMount, setEditorMount] = useState<ChannelEditorState | null>(
        null,
    );
    const [deleteId, setDeleteId] = useState<string | null>(null);
    const [emailEditorOpen, setEmailEditorOpen] = useState(false);
    const [emailEditorDraft, setEmailEditorDraft] =
        useState<EmailEditorDraft | null>(null);
    const [emailPreset, setEmailPreset] = useState('');
    const [oneBotEditorOpen, setOneBotEditorOpen] = useState(false);
    const [oneBotEditorDraft, setOneBotEditorDraft] =
        useState<OneBotEditorDraft | null>(null);
    const [oneBotCandidates, setOneBotCandidates] = useState<OneBotCandidate[]>(
        [],
    );
    const [oneBotCandidatesLoading, setOneBotCandidatesLoading] =
        useState(false);
    const [oneBotEnablingId, setOneBotEnablingId] = useState<string | null>(
        null,
    );
    const [history, setHistory] = useState<
        Awaited<ReturnType<typeof settingsService.listOfflineDeliveryHistory>>
    >([]);
    const [historyLoading, setHistoryLoading] = useState(false);
    const [historyDialogOpen, setHistoryDialogOpen] = useState(false);

    useEffect(() => {
        if (editor !== null) setEditorMount(editor);
    }, [editor]);

    const refreshHistory = async () => {
        setHistoryLoading(true);
        try {
            setHistory(await settingsService.listOfflineDeliveryHistory());
        } catch {
            setHistory([]);
        } finally {
            setHistoryLoading(false);
        }
    };

    useEffect(() => {
        void refreshHistory();
    }, []);

    const openHistoryDialog = () => {
        setHistoryDialogOpen(true);
        void refreshHistory();
    };

    if (!draft) {
        return (
            <p className="text-[13px] text-text-tertiary">正在加载设置…</p>
        );
    }

    const channels = draft.webHookChannels;
    const deleteTarget = channels.find((c) => c.id === deleteId) ?? null;
    const enabledChannelCount = channels.filter(
        (c) => c.enabled && c.url.trim(),
    ).length;

    const updateChannels = (next: WebhookChannelDraft[]) => {
        const first = next.find((c) => c.enabled && c.url.trim()) ?? next[0];
        patchDraft({
            webHookChannels: next,
            webHookUrl: first?.url ?? '',
            webHookSecret: first?.secret ?? '',
            webHookJson: first?.bodyTemplate ?? draft.webHookJson,
            webHookMethod: first?.method ?? 'POST',
        });
    };

    const openCreate = () => {
        const id = newChannelId(channels);
        setEditor({
            mode: 'create',
            draft: createBlankWebhookChannel(id, `通道 ${channels.length + 1}`),
        });
    };

    const openEdit = (ch: WebhookChannelDraft) => {
        setEditor({ mode: 'edit', id: ch.id, draft: { ...ch } });
    };

    const closeEditor = () => setEditor(null);

    const patchEditorDraft = (patch: Partial<WebhookChannelDraft>) => {
        setEditor((cur) =>
            cur ? { ...cur, draft: { ...cur.draft, ...patch } } : cur,
        );
    };

    const saveEditor = () => {
        if (!editor) return;
        const nextDraft: WebhookChannelDraft = {
            ...editor.draft,
            name: editor.draft.name.trim(),
            url: editor.draft.url.trim(),
            method: editor.draft.method === 'GET' ? 'GET' : 'POST',
            bodyTemplate: editor.draft.bodyTemplate.trim()
                ? editor.draft.bodyTemplate
                : createBlankWebhookChannel(editor.draft.id).bodyTemplate,
        };
        if (editor.mode === 'create') {
            updateChannels([...channels, nextDraft]);
        } else {
            updateChannels(
                channels.map((c) => (c.id === editor.id ? nextDraft : c)),
            );
        }
        setEditor(null);
    };

    const confirmDelete = () => {
        if (!deleteId) return;
        updateChannels(channels.filter((c) => c.id !== deleteId));
        setDeleteId(null);
    };

    const applyPreset = (presetId: WebhookPresetId) => {
        if (!editor) return;
        const preset = WEBHOOK_PRESETS.find((p) => p.id === presetId);
        if (!preset) return;
        // 切换服务时尽量保留用户已填的标题/正文，只换外壳 JSON 结构。
        const existing =
            parseVisualFields(editor.draft.bodyTemplate) ??
            fieldsFromPresetBody(presetId);
        patchEditorDraft({
            bodyTemplate: serializeVisualFields(presetId, existing),
            name: editor.draft.name.trim() || preset.label,
        });
    };

    const openEmailEditor = () => {
        setEmailPreset('');
        setEmailEditorDraft({
            emailSender: draft.emailSender,
            emailReceiver: draft.emailReceiver,
            emailToken: draft.emailToken,
            emailSmtpServer: draft.emailSmtpServer,
            emailSmtpPort: draft.emailSmtpPort || 465,
            emailEncryption: draft.emailEncryption || 'SSL',
        });
        setEmailEditorOpen(true);
    };

    const saveEmailEditor = () => {
        if (!emailEditorDraft) return;
        patchDraft({
            emailSender: emailEditorDraft.emailSender,
            emailReceiver: emailEditorDraft.emailReceiver,
            emailToken: emailEditorDraft.emailToken,
            emailSmtpServer: emailEditorDraft.emailSmtpServer,
            emailSmtpPort: emailEditorDraft.emailSmtpPort,
            emailEncryption: emailEditorDraft.emailEncryption,
        });
        setEmailEditorOpen(false);
    };

    const openOneBotEditor = () => {
        const messengerIds =
            draft.onebotMessengerBotIds.length > 0
                ? [...draft.onebotMessengerBotIds]
                : draft.onebotMessengerBotId.trim()
                    ? [draft.onebotMessengerBotId.trim()]
                    : [];
        const targetIds =
            draft.onebotTargetIds.length > 0
                ? [...draft.onebotTargetIds]
                : draft.onebotTargetId > 0
                    ? [draft.onebotTargetId]
                    : [];
        setOneBotEditorDraft({
            onebotMessengerBotIds: messengerIds,
            onebotTargetType:
                draft.onebotTargetType === 'group' ? 'group' : 'private',
            onebotTargetIds: targetIds,
            onebotMessageTemplate:
                draft.onebotMessageTemplate || DEFAULT_ONEBOT_MESSAGE,
        });
        setOneBotEditorOpen(true);
        setOneBotCandidatesLoading(true);
        void settingsService
            .listOneBotMessengerCandidates()
            .then((items) => setOneBotCandidates(items))
            .catch(() => setOneBotCandidates([]))
            .finally(() => setOneBotCandidatesLoading(false));
    };

    const saveOneBotEditor = () => {
        if (!oneBotEditorDraft) return;
        const messengerIds = oneBotEditorDraft.onebotMessengerBotIds
            .map((id) => id.trim())
            .filter(Boolean);
        const targetIds = oneBotEditorDraft.onebotTargetIds.filter((id) => id > 0);
        patchDraft({
            onebotMessengerBotIds: messengerIds,
            onebotMessengerBotId: messengerIds[0] ?? '',
            onebotTargetType: oneBotEditorDraft.onebotTargetType,
            onebotTargetIds: targetIds,
            onebotTargetId: targetIds[0] ?? 0,
            onebotMessageTemplate:
                oneBotEditorDraft.onebotMessageTemplate.trim() ||
                DEFAULT_ONEBOT_MESSAGE,
        });
        setOneBotEditorOpen(false);
    };

    const ensureOneBotHttp = async (botId: string) => {
        setOneBotEnablingId(botId);
        try {
            const result = await settingsService.ensureOneBotMessengerHttp(botId);
            setOneBotCandidates((current) => {
                const next = current.filter((item) => item.bot_id !== botId);
                next.push(result.candidate);
                return next;
            });
            setOneBotEditorDraft((current) => {
                if (!current) return current;
                if (current.onebotMessengerBotIds.includes(botId)) return current;
                return {
                    ...current,
                    onebotMessengerBotIds: [...current.onebotMessengerBotIds, botId],
                };
            });
            const actionText =
                result.action === 'already_ready'
                    ? '已具备本机 HTTP'
                    : result.action === 'enabled'
                        ? '已启用现有 HTTP 服务'
                        : '已自动创建本机 HTTP 服务';
            pushInfoBar({
                key: 'onebot-enable-http',
                tone: 'success',
                title: actionText,
                content: `${result.candidate.name || botId} · 端口 ${result.port}。若 Bot 正在运行，会热更新连接配置。`,
            });
        } catch (err) {
            pushInfoBar({
                key: 'onebot-enable-http',
                tone: 'danger',
                title: '自动配置 HTTP 失败',
                content: err instanceof Error ? err.message : String(err),
            });
        } finally {
            setOneBotEnablingId(null);
        }
    };

    const runWebhookTest = async (channelId: string) => {
        setTesting(`webhook:${channelId}`);
        try {
            await settingsService.testWebhook(channelId);
            pushInfoBar({
                key: 'offline-webhook-test',
                tone: 'success',
                title: '测试已发送',
                content:
                    '请到目标服务确认是否收到。若刚改过配置，请先保存设置。',
            });
        } catch (err) {
            pushInfoBar({
                key: 'offline-webhook-test',
                tone: 'danger',
                title: '测试失败',
                content: err instanceof Error ? err.message : String(err),
            });
        } finally {
            setTesting(null);
        }
    };

    const editorWorking = editor ?? editorMount;
    const emailDraft = emailEditorDraft ?? {
        emailSender: draft.emailSender,
        emailReceiver: draft.emailReceiver,
        emailToken: draft.emailToken,
        emailSmtpServer: draft.emailSmtpServer,
        emailSmtpPort: draft.emailSmtpPort || 465,
        emailEncryption: draft.emailEncryption || 'SSL',
    };
    const oneBotDraft = oneBotEditorDraft ?? {
        onebotMessengerBotIds:
            draft.onebotMessengerBotIds.length > 0
                ? draft.onebotMessengerBotIds
                : draft.onebotMessengerBotId.trim()
                    ? [draft.onebotMessengerBotId.trim()]
                    : [],
        onebotTargetType:
            draft.onebotTargetType === 'group' ? 'group' : 'private',
        onebotTargetIds:
            draft.onebotTargetIds.length > 0
                ? draft.onebotTargetIds
                : draft.onebotTargetId > 0
                    ? [draft.onebotTargetId]
                    : [],
        onebotMessageTemplate:
            draft.onebotMessageTemplate || DEFAULT_ONEBOT_MESSAGE,
    };

    return (
        <SettingsTabSections>
            <SettingsSection
                title="桌面通知"
                description="主窗口隐藏或进入轻量模式时，仍可通过系统通知提醒你"
            >
                <FieldRow
                    label="Bot 掉线"
                    description="还需在对应 Bot 高级设置里打开「掉线时发送通知」（NapCat / SnowLuma 均需；此开关同时门控 Webhook / 邮件 / OneBot）"
                >
                    <Switch
                        checked={draft.notifyOnOffline}
                        onCheckedChange={(v) =>
                            patchDraft({ notifyOnOffline: v })
                        }
                    />
                </FieldRow>
                <FieldRow
                    label="Bot 异常退出"
                    description="进程非正常结束时（全局）"
                >
                    <Switch
                        checked={draft.notifyOnBotCrashed}
                        onCheckedChange={(v) =>
                            patchDraft({ notifyOnBotCrashed: v })
                        }
                    />
                </FieldRow>
                <FieldRow
                    label="被踢下线"
                    description="QQ 被踢或登录失效时（全局）"
                    isLast
                >
                    <Switch
                        checked={draft.notifyOnLoginKicked}
                        onCheckedChange={(v) =>
                            patchDraft({ notifyOnLoginKicked: v })
                        }
                    />
                </FieldRow>
            </SettingsSection>

            <SettingsSection
                title="投递行为"
                description="恢复通知、重复掉线防抖与内存投递历史（不落盘，重启清空）"
            >
                <FieldRow
                    label="上线恢复通知"
                    description="掉线后又上线时再推一次（默认关；Webhook/邮件/OneBot 同样走总开关）"
                >
                    <Switch
                        checked={draft.notifyOnRecovered}
                        onCheckedChange={(v) =>
                            patchDraft({ notifyOnRecovered: v })
                        }
                    />
                </FieldRow>
                <FieldRow
                    label="掉线防抖（秒）"
                    description="同一 Bot 在窗口内重复 offline 边沿只投递一次；0 关闭"
                >
                    <NumberField
                        value={draft.offlineDebounceSeconds}
                        min={0}
                        max={600}
                        step={1}
                        onValueChange={(v) =>
                            patchDraft({
                                offlineDebounceSeconds: Math.max(
                                    0,
                                    Math.min(600, Math.round(v || 0)),
                                ),
                            })
                        }
                        className="w-28"
                    />
                </FieldRow>
                <FieldRow
                    label="历史条数上限"
                    description="内存保留最近 N 条投递记录；0 不记录"
                >
                    <NumberField
                        value={draft.offlineDeliveryHistoryLimit}
                        min={0}
                        max={200}
                        step={1}
                        onValueChange={(v) =>
                            patchDraft({
                                offlineDeliveryHistoryLimit: Math.max(
                                    0,
                                    Math.min(200, Math.round(v || 0)),
                                ),
                            })
                        }
                        className="w-28"
                    />
                </FieldRow>
                <FieldRow
                    label="投递记录"
                    description={
                        history.length > 0
                            ? `本次运行已有 ${history.length} 条记录；重启后自动清空`
                            : '查看本次运行中各渠道的投递结果；重启后自动清空'
                    }
                    isLast
                >
                    <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        onClick={openHistoryDialog}
                    >
                        <ActionMotionIcon icon={History} size={14} />
                        查看记录
                    </Button>
                </FieldRow>
            </SettingsSection>

            <SettingsSection
                title="Webhook 推送"
                description="掉线时向配置的地址发 JSON。启用后可管理通道；点通道在对话框里编辑。"
            >
                <FieldRow
                    label="启用 Webhook"
                    description={
                        draft.botOfflineWebHookNotice
                            ? enabledChannelCount > 0
                                ? `当前 ${enabledChannelCount} 个通道会在掉线时发送`
                                : '已开启，但还没有可用通道'
                            : '关闭时折叠配置，通道会保留'
                    }
                    isLast={!draft.botOfflineWebHookNotice}
                >
                    <Switch
                        checked={draft.botOfflineWebHookNotice}
                        onCheckedChange={(v) =>
                            patchDraft({ botOfflineWebHookNotice: v })
                        }
                    />
                </FieldRow>

                {draft.botOfflineWebHookNotice ? (
                    channels.length === 0 ? (
                        <FieldRow
                            label="推送通道"
                            description="还没有通道；点右侧添加，在对话框里填地址与模板"
                            isLast
                        >
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                onClick={openCreate}
                            >
                                <ActionMotionIcon icon={Plus} size={14} />
                                添加
                            </Button>
                        </FieldRow>
                    ) : (
                        <>
                            {channels.map((ch) => {
                                const ready = ch.enabled && !!ch.url.trim();
                                const status = !ch.enabled
                                    ? '已关闭'
                                    : !ch.url.trim()
                                        ? '缺地址'
                                        : ch.method || 'POST';
                                return (
                                    <FieldRow
                                        key={ch.id}
                                        label={channelDisplayName(ch)}
                                        description={`${channelSummary(ch)} · ${status}`}
                                        isLast={false}
                                    >
                                        <div className="flex items-center gap-1.5">
                                            <span
                                                className={cn(
                                                    'mr-1 h-1.5 w-1.5 shrink-0 rounded-full',
                                                    ready
                                                        ? 'bg-success'
                                                        : 'bg-text-tertiary/45',
                                                )}
                                                aria-hidden
                                            />
                                            {!ch.enabled ? (
                                                <Badge
                                                    tone="neutral"
                                                    appearance="soft"
                                                >
                                                    已关闭
                                                </Badge>
                                            ) : !ch.url.trim() ? (
                                                <Badge
                                                    tone="warning"
                                                    appearance="soft"
                                                >
                                                    缺地址
                                                </Badge>
                                            ) : null}
                                            <Button
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                aria-label={`编辑 ${channelDisplayName(ch)}`}
                                                onClick={() => openEdit(ch)}
                                            >
                                                <ActionMotionIcon
                                                    icon={Pencil}
                                                    size={14}
                                                />
                                            </Button>
                                            <Button
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                aria-label={`删除 ${channelDisplayName(ch)}`}
                                                onClick={() =>
                                                    setDeleteId(ch.id)
                                                }
                                            >
                                                <ActionMotionIcon
                                                    icon={Trash2}
                                                    size={14}
                                                />
                                            </Button>
                                        </div>
                                    </FieldRow>
                                );
                            })}
                            <FieldRow
                                label="添加通道"
                                description={`已有 ${channels.length} 个；继续添加可配置多路推送`}
                                isLast
                            >
                                <Button
                                    type="button"
                                    variant="secondary"
                                    size="sm"
                                    onClick={openCreate}
                                >
                                    <ActionMotionIcon icon={Plus} size={14} />
                                    添加
                                </Button>
                            </FieldRow>
                        </>
                    )
                ) : null}
            </SettingsSection>

            <SettingsSection
                title={
                    <span className="flex items-center gap-2">
                        邮件通知
                        {draft.botOfflineEmailNotice && !emailIsReady(emailDraft) ? (
                            <Badge tone="warning" appearance="soft">待完善</Badge>
                        ) : null}
                    </span>
                }
                description="通过 SMTP 发 HTML 邮件。连接信息收敛到对话框，便于一次完成配置。"
            >
                <FieldRow
                    label="启用邮件"
                    description={
                        draft.botOfflineEmailNotice
                            ? emailIsReady(emailDraft)
                                ? `已就绪 · ${emailSummary(emailDraft)}`
                                : emailSummary(emailDraft)
                            : '关闭时折叠配置，已填内容会保留'
                    }
                    isLast
                >
                    <div className="flex items-center gap-2">
                        {draft.botOfflineEmailNotice ? (
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                onClick={openEmailEditor}
                            >
                                <ActionMotionIcon icon={Pencil} size={14} />
                                配置…
                            </Button>
                        ) : null}
                        <Switch
                            checked={draft.botOfflineEmailNotice}
                            onCheckedChange={(v) =>
                                patchDraft({ botOfflineEmailNotice: v })
                            }
                        />
                    </div>
                </FieldRow>
            </SettingsSection>

            <SettingsSection
                title={
                    <span className="flex items-center gap-2">
                        OneBot 通知
                        {draft.onebotNoticeEnabled && !oneBotIsReady(oneBotDraft) ? (
                            <Badge tone="warning" appearance="soft">待完善</Badge>
                        ) : null}
                    </span>
                }
                description="用仍在线 Bot 的本机 HTTP 发私聊或群消息。发送方与消息收敛到同一个对话框。"
            >
                <FieldRow
                    label="启用 OneBot 通知"
                    description={
                        draft.onebotNoticeEnabled
                            ? oneBotIsReady(oneBotDraft)
                                ? `已就绪 · ${oneBotSummary(oneBotDraft)}`
                                : oneBotSummary(oneBotDraft)
                            : '关闭时折叠配置，已填内容会保留'
                    }
                    isLast
                >
                    <div className="flex items-center gap-2">
                        {draft.onebotNoticeEnabled ? (
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                onClick={openOneBotEditor}
                            >
                                <ActionMotionIcon icon={Pencil} size={14} />
                                配置…
                            </Button>
                        ) : null}
                        <Switch
                            checked={draft.onebotNoticeEnabled}
                            onCheckedChange={(v) =>
                                patchDraft({ onebotNoticeEnabled: v })
                            }
                        />
                    </div>
                </FieldRow>
            </SettingsSection>

            <SettingsSection
                title="应用内提示条"
                description="错误类始终需手动关闭；下面控制说明 / 成功 / 警告是否自动消失"
            >
                <FieldRow label="说明" description="蓝色提示">
                    <InfoBarDismissSliderPresence
                        visible={draft.infoBarDismissInfoEnabled}
                    >
                        <InfoBarDismissDurationSlider
                            value={draft.infoBarDismissInfoMs}
                            defaultMs={
                                DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissInfoMs
                            }
                            onChange={(v) =>
                                patchDraft({ infoBarDismissInfoMs: v })
                            }
                        />
                    </InfoBarDismissSliderPresence>
                    <Switch
                        checked={draft.infoBarDismissInfoEnabled}
                        onCheckedChange={(v) =>
                            patchDraft({ infoBarDismissInfoEnabled: v })
                        }
                    />
                </FieldRow>
                <FieldRow label="成功" description="绿色提示">
                    <InfoBarDismissSliderPresence
                        visible={draft.infoBarDismissSuccessEnabled}
                    >
                        <InfoBarDismissDurationSlider
                            value={draft.infoBarDismissSuccessMs}
                            defaultMs={
                                DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissSuccessMs
                            }
                            onChange={(v) =>
                                patchDraft({ infoBarDismissSuccessMs: v })
                            }
                        />
                    </InfoBarDismissSliderPresence>
                    <Switch
                        checked={draft.infoBarDismissSuccessEnabled}
                        onCheckedChange={(v) =>
                            patchDraft({ infoBarDismissSuccessEnabled: v })
                        }
                    />
                </FieldRow>
                <FieldRow label="警告" description="橙色提示" isLast>
                    <InfoBarDismissSliderPresence
                        visible={draft.infoBarDismissWarningEnabled}
                    >
                        <InfoBarDismissDurationSlider
                            value={draft.infoBarDismissWarningMs}
                            defaultMs={
                                DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissWarningMs
                            }
                            onChange={(v) =>
                                patchDraft({ infoBarDismissWarningMs: v })
                            }
                        />
                    </InfoBarDismissSliderPresence>
                    <Switch
                        checked={draft.infoBarDismissWarningEnabled}
                        onCheckedChange={(v) =>
                            patchDraft({ infoBarDismissWarningEnabled: v })
                        }
                    />
                </FieldRow>
            </SettingsSection>

            {/* 投递历史 Dialog */}
            <Dialog
                open={historyDialogOpen}
                onOpenChange={setHistoryDialogOpen}
            >
                <DialogContent
                    size="lg"
                    dismissOnOutsideClick={false}
                    hideClose
                >
                    <DialogHeader>
                        <div className="flex min-w-0 items-center gap-2">
                            <DialogTitle>最近投递</DialogTitle>
                            <Badge tone="neutral" appearance="soft">
                                {history.length} 条
                            </Badge>
                        </div>
                        <DialogDescription>
                            本次运行的通知结果；重启应用后自动清空。
                        </DialogDescription>
                    </DialogHeader>

                    <div className="min-h-0 overflow-hidden rounded-sm border border-border-subtle bg-field">
                        {history.length === 0 ? (
                            <div className="flex min-h-48 flex-col justify-center px-5 py-8 text-center">
                                <p className="text-[13px] font-medium text-text-secondary">
                                    暂无投递记录
                                </p>
                                <p className="mx-auto mt-1.5 max-w-sm text-[12px] leading-relaxed text-text-tertiary">
                                    Bot 掉线、恢复或异常退出后，这里会记录桌面通知和各推送通道的结果。
                                </p>
                            </div>
                        ) : (
                            <ul className="max-h-[min(55vh,32rem)] divide-y divide-border-subtle/70 overflow-y-auto">
                                {history.map((h, i) => (
                                    <li
                                        key={`${h.at}-${h.bot_id}-${i}`}
                                        className="space-y-2 px-4 py-3"
                                    >
                                        <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
                                            <div className="flex min-w-0 items-center gap-2">
                                                <span className="truncate text-[13px] font-medium text-text">
                                                    {h.botName || h.bot_id}
                                                </span>
                                                <Badge
                                                    tone={h.debounced ? 'warning' : 'neutral'}
                                                    appearance="soft"
                                                >
                                                    {h.debounced
                                                        ? '已合并重复提醒'
                                                        : historyKindLabel(h.kind)}
                                                </Badge>
                                            </div>
                                            <span className="shrink-0 text-[11px] tabular-nums text-text-tertiary">
                                                {h.at}
                                            </span>
                                        </div>
                                        {!h.debounced ? (
                                            <div className="flex flex-wrap gap-1.5">
                                                <DeliveryResultBadge label="桌面" value={h.toast} />
                                                <DeliveryResultBadge label="Webhook" value={h.webhook} />
                                                <DeliveryResultBadge label="邮件" value={h.email} />
                                                <DeliveryResultBadge label="OneBot" value={h.onebot} />
                                            </div>
                                        ) : null}
                                        {h.note ? (
                                            <p className="text-[12px] leading-relaxed text-text-tertiary">
                                                {h.note}
                                            </p>
                                        ) : null}
                                    </li>
                                ))}
                            </ul>
                        )}
                    </div>

                    <DialogFooter>
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            disabled={historyLoading}
                            onClick={() => void refreshHistory()}
                        >
                            {historyLoading ? '刷新中…' : '刷新'}
                        </Button>
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            disabled={history.length === 0}
                            onClick={async () => {
                                await settingsService.clearOfflineDeliveryHistory();
                                setHistory([]);
                            }}
                        >
                            清空记录
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            onClick={() => setHistoryDialogOpen(false)}
                        >
                            完成
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            {/* 通道编辑 Dialog */}
            <Dialog
                open={editor !== null}
                onOpenChange={(open) => {
                    if (!open) closeEditor();
                }}
            >
                <DialogContent
                    size="sheetWide"
                    dismissOnOutsideClick={false}
                    onExited={() => setEditorMount(null)}
                    onPointerDownOutside={(e) => {
                        const target = e.target as HTMLElement;
                        if (target.hasAttribute('data-dialog-overlay')) {
                            e.preventDefault();
                        }
                    }}
                    onInteractOutside={(e) => {
                        const target = e.target as HTMLElement;
                        if (target.hasAttribute('data-dialog-overlay')) {
                            e.preventDefault();
                        }
                    }}
                >
                    {editorWorking ? (
                        <>
                            <DialogHeader className="shrink-0">
                                <DialogTitle>
                                    {editorWorking.mode === 'create'
                                        ? '添加推送通道'
                                        : '编辑推送通道'}
                                </DialogTitle>
                                <DialogDescription>
                                    选服务、填连接、编辑消息。完成后点添加/完成，再保存设置。
                                </DialogDescription>
                            </DialogHeader>

                            <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                                <div className="grid min-h-0 flex-1 grid-cols-1 gap-3 overflow-y-auto lg:grid-cols-[14.5rem_minmax(0,1fr)] lg:gap-3 lg:overflow-hidden">
                                    {/* 左：服务 + 连接；宽屏栏内滚动，窄屏整页流式 */}
                                    <div className="flex min-h-0 min-w-0 flex-col gap-3 overflow-x-hidden lg:overflow-y-auto lg:px-0.5 lg:py-0.5">
                                        <div className="space-y-1.5">
                                            <p className="text-xs font-medium text-text-secondary">
                                                服务类型
                                            </p>
                                            <div className="grid grid-cols-2 gap-1.5">
                                                {WEBHOOK_PRESETS.map((preset) => {
                                                    const active =
                                                        detectWebhookService(
                                                            editorWorking.draft
                                                                .bodyTemplate,
                                                        ) === preset.id;
                                                    return (
                                                        <button
                                                            key={preset.id}
                                                            type="button"
                                                            onClick={() =>
                                                                applyPreset(
                                                                    preset.id,
                                                                )
                                                            }
                                                            className={cn(
                                                                'rounded-sm border px-2 py-1.5 text-left transition-colors',
                                                                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                                                                active
                                                                    ? 'border-brand bg-brand/10 text-text'
                                                                    : 'border-border-subtle bg-field text-text-secondary hover:bg-inset hover:text-text',
                                                            )}
                                                        >
                                                            <span className="block text-[12px] font-medium leading-snug">
                                                                {preset.label}
                                                            </span>
                                                            <span className="mt-0.5 block truncate text-[10.5px] leading-snug text-text-tertiary">
                                                                {
                                                                    PRESET_HINTS[
                                                                    preset
                                                                        .id
                                                                    ]
                                                                }
                                                            </span>
                                                        </button>
                                                    );
                                                })}
                                            </div>
                                        </div>

                                        <div className="space-y-2.5 border-t border-border-subtle/70 pt-3">
                                            <p className="text-xs font-medium text-text-secondary">
                                                连接
                                            </p>
                                            {/* 启用是独立表单项，不和「连接」标题绑在一起 */}
                                            <div className="flex items-center justify-between gap-3 rounded-sm border border-border-subtle bg-field px-3 py-2.5">
                                                <div className="min-w-0 space-y-0.5">
                                                    <p className="text-xs font-medium text-text">
                                                        启用通道
                                                    </p>
                                                    <p className="text-[11px] leading-relaxed text-text-tertiary">
                                                        关闭后保留配置，掉线时不发送
                                                    </p>
                                                </div>
                                                <Switch
                                                    checked={
                                                        editorWorking.draft
                                                            .enabled
                                                    }
                                                    onCheckedChange={(v) =>
                                                        patchEditorDraft({
                                                            enabled: v,
                                                        })
                                                    }
                                                />
                                            </div>
                                            <DialogField label="名称">
                                                <TextField
                                                    name="webhook-channel-name"
                                                    autoComplete="off"
                                                    value={
                                                        editorWorking.draft.name
                                                    }
                                                    placeholder="如 Server酱"
                                                    onValueChange={(v) =>
                                                        patchEditorDraft({
                                                            name: v,
                                                        })
                                                    }
                                                />
                                            </DialogField>
                                            <DialogField label="地址">
                                                <TextField
                                                    name="webhook-url"
                                                    type="url"
                                                    autoComplete="off"
                                                    spellCheck={false}
                                                    value={
                                                        editorWorking.draft.url
                                                    }
                                                    placeholder="https://…"
                                                    onValueChange={(v) =>
                                                        patchEditorDraft({
                                                            url: v,
                                                        })
                                                    }
                                                />
                                            </DialogField>
                                            <DialogField label="请求方法">
                                                <Select
                                                    value={
                                                        editorWorking.draft
                                                            .method === 'GET'
                                                            ? 'GET'
                                                            : 'POST'
                                                    }
                                                    onValueChange={(v) =>
                                                        patchEditorDraft({
                                                            method: v,
                                                        })
                                                    }
                                                    items={[...METHOD_ITEMS]}
                                                />
                                            </DialogField>
                                            <DialogField
                                                label={
                                                    <>
                                                        密钥
                                                        <span className="ml-1 text-[10.5px] font-normal text-text-tertiary">
                                                            - 可选 Bearer
                                                        </span>
                                                    </>
                                                }
                                            >
                                                <SecretField
                                                    name="webhook-secret"
                                                    value={
                                                        editorWorking.draft
                                                            .secret
                                                    }
                                                    onValueChange={(v) =>
                                                        patchEditorDraft({
                                                            secret: v,
                                                        })
                                                    }
                                                />
                                            </DialogField>
                                            {editorWorking.mode === 'edit' ? (
                                                <Button
                                                    type="button"
                                                    variant="secondary"
                                                    size="sm"
                                                    className="self-start"
                                                    disabled={
                                                        testing ===
                                                        `webhook:${editorWorking.id}` ||
                                                        !editorWorking.draft.url.trim()
                                                    }
                                                    onClick={() =>
                                                        void runWebhookTest(
                                                            editorWorking.id,
                                                        )
                                                    }
                                                >
                                                    {testing ===
                                                        `webhook:${editorWorking.id}`
                                                        ? '发送中…'
                                                        : '发送测试'}
                                                </Button>
                                            ) : null}
                                        </div>
                                    </div>

                                    {/* 中+右：字段表单 + 原始 JSON（组件内两栏） */}
                                    <div className="flex min-h-[14rem] min-w-0 flex-col lg:h-full lg:min-h-0">
                                        <WebhookMessageBuilder
                                            bodyTemplate={
                                                editorWorking.draft
                                                    .bodyTemplate
                                            }
                                            serviceHint={(() => {
                                                const k = detectWebhookService(
                                                    editorWorking.draft
                                                        .bodyTemplate,
                                                );
                                                return k === 'custom'
                                                    ? null
                                                    : k;
                                            })()}
                                            onBodyTemplateChange={(v) =>
                                                patchEditorDraft({
                                                    bodyTemplate: v,
                                                })
                                            }
                                        />
                                    </div>
                                </div>
                            </div>

                            <DialogFooter className="mt-3 shrink-0">
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    onClick={closeEditor}
                                >
                                    取消
                                </Button>
                                <Button
                                    type="button"
                                    size="sm"
                                    onClick={saveEditor}
                                >
                                    {editorWorking.mode === 'create'
                                        ? '添加'
                                        : '完成'}
                                </Button>
                            </DialogFooter>
                        </>
                    ) : null}
                </DialogContent>
            </Dialog>

            {/* 邮件连接 Dialog */}
            <Dialog
                open={emailEditorOpen}
                onOpenChange={(open) => {
                    setEmailEditorOpen(open);
                    if (!open) setEmailEditorDraft(null);
                }}
            >
                <DialogContent size="md" dismissOnOutsideClick={false}>
                    <DialogHeader>
                        <DialogTitle>配置邮件通知</DialogTitle>
                        <DialogDescription>
                            选择常用邮箱后补齐收发地址和授权码。完成后回到设置页保存。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="space-y-5 py-1">
                        <DialogField
                            label="常用邮箱"
                            hint="选择后会自动填入服务器、端口和加密方式。"
                        >
                            <Select
                                value={emailPreset}
                                placeholder="选择服务商，或手动填写"
                                onValueChange={(label) => {
                                    setEmailPreset(label);
                                    const preset = EMAIL_PRESETS.find(
                                        (item) => item.label === label,
                                    );
                                    if (!preset) return;
                                    setEmailEditorDraft((current) =>
                                        current
                                            ? {
                                                ...current,
                                                emailSmtpServer: preset.server,
                                                emailSmtpPort: preset.port,
                                                emailEncryption: preset.encryption,
                                            }
                                            : current,
                                    );
                                }}
                                items={EMAIL_PRESET_ITEMS}
                            />
                        </DialogField>

                        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                            <DialogField label="发件邮箱">
                                <TextField
                                    name="email-sender"
                                    type="email"
                                    autoComplete="off"
                                    spellCheck={false}
                                    value={emailDraft.emailSender}
                                    placeholder="you@example.com"
                                    onValueChange={(emailSender) =>
                                        setEmailEditorDraft((current) =>
                                            current ? { ...current, emailSender } : current,
                                        )
                                    }
                                />
                            </DialogField>
                            <DialogField label="收件邮箱">
                                <TextField
                                    name="email-receiver"
                                    type="email"
                                    autoComplete="off"
                                    spellCheck={false}
                                    value={emailDraft.emailReceiver}
                                    placeholder="alert@example.com"
                                    onValueChange={(emailReceiver) =>
                                        setEmailEditorDraft((current) =>
                                            current ? { ...current, emailReceiver } : current,
                                        )
                                    }
                                />
                            </DialogField>
                        </div>

                        <DialogField
                            label="授权码"
                            hint="使用邮箱服务商生成的 SMTP 授权码，不是登录密码。"
                        >
                            <SecretField
                                name="email-token"
                                value={emailDraft.emailToken}
                                placeholder="输入授权码"
                                onValueChange={(emailToken) =>
                                    setEmailEditorDraft((current) =>
                                        current ? { ...current, emailToken } : current,
                                    )
                                }
                            />
                        </DialogField>

                        <div className="grid grid-cols-[minmax(0,1fr)_6.5rem] gap-3">
                            <DialogField label="SMTP 服务器">
                                <TextField
                                    name="email-smtp"
                                    autoComplete="off"
                                    spellCheck={false}
                                    value={emailDraft.emailSmtpServer}
                                    placeholder="smtp.example.com"
                                    onValueChange={(emailSmtpServer) =>
                                        setEmailEditorDraft((current) =>
                                            current
                                                ? { ...current, emailSmtpServer }
                                                : current,
                                        )
                                    }
                                />
                            </DialogField>
                            <DialogField label="端口">
                                <NumberField
                                    name="email-port"
                                    value={emailDraft.emailSmtpPort}
                                    min={1}
                                    max={65535}
                                    onValueChange={(value) =>
                                        setEmailEditorDraft((current) =>
                                            current
                                                ? {
                                                    ...current,
                                                    emailSmtpPort: Math.max(
                                                        1,
                                                        Math.min(65535, Math.round(value || 1)),
                                                    ),
                                                }
                                                : current,
                                        )
                                    }
                                />
                            </DialogField>
                        </div>

                        <DialogField label="连接加密">
                            <Select
                                value={emailDraft.emailEncryption || 'SSL'}
                                onValueChange={(emailEncryption) =>
                                    setEmailEditorDraft((current) =>
                                        current ? { ...current, emailEncryption } : current,
                                    )
                                }
                                items={[...ENCRYPTION_ITEMS]}
                            />
                        </DialogField>
                    </div>
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => setEmailEditorOpen(false)}
                        >
                            取消
                        </Button>
                        <Button type="button" size="sm" onClick={saveEmailEditor}>
                            完成
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            {/* OneBot Dialog：左发送方主列表，右栏目标 + 消息上下叠 */}
            <Dialog
                open={oneBotEditorOpen}
                onOpenChange={(open) => {
                    setOneBotEditorOpen(open);
                    if (!open) setOneBotEditorDraft(null);
                }}
            >
                <DialogContent size="sheetWide" dismissOnOutsideClick={false}>
                    <DialogHeader className="shrink-0">
                        <DialogTitle>配置 OneBot 通知</DialogTitle>
                        <DialogDescription>
                            选好发送方和接收目标即可；消息模板可按需改。
                        </DialogDescription>
                    </DialogHeader>

                    {/* min-h 抬高 sheetWide 内容高度，避免矮对话框里目标区被压扁 */}
                    <div className="grid min-h-[min(72dvh,40rem)] flex-1 grid-cols-1 gap-3 overflow-y-auto lg:grid-cols-[minmax(0,1.15fr)_minmax(18rem,0.85fr)] lg:overflow-hidden">
                        {/* 左：发送方列表（主焦点） */}
                        <div className="flex min-h-[22rem] min-w-0 flex-col lg:h-full lg:min-h-0">
                            <OneBotMessengerPicker
                                selected={oneBotDraft.onebotMessengerBotIds}
                                candidates={oneBotCandidates}
                                loading={oneBotCandidatesLoading}
                                enablingId={oneBotEnablingId}
                                onChange={(onebotMessengerBotIds) =>
                                    setOneBotEditorDraft((current) =>
                                        current
                                            ? {
                                                ...current,
                                                onebotMessengerBotIds,
                                            }
                                            : current,
                                    )
                                }
                                onEnsureHttp={(botId) => {
                                    void ensureOneBotHttp(botId);
                                }}
                            />
                        </div>

                        {/* 右：接收目标（上）+ 消息内容（下） */}
                        <div className="flex min-h-0 min-w-0 flex-col gap-3 lg:h-full lg:overflow-hidden">
                            <section className="shrink-0 space-y-2.5 rounded-sm border border-border-subtle bg-field p-3">
                                <div className="flex items-center justify-between gap-2">
                                    <p className="text-[13px] font-medium text-text">
                                        接收目标
                                    </p>
                                    {oneBotDraft.onebotTargetIds.length > 0 ? (
                                        <Badge tone="neutral" appearance="soft">
                                            {oneBotDraft.onebotTargetIds.length}{' '}
                                            个
                                        </Badge>
                                    ) : null}
                                </div>
                                <div className="space-y-2">
                                    <div className="w-full max-w-[10rem]">
                                        <Select
                                            value={oneBotDraft.onebotTargetType}
                                            onValueChange={(onebotTargetType) =>
                                                setOneBotEditorDraft((current) =>
                                                    current
                                                        ? {
                                                            ...current,
                                                            onebotTargetType,
                                                        }
                                                        : current,
                                                )
                                            }
                                            items={[...ONEBOT_TARGET_ITEMS]}
                                        />
                                    </div>
                                    <TargetIdChipsInput
                                        values={oneBotDraft.onebotTargetIds}
                                        className="min-h-[6.5rem] max-h-[9.5rem]"
                                        placeholder={
                                            oneBotDraft.onebotTargetType ===
                                                'group'
                                                ? '群号，逗号添加多个'
                                                : 'QQ 号，逗号添加多个'
                                        }
                                        onChange={(onebotTargetIds) =>
                                            setOneBotEditorDraft((current) =>
                                                current
                                                    ? {
                                                        ...current,
                                                        onebotTargetIds,
                                                    }
                                                    : current,
                                            )
                                        }
                                    />
                                </div>
                            </section>

                            <div className="flex min-h-[16rem] min-w-0 flex-1 flex-col lg:min-h-0">
                                <OneBotMessageEditor
                                    value={oneBotDraft.onebotMessageTemplate}
                                    onChange={(onebotMessageTemplate) =>
                                        setOneBotEditorDraft((current) =>
                                            current
                                                ? {
                                                    ...current,
                                                    onebotMessageTemplate,
                                                }
                                                : current,
                                        )
                                    }
                                    onReset={() =>
                                        setOneBotEditorDraft((current) =>
                                            current
                                                ? {
                                                    ...current,
                                                    onebotMessageTemplate:
                                                        DEFAULT_ONEBOT_MESSAGE,
                                                }
                                                : current,
                                        )
                                    }
                                />
                            </div>
                        </div>
                    </div>

                    <DialogFooter className="mt-3 shrink-0">
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => setOneBotEditorOpen(false)}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            onClick={saveOneBotEditor}
                        >
                            完成
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            {/* 删除确认 */}
            <Dialog
                open={deleteId !== null}
                onOpenChange={(open) => {
                    if (!open) setDeleteId(null);
                }}
            >
                <DialogContent size="sm">
                    <DialogHeader>
                        <DialogTitle>删除此通道？</DialogTitle>
                        <DialogDescription>
                            {deleteTarget
                                ? `将删除「${channelDisplayName(deleteTarget)}」。写入草稿后需保存设置才会落盘。`
                                : '写入草稿后需保存设置才会落盘。'}
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => setDeleteId(null)}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            variant="danger"
                            size="sm"
                            onClick={confirmDelete}
                        >
                            删除
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </SettingsTabSections >
    );
}

