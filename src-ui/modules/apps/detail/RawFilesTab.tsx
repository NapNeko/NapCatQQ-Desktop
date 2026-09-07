// 「原始文件」Tab：左侧文件条 + 右侧语法高亮编辑（与设置页 JSON 编辑器同一套）。

import { useEffect, useMemo, useState } from 'react';
import { RefreshCw, Save } from 'lucide-react';
import { Button, Spinner, SyntaxTextEditor, type SyntaxMode } from '../../../shared/ui';
import { ActionMotionIcon } from '../../../shared/ui/motion';
import { useAppConfigDocuments, useAppConfigText } from '../../../hooks/apps/useAppInstanceConfig';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { cn } from '../../../shared/utils/cn';
import { ConfigConflictDialog } from './ConfigConflictDialog';
import type { AppConfigDocument, AppConfigError, AppInstance } from '../../../core/ipc/types';

const FORMAT_LABEL: Record<AppConfigDocument['format'], string> = {
    json: 'JSON',
    dot_env: 'dotenv',
    toml: 'TOML',
};

function editorMode(format: AppConfigDocument['format']): SyntaxMode {
    if (format === 'json' || format === 'dot_env' || format === 'toml') return format;
    return 'plain';
}

export const RawFilesTab: React.FC<{ instance: AppInstance }> = ({ instance }) => {
    const docsQuery = useAppConfigDocuments(instance.id);
    const docs = useMemo(() => docsQuery.data ?? [], [docsQuery.data]);
    const [selectedId, setSelectedId] = useState<string | null>(null);
    const activeId = selectedId ?? docs[0]?.id ?? null;
    const activeDoc = docs.find((d) => d.id === activeId) ?? null;

    const text = useAppConfigText(instance.id, activeId);
    const [draft, setDraft] = useState('');
    const [loaded, setLoaded] = useState<{ docId: string; revision: string } | null>(null);
    const [syntaxError, setSyntaxError] = useState<string | null>(null);
    const [conflict, setConflict] = useState(false);

    const sameDocLoaded = loaded !== null && loaded.docId === text.doc?.doc_id;
    const dirty = sameDocLoaded && text.doc != null && draft !== text.doc.text;

    useEffect(() => {
        if (!text.doc) return;
        if (sameDocLoaded && loaded!.revision === text.doc.revision) return;
        if (sameDocLoaded && dirty) return;
        setDraft(text.doc.text);
        setLoaded({ docId: text.doc.doc_id, revision: text.doc.revision });
        setSyntaxError(null);
    }, [text.doc, sameDocLoaded, dirty, loaded]);

    useEffect(() => {
        setSyntaxError(null);
    }, [activeId]);

    const precheck = (): boolean => {
        if (!activeDoc) return false;
        if (activeDoc.format === 'json') {
            try {
                JSON.parse(draft);
            } catch (e) {
                setSyntaxError(`JSON 语法错误：${(e as Error).message}`);
                return false;
            }
        }
        setSyntaxError(null);
        return true;
    };

    const save = async (overwrite = false) => {
        if (!activeDoc || !text.doc) return;
        if (!precheck()) return;
        try {
            const saved = await text.write({
                text: draft,
                baseRevision: overwrite ? null : (loaded?.revision ?? text.doc.revision),
            });
            setLoaded({ docId: saved.doc_id, revision: saved.revision });
            setConflict(false);
            pushInfoBar({
                key: `app-raw-saved:${instance.id}:${activeDoc.id}`,
                tone: activeDoc.hot_reload ? 'success' : 'warning',
                title: `${activeDoc.label} 已保存`,
                content: activeDoc.hot_reload
                    ? '应用端会自动热加载'
                    : '该文件不热加载，需重启实例后生效',
                autoDismissMs: activeDoc.hot_reload ? 3000 : 6000,
            });
        } catch (e) {
            const err = e as AppConfigError;
            if (err.kind === 'conflict') {
                setConflict(true);
                return;
            }
            if (err.kind === 'invalid') {
                setSyntaxError(err.issues[0]?.message ?? err.message);
                return;
            }
            pushInfoBar({
                key: `app-raw-save-failed:${instance.id}`,
                tone: 'danger',
                title: '保存失败',
                content: err.message,
            });
        }
    };

    const reload = async () => {
        setLoaded(null);
        setConflict(false);
        await text.reload();
    };

    if (docsQuery.isLoading) {
        return (
            <div className="flex items-center gap-2 py-8 text-sm text-text-tertiary">
                <Spinner size="sm" /> 读取文档列表…
            </div>
        );
    }
    if (docs.length === 0) {
        return <p className="py-8 text-sm text-text-tertiary">该应用端没有可编辑的配置文件。</p>;
    }

    return (
        <div className="flex min-h-0 flex-1 gap-3">
            <aside className="w-44 shrink-0 overflow-y-auto border-r border-border-subtle pr-2">
                <ul className="flex flex-col">
                    {docs.map((d) => (
                        <li key={d.id}>
                            <button
                                type="button"
                                onClick={() => setSelectedId(d.id)}
                                className={cn(
                                    'flex w-full items-baseline justify-between gap-2 rounded-sm px-2 py-1.5 text-left transition-colors',
                                    d.id === activeId
                                        ? 'bg-brand-soft/70 text-text'
                                        : 'text-text-secondary hover:bg-inset hover:text-text',
                                )}
                            >
                                <span className="min-w-0 truncate font-mono text-[12px]">{d.label}</span>
                                <span className="shrink-0 text-2xs text-text-tertiary">
                                    {FORMAT_LABEL[d.format]}
                                    {d.hot_reload ? '' : ' · 重启'}
                                </span>
                            </button>
                        </li>
                    ))}
                </ul>
            </aside>

            <section className="flex min-h-0 min-w-0 flex-1 flex-col gap-2">
                <div className="flex shrink-0 items-center justify-between gap-3">
                    <p
                        className="min-w-0 truncate font-mono text-[11px] text-text-tertiary"
                        title={`${instance.install_dir}/${activeDoc?.rel_path ?? ''}`}
                    >
                        {activeDoc?.rel_path}
                        {text.doc?.revision === 'missing' ? ' · 保存时创建' : ''}
                    </p>
                    <div className="flex shrink-0 items-center gap-1.5">
                        <span className="text-2xs text-text-tertiary">
                            {dirty ? '未保存' : text.doc ? `版本 ${text.doc.revision}` : ''}
                        </span>
                        <Button size="sm" variant="ghost" onClick={() => void reload()} disabled={text.isLoading || text.isWriting}>
                            <ActionMotionIcon icon={RefreshCw} size={13} />
                            重载
                        </Button>
                        <Button size="sm" variant="primary" onClick={() => void save()} disabled={!dirty || text.isWriting}>
                            {text.isWriting ? <Spinner size="xs" /> : <ActionMotionIcon icon={Save} size={13} />}
                            保存
                        </Button>
                    </div>
                </div>
                {text.error && <p className="shrink-0 text-xs text-danger">读取失败：{text.error.message}</p>}
                {syntaxError && <p className="shrink-0 text-xs text-danger">{syntaxError}</p>}
                <SyntaxTextEditor
                    mode={activeDoc ? editorMode(activeDoc.format) : 'plain'}
                    value={draft}
                    invalid={!!syntaxError}
                    disabled={text.isLoading || !text.doc}
                    aria-label={activeDoc?.rel_path ?? '配置文件'}
                    onChange={(next) => {
                        setDraft(next);
                        if (syntaxError) setSyntaxError(null);
                    }}
                />
            </section>

            <ConfigConflictDialog
                open={conflict}
                busy={text.isWriting}
                what={activeDoc?.label}
                onCancel={() => setConflict(false)}
                onReload={() => void reload()}
                onOverwrite={() => void save(true)}
            />
        </div>
    );
};
