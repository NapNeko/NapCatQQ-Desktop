// 知识库建 / 删走 Dashboard 即时落库；「挂到对话里」是配置项，改了要走顶部保存。

import { useState } from 'react';
import { ExternalLink, Library, Plus, Trash2 } from 'lucide-react';
import { Badge, Button, FormSection, Select, Switch, TextField } from '../../../../shared/ui';
import { ConfigForm } from '../karin/configLayout';
import { embeddingSources } from '../../../../core/domain/apps/astrbotConfig';
import { AstrBotRuntimeGate, dashboardReady } from './AstrBotRuntimeGate';
import { ConfirmDelete, EmptyHint, EntityRow, FormDialog, JumpLink } from './parts';
import { useAstrBotKbs } from '../../../../hooks/apps/useAstrBotDashboard';
import type {
    AstrBotDashboardStatus,
    AstrBotInstanceConfig,
    AstrBotKbCreate,
    AstrBotKnowledgeBase,
} from '../../../../core/ipc/types';

export const AstrBotKbTab: React.FC<{
    instanceId: string;
    status: AstrBotDashboardStatus | undefined;
    statusLoading: boolean;
    config: AstrBotInstanceConfig;
    onChange: (next: AstrBotInstanceConfig) => void;
    formDisabled?: boolean;
    onOpenWebUi: (path: string) => void;
    onGoTab: (tab: string) => void;
    onStart: () => void;
    starting: boolean;
}> = ({ instanceId, status, statusLoading, config, onChange, formDisabled, onOpenWebUi, onGoTab, onStart, starting }) => {
    const ready = dashboardReady(status);
    const kbs = useAstrBotKbs(instanceId, ready);
    const embeddings = embeddingSources(config);
    const [draft, setDraft] = useState<AstrBotKbCreate | null>(null);
    const [pendingDelete, setPendingDelete] = useState<AstrBotKnowledgeBase | null>(null);
    const list = kbs.data ?? [];
    const canCreate = ready && embeddings.length > 0;
    const nameTaken = !!draft && list.some((k) => k.kb_name === draft.kb_name.trim());

    const mounted = new Set(config.kb.names);
    const setMounted = (name: string, on: boolean) =>
        onChange({
            ...config,
            kb: {
                ...config.kb,
                names: on ? [...config.kb.names.filter((n) => n !== name), name] : config.kb.names.filter((n) => n !== name),
            },
        });

    const newButton = (
        <Button
            size="sm"
            variant="secondary"
            disabled={!canCreate}
            title={ready && embeddings.length === 0 ? '先要有一个向量嵌入提供商' : undefined}
            onClick={() =>
                setDraft({
                    kb_name: '',
                    description: '',
                    embedding_provider_id: embeddings[0]?.id ?? '',
                })
            }
        >
            <Plus size={13} /> 新建
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
                title="知识库"
                description="文档的上传和切片在 AstrBot WebUI 里做，这里只管建、删和挂到对话里"
                actions={
                    <>
                        <Button
                            size="sm"
                            variant="ghost"
                            disabled={!ready}
                            onClick={() => onOpenWebUi('/knowledge-base')}
                        >
                            <ExternalLink size={12} /> WebUI
                        </Button>
                        {list.length > 0 && newButton}
                    </>
                }
                layout="none"
            >
                {list.length === 0 ? (
                    <EmptyHint
                        icon={Library}
                        title={
                            !ready ? (
                                '连上控制台后可以管理知识库'
                            ) : embeddings.length === 0 ? (
                                <>
                                    建知识库要先有一个向量嵌入提供商。
                                    <JumpLink tab="models" onGo={onGoTab}>去「模型」页加</JumpLink>
                                </>
                            ) : (
                                '还没有知识库'
                            )
                        }
                        action={canCreate ? newButton : undefined}
                    />
                ) : (
                    <div className="flex flex-col gap-2">
                        {list.map((kb) => (
                            <EntityRow
                                key={kb.kb_id}
                                icon={Library}
                                title={kb.kb_name || kb.kb_id}
                                tags={
                                    kb.embedding_provider_id ? (
                                        <Badge tone="info" className="font-mono">
                                            {kb.embedding_provider_id}
                                        </Badge>
                                    ) : undefined
                                }
                                subtitle={kb.description || undefined}
                                actions={
                                    <>
                                        <Switch
                                            label="挂到对话"
                                            checked={mounted.has(kb.kb_name)}
                                            disabled={formDisabled}
                                            onCheckedChange={(on) => setMounted(kb.kb_name, on)}
                                        />
                                        <Button
                                            variant="ghost"
                                            size="icon"
                                            className="ml-1 h-7 w-7 text-danger hover:text-danger"
                                            aria-label="删除"
                                            disabled={!ready}
                                            onClick={() => setPendingDelete(kb)}
                                        >
                                            <Trash2 size={13} />
                                        </Button>
                                    </>
                                }
                            />
                        ))}
                    </div>
                )}
            </FormSection>

            {draft && (
                <FormDialog
                    open
                    size="sm"
                    title="新建知识库"
                    confirmLabel="创建"
                    confirmDisabled={!draft.kb_name.trim() || nameTaken || !draft.embedding_provider_id.trim()}
                    busy={kbs.create.isPending}
                    onCancel={() => setDraft(null)}
                    onConfirm={() => {
                        void kbs.create
                            .mutateAsync({ ...draft, kb_name: draft.kb_name.trim() })
                            .then(() => setDraft(null));
                    }}
                >
                    <TextField
                        label="名称"
                        autoFocus
                        value={draft.kb_name}
                        error={nameTaken ? '已有同名知识库' : undefined}
                        onValueChange={(kb_name) => setDraft({ ...draft, kb_name })}
                    />
                    <TextField
                        label="说明"
                        value={draft.description}
                        onValueChange={(description) => setDraft({ ...draft, description })}
                    />
                    <Select
                        label="向量嵌入提供商"
                        value={draft.embedding_provider_id || undefined}
                        items={embeddings.map((s) => ({ value: s.id, label: s.id }))}
                        hint="建好后不能换"
                        onValueChange={(embedding_provider_id) => setDraft({ ...draft, embedding_provider_id })}
                    />
                </FormDialog>
            )}

            <ConfirmDelete
                open={!!pendingDelete}
                title={`删除知识库「${pendingDelete?.kb_name}」？`}
                description="里面的文档和向量一起删，不能撤销。"
                busy={kbs.remove.isPending}
                onCancel={() => setPendingDelete(null)}
                onConfirm={() => {
                    if (!pendingDelete) return;
                    const name = pendingDelete.kb_name;
                    void kbs.remove.mutateAsync(pendingDelete.kb_id).then(() => {
                        setPendingDelete(null);
                        if (mounted.has(name)) setMounted(name, false);
                    });
                }}
            />
        </ConfigForm>
    );
};
