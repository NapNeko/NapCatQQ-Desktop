// Webhook 推送通道编辑 Dialog（服务类型 + 连接 + 消息模板）

import {
    WEBHOOK_PRESETS,
    type WebhookChannelDraft,
    type WebhookPresetId,
} from '../../../../core/domain/settings/offline-notify-defaults';
import {
    detectWebhookService,
    fieldsFromPresetBody,
    parseVisualFields,
    serializeVisualFields,
} from '../../../../core/domain/settings/webhook-message-visual';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Select,
    Switch,
    TextField,
} from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { WebhookMessageBuilder } from '../WebhookMessageBuilder';
import { DialogField, SecretField } from './dialog-shared';

const METHOD_ITEMS = [
    { value: 'POST', label: 'POST' },
    { value: 'GET', label: 'GET' },
] as const;

/** 左侧服务卡片副文案：尽量短 */
const PRESET_HINTS: Record<WebhookPresetId, string> = {
    serverchan: 'title / desp',
    dingtalk: 'markdown',
    feishu: 'text',
    discord: 'Embed',
    bark: '推送',
};

export type ChannelEditorState =
    | { mode: 'create'; draft: WebhookChannelDraft }
    | { mode: 'edit'; id: string; draft: WebhookChannelDraft };

export function WebhookChannelEditorDialog({
    open,
    working,
    testingKey,
    onOpenChange,
    onExited,
    onPatchDraft,
    onSave,
    onTest,
}: {
    open: boolean;
    working: ChannelEditorState | null;
    testingKey: string | null;
    onOpenChange: (open: boolean) => void;
    onExited: () => void;
    onPatchDraft: (patch: Partial<WebhookChannelDraft>) => void;
    onSave: () => void;
    onTest: (channelId: string) => void;
}) {
    const applyPreset = (presetId: WebhookPresetId) => {
        if (!working) return;
        const preset = WEBHOOK_PRESETS.find((p) => p.id === presetId);
        if (!preset) return;
        // 切换服务时尽量保留用户已填的标题/正文，只换外壳 JSON 结构。
        const existing =
            parseVisualFields(working.draft.bodyTemplate) ??
            fieldsFromPresetBody(presetId);
        onPatchDraft({
            bodyTemplate: serializeVisualFields(presetId, existing),
            name: working.draft.name.trim() || preset.label,
        });
    };

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                size="sheetWide"
                dismissOnOutsideClick={false}
                onExited={onExited}
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
                {working ? (
                    <>
                        <DialogHeader className="shrink-0">
                            <DialogTitle>
                                {working.mode === 'create'
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
                                                        working.draft
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
                                                                    preset.id
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
                                                checked={working.draft.enabled}
                                                onCheckedChange={(v) =>
                                                    onPatchDraft({
                                                        enabled: v,
                                                    })
                                                }
                                            />
                                        </div>
                                        <DialogField label="名称">
                                            <TextField
                                                name="webhook-channel-name"
                                                autoComplete="off"
                                                value={working.draft.name}
                                                placeholder="如 Server酱"
                                                onValueChange={(v) =>
                                                    onPatchDraft({ name: v })
                                                }
                                            />
                                        </DialogField>
                                        <DialogField label="地址">
                                            <TextField
                                                name="webhook-url"
                                                type="url"
                                                autoComplete="off"
                                                spellCheck={false}
                                                value={working.draft.url}
                                                placeholder="https://…"
                                                onValueChange={(v) =>
                                                    onPatchDraft({ url: v })
                                                }
                                            />
                                        </DialogField>
                                        <DialogField label="请求方法">
                                            <Select
                                                value={
                                                    working.draft.method ===
                                                    'GET'
                                                        ? 'GET'
                                                        : 'POST'
                                                }
                                                onValueChange={(v) =>
                                                    onPatchDraft({
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
                                                value={working.draft.secret}
                                                onValueChange={(v) =>
                                                    onPatchDraft({
                                                        secret: v,
                                                    })
                                                }
                                            />
                                        </DialogField>
                                        {working.mode === 'edit' ? (
                                            <Button
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="self-start"
                                                disabled={
                                                    testingKey ===
                                                        `webhook:${working.id}` ||
                                                    !working.draft.url.trim()
                                                }
                                                onClick={() =>
                                                    onTest(working.id)
                                                }
                                            >
                                                {testingKey ===
                                                `webhook:${working.id}`
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
                                            working.draft.bodyTemplate
                                        }
                                        serviceHint={(() => {
                                            const k = detectWebhookService(
                                                working.draft.bodyTemplate,
                                            );
                                            return k === 'custom' ? null : k;
                                        })()}
                                        onBodyTemplateChange={(v) =>
                                            onPatchDraft({
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
                                onClick={() => onOpenChange(false)}
                            >
                                取消
                            </Button>
                            <Button type="button" size="sm" onClick={onSave}>
                                {working.mode === 'create' ? '添加' : '完成'}
                            </Button>
                        </DialogFooter>
                    </>
                ) : null}
            </DialogContent>
        </Dialog>
    );
}
