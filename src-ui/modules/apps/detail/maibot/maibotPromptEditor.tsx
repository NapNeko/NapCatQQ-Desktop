// 提示词编辑区：头部（名字、说明、版本、对比）、参数、编辑器或对比视图、底部（校验、恢复默认、保存）。
// 保存覆盖在用的版本（没有就新建一个），另存为走版本菜单；Ctrl+S 存当前这份。

import { useEffect, useMemo, useRef, useState } from 'react';
import { AlertCircle, Check, GitCompare, RotateCcw, Save } from 'lucide-react';
import { Badge, Button, Spinner, SyntaxTextEditor, TextField, type SyntaxTextEditorHandle } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import {
    checkPrompt,
    PROMPT_LEGACY_VERSION,
    promptDisplayName,
    promptLanguageLabel,
    promptParams,
} from '../../../../core/domain/apps/maibotPrompts';
import { changedLines, lineDiff } from '../../../../core/domain/apps/textDiff';
import type { MaiBotPromptInfo, MaiBotPromptVersion } from '../../../../core/ipc/types';
import {
    useMaiBotPromptAction,
    useMaiBotPromptFile,
    useMaiBotPromptVersion,
    type PromptMode,
} from '../../../../hooks/apps/useMaiBotPrompts';
import { ConfirmDelete, FormDialog } from '../entityParts';
import { PaneLoadError } from '../PaneStatus';
import { DiffView, ParamChips, VersionMenu, VersionPreview } from './maibotPromptParts';
import { promptDraftKey, type PromptDrafts } from './maibotPromptDrafts';

type Pending =
    | { kind: 'restore' }
    | { kind: 'delete'; version: MaiBotPromptVersion }
    | { kind: 'activate'; version: MaiBotPromptVersion };

