// 人格列表走 Dashboard 即时落库；「默认人格」是配置项，改了要走顶部保存。

import { useState } from 'react';
import { MessagesSquare, Plus, Star, Trash2, UserRound } from 'lucide-react';
import { Badge, Button, FormSection, StringListField, SyntaxTextEditor, TextField } from '../../../../shared/ui';
import { ConfigForm } from '../karin/configLayout';
import { AstrBotRuntimeGate, dashboardReady } from './AstrBotRuntimeGate';
import { ConfirmDelete, EmptyHint, EntityRow, FormDialog } from './parts';
import { useAstrBotPersonas } from '../../../../hooks/apps/useAstrBotDashboard';
import { cn } from '../../../../shared/utils/cn';
import type { AstrBotDashboardStatus, AstrBotPersona } from '../../../../core/ipc/types';

type Draft = { mode: 'create' | 'edit'; persona: AstrBotPersona };

const emptyPersona = (): AstrBotPersona => ({
    persona_id: '',
    system_prompt: '',
    begin_dialogs: [],
    folder_id: '',
});

export const AstrBotPersonaTab: React.FC<{
    instanceId: string;
    status: AstrBotDashboardStatus | undefined;
    statusLoading: boolean;
    defaultPersona: string;
    onSetDefault: (id: string) => void;
    formDisabled?: boolean;
    onGoTab: (tab: string) => void;
    onStart: () => void;
    starting: boolean;
}> = ({ instanceId, status, statusLoading, defaultPersona, onSetDefault, formDisabled, onGoTab, onStart, starting }) => {
    const ready = dashboardReady(status);
    const personas = useAstrBotPersonas(instanceId, ready);
    const [draft, setDraft] = useState<Draft | null>(null);
    const [pendingDelete, setPendingDelete] = useState<AstrBotPersona | null>(null);
    const list = personas.data ?? [];

    const idTaken = draft?.mode === 'create' && list.some((p) => p.persona_id === draft.persona.persona_id.trim());

    const newButton = (
        <Button
            size="sm"
            variant="secondary"
            disabled={!ready}
            onClick={() => setDraft({ mode: 'create', persona: emptyPersona() })}
        >
            <Plus size={13} /> 新建人格
        </Button>
    );

    return (
        <ConfigForm>
            <AstrBotRuntimeGate
                status={status}
                loading={statusLoading}
                onGoTab={onGoTab}
                onStart={onStart}
                starting={starting}
            />
            <FormSection
                title="人格"
                actions={list.length > 0 ? newButton : undefined}
                layout="none"
            >
                {list.length === 0 ? (
                    <EmptyHint
                        icon={UserRound}
                        title={ready ? '还没有人格，新建一个来定下机器人的说话方式' : '连上控制台后可以管理人格'}
                        action={ready ? newButton : undefined}
                    />
                ) : (
                    <div className="flex flex-col gap-2">
                        {list.map((p) => {
                            const isDefault = p.persona_id === defaultPersona;
                            return (
                                <EntityRow
                                    key={p.persona_id}
                                    icon={UserRound}
                                    title={<span className="font-mono">{p.persona_id}</span>}
                                    tags={
                                        <>
                                            {isDefault && <Badge tone="warning">默认</Badge>}
                                            {p.begin_dialogs.length > 0 && (
                                                <Badge tone="neutral">
                                                    <MessagesSquare size={10} />
                                                    {p.begin_dialogs.length}
                                                </Badge>
                                            )}
                                        </>
                                    }
                                    subtitle={p.system_prompt || '没写提示词'}
                                    actions={
                                        <>
                                            <Button
                                                variant="ghost"
                                                size="icon"
                                                className={cn('h-7 w-7', isDefault && 'text-warning hover:text-warning')}
                                                aria-label={isDefault ? '当前默认人格' : '设为默认人格'}
                                                aria-pressed={isDefault}
                                                title={isDefault ? '当前默认人格' : '设为默认人格（要保存）'}
                                                disabled={formDisabled || isDefault}
                                                onClick={() => onSetDefault(p.persona_id)}
                                            >
                                                <Star size={13} fill={isDefault ? 'currentColor' : 'none'} />
                                            </Button>
                                            <Button
                                                variant="ghost"
                                                size="icon"
                                                className="h-7 w-7 text-danger hover:text-danger"
                                                aria-label="删除"
                                                disabled={!ready}
                                                onClick={() => setPendingDelete(p)}
                                            >
                                                <Trash2 size={13} />
                                            </Button>
                                        </>
                                    }
                                    onOpen={ready ? () => setDraft({ mode: 'edit', persona: p }) : undefined}
                                />
                            );
                        })}
                    </div>
                )}
            </FormSection>

            {draft && (
                <FormDialog
                    open
                    size="lg"
                    title={draft.mode === 'create' ? '新建人格' : `编辑人格 · ${draft.persona.persona_id}`}
                    confirmLabel={draft.mode === 'create' ? '创建' : '保存'}
                    confirmDisabled={!draft.persona.persona_id.trim() || idTaken}
                    busy={personas.upsert.isPending}
                    onCancel={() => setDraft(null)}
                    onConfirm={() => {
                        void personas.upsert
                            .mutateAsync({
                                persona: { ...draft.persona, persona_id: draft.persona.persona_id.trim() },
                                creating: draft.mode === 'create',
                            })
                            .then(() => setDraft(null));
                    }}
                >
                    {draft.mode === 'create' && (
                        <TextField
                            label="标识"
                            autoFocus
                            value={draft.persona.persona_id}
                            className="font-mono"
                            placeholder="唯一，创建后不能改"
                            error={idTaken ? '已有同名人格' : undefined}
                            onValueChange={(persona_id) => setDraft({ ...draft, persona: { ...draft.persona, persona_id } })}
                        />
                    )}
                    <div className="flex flex-col gap-1.5">
                        <label className="text-xs font-medium text-text-secondary">系统提示词</label>
                        <div className="flex h-48 flex-col">
                            <SyntaxTextEditor
                                mode="plain"
                                wrap
                                value={draft.persona.system_prompt}
                                aria-label="系统提示词"
                                onChange={(system_prompt) => setDraft({ ...draft, persona: { ...draft.persona, system_prompt } })}
                            />
                        </div>
                    </div>
                    <StringListField
                        label="开场对话"
                        value={draft.persona.begin_dialogs}
                        mono={false}
                        onChange={(begin_dialogs) => setDraft({ ...draft, persona: { ...draft.persona, begin_dialogs } })}
                    />
                </FormDialog>
            )}

            <ConfirmDelete
                open={!!pendingDelete}
                title={`删除人格「${pendingDelete?.persona_id}」？`}
                description={
                    pendingDelete?.persona_id === defaultPersona
                        ? '它是当前默认人格，删了之后对话会退回内置人格。'
                        : '立即从 AstrBot 删除，不能撤销。'
                }
                busy={personas.remove.isPending}
                onCancel={() => setPendingDelete(null)}
                onConfirm={() => {
                    if (!pendingDelete) return;
                    void personas.remove.mutateAsync(pendingDelete.persona_id).then(() => setPendingDelete(null));
                }}
            />
        </ConfigForm>
    );
};