export const PromptEditor: React.FC<{
    instanceId: string;
    mode: PromptMode;
    info: MaiBotPromptInfo;
    language: string;
    activeLanguage: string;
    /** 麦麦在跑：保存后下一次用到就生效 */
    live: boolean;
    drafts: PromptDrafts;
}> = ({ instanceId, mode, info, language, activeLanguage, live, drafts }) => {
    const file = useMaiBotPromptFile(instanceId, mode, language, info.name);
    const act = useMaiBotPromptAction(instanceId, mode);
    const key = promptDraftKey(language, info.name);
    const f = file.data;
    const draft = drafts.get(key);
    const content = draft ?? f?.content ?? '';
    const dirty = !!f && draft !== undefined && draft !== f.content;
    const defaultContent = f?.default_content ?? '';
    const check = useMemo(() => checkPrompt(content, defaultContent), [content, defaultContent]);
    const params = useMemo(() => promptParams(defaultContent), [defaultContent]);
    const [diffOn, setDiffOn] = useState(false);
    const [preview, setPreview] = useState<MaiBotPromptVersion | null>(null);
    const [pending, setPending] = useState<Pending | null>(null);
    const [saveAsLabel, setSaveAsLabel] = useState<string | null>(null);
    const editor = useRef<SyntaxTextEditorHandle>(null);
    const previewBody = useMaiBotPromptVersion(instanceId, mode, language, info.name, preview?.id ?? null);

    const busy = act.isPending;
    const canSave = dirty && !check.error && !busy;
    const save = (label = '', asNew = false) => {
        if (!f) return;
        const active = f.active_version_id && f.active_version_id !== PROMPT_LEGACY_VERSION ? f.active_version_id : null;
        const saved = content;
        act.mutate(
            { op: 'save', language, name: info.name, content: saved, label, version_id: asNew ? null : active },
            // 存的过程中又改了几个字，草稿留着
            { onSuccess: () => drafts.get(key) === saved && drafts.clear(key) },
        );
    };
    const activate = (v: MaiBotPromptVersion) =>
        act.mutate(
            { op: 'activate', language, name: info.name, version_id: v.id },
            {
                onSuccess: () => {
                    drafts.clear(key);
                    setPreview(null);
                },
            },
        );

    // Ctrl+S 存这一份；对话框里不接
    const saveRef = useRef<() => void>(() => {});
    saveRef.current = () => {
        if (canSave) save();
    };
    useEffect(() => {
        const onKey = (e: KeyboardEvent) => {
            if (!(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== 's' || e.altKey || e.shiftKey) return;
            if (e.target instanceof Element && e.target.closest('[role="dialog"]')) return;
            e.preventDefault();
            saveRef.current();
        };
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, []);

    if (file.isError && !f) {
        return <PaneLoadError message={`读取「${promptDisplayName(info)}」失败`} onRetry={() => void file.refetch()} />;
    }

    const changed = f ? changedLines(lineDiff(defaultContent, content)) : 0;
    const otherLanguage = language !== activeLanguage;

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-3 p-4">
            <header className="flex min-w-0 items-start gap-3">
                <div className="min-w-0 flex-1">
                    <div className="flex min-w-0 items-center gap-2">
                        <h3 className="truncate font-display text-[15px] font-semibold text-text">{promptDisplayName(info)}</h3>
                        {f?.customized ? <Badge tone="brand">已改</Badge> : <Badge tone="neutral">默认</Badge>}
                        {info.advanced && <Badge tone="neutral">高级</Badge>}
                    </div>
                    <p className="mt-0.5 line-clamp-2 text-xs leading-relaxed text-text-tertiary">
                        <span className="font-mono">{info.name}</span>
                        {info.description && ` · ${info.description}`}
                    </p>
                    {otherLanguage && (
                        <p className="mt-1 flex items-center gap-1 text-xs text-warning">
                            <AlertCircle size={12} />
                            麦麦现在用的是{promptLanguageLabel(activeLanguage)}，这一份改了也不会用上
                        </p>
                    )}
                </div>
                <div className="flex shrink-0 items-center gap-1">
                    <Button
                        size="sm"
                        variant="ghost"
                        aria-pressed={diffOn}
                        className={cn('gap-1.5 px-2', diffOn && 'bg-inset text-text')}
                        disabled={!f}
                        onClick={() => setDiffOn((v) => !v)}
                    >
                        <GitCompare size={13} />
                        对比默认
                    </Button>
                    <VersionMenu
                        versions={f?.versions ?? []}
                        busy={busy}
                        canSaveAs={!!f && !check.error}
                        onPreview={setPreview}
                        onActivate={(v) => (dirty ? setPending({ kind: 'activate', version: v }) : activate(v))}
                        onDelete={(v) => setPending({ kind: 'delete', version: v })}
                        onSaveAs={() => setSaveAsLabel('')}
                    />
                </div>
            </header>

            <ParamChips params={params} check={check} onInsert={(p) => editor.current?.insert(p)} />

            {!f ? (
                <div className="flex flex-1 items-center justify-center">
                    <Spinner size="md" tone="brand" label="正在读取提示词" />
                </div>
            ) : diffOn ? (
                <DiffView before={defaultContent} after={content} />
            ) : (
                <SyntaxTextEditor
                    key={key}
                    value={content}
                    onChange={(next) => (next === f.content ? drafts.clear(key) : drafts.set(key, next))}
                    mode="prompt"
                    wrap
                    prose
                    handleRef={editor}
                    aria-label={`提示词 ${promptDisplayName(info)}`}
                    className="bg-field"
                />
            )}

            <footer className="flex flex-wrap items-center gap-x-3 gap-y-2">
                <div className="flex min-w-0 flex-1 items-center gap-3 text-xs">
                    {check.error ? (
                        // 错误要看全，宁可折行也不截断
                        <span className="flex min-w-0 items-start gap-1 leading-relaxed text-danger">
                            <AlertCircle size={13} className="mt-[3px] shrink-0" />
                            <span className="min-w-0">{check.error}</span>
                        </span>
                    ) : (
                        <>
                            <span className="flex shrink-0 items-center gap-1 text-success">
                                <Check size={13} />
                                参数齐了
                            </span>
                            <span className="shrink-0 text-text-tertiary">
                                {content.length} 字{changed > 0 && ` · 和默认差 ${changed} 行`}
                            </span>
                            <span className="hidden min-w-0 truncate text-text-tertiary lg:inline">
                                {live ? '保存后下一次用到就生效' : '麦麦没在运行，保存后下次启动生效'}
                            </span>
                        </>
                    )}
                </div>
                <div className="flex shrink-0 items-center gap-1.5">
                    {f?.customized && (
                        <Button size="sm" variant="ghost" disabled={busy} onClick={() => setPending({ kind: 'restore' })}>
                            <RotateCcw size={13} />
                            恢复默认
                        </Button>
                    )}
                    {dirty && (
                        <Button size="sm" variant="ghost" disabled={busy} onClick={() => drafts.clear(key)}>
                            撤销改动
                        </Button>
                    )}
                    <Button size="sm" variant="primary" title="Ctrl+S" disabled={!canSave} onClick={() => save()}>
                        {busy ? <Spinner size="xs" className="text-white" /> : <Save size={13} />}
                        保存
                    </Button>
                </div>
            </footer>

            <VersionPreview
                version={preview}
                content={previewBody.data}
                loading={previewBody.isLoading}
                busy={busy}
                onClose={() => setPreview(null)}
                onActivate={(v) => (dirty ? setPending({ kind: 'activate', version: v }) : activate(v))}
            />

            {saveAsLabel !== null && (
                <FormDialog
                    open
                    size="sm"
                    title="另存为新版本"
                    description="存完就用这个新版本，原来的版本留在记录里。"
                    confirmLabel="存成新版本"
                    busy={busy}
                    onCancel={() => setSaveAsLabel(null)}
                    onConfirm={() => {
                        save(saveAsLabel.trim(), true);
                        setSaveAsLabel(null);
                    }}
                >
                    <TextField
                        label="版本名"
                        autoFocus
                        placeholder="比如：回复再短一点"
                        value={saveAsLabel}
                        onValueChange={setSaveAsLabel}
                    />
                </FormDialog>
            )}

            <ConfirmDelete
                open={pending !== null}
                busy={busy}
                title={
                    pending?.kind === 'restore'
                        ? '恢复成默认内容？'
                        : pending?.kind === 'delete'
                          ? `删掉版本「${pending.version.label}」？`
                          : `换成版本「${pending?.version.label ?? ''}」？`
                }
                description={
                    pending?.kind === 'restore'
                        ? `现在用的版本会留在记录里，随时能换回来。${dirty ? '没保存的改动会丢掉。' : ''}`
                        : pending?.kind === 'delete'
                          ? pending.version.active
                              ? '这是在用的版本，删掉后回到默认内容。'
                              : '删掉就找不回来了。'
                          : '没保存的改动会丢掉。'
                }
                confirmLabel={pending?.kind === 'restore' ? '恢复默认' : pending?.kind === 'delete' ? '删掉' : '换过去'}
                onCancel={() => setPending(null)}
                onConfirm={() => {
                    if (!pending) return;
                    // 恢复默认、删掉在用的版本会换掉内容，草稿跟着作废；删别的版本不动草稿
                    const replaces = pending.kind === 'restore' || (pending.kind === 'delete' && pending.version.active);
                    const done = {
                        onSuccess: () => replaces && drafts.clear(key),
                        onSettled: () => setPending(null),
                    };
                    if (pending.kind === 'restore') act.mutate({ op: 'restore', language, name: info.name }, done);
                    else if (pending.kind === 'delete')
                        act.mutate({ op: 'delete_version', language, name: info.name, version_id: pending.version.id }, done);
                    else {
                        drafts.clear(key);
                        activate(pending.version);
                        setPending(null);
                    }
                }}
            />
        </div>
    );
};
