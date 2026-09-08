// 插件配置：一插件一页。Karin WebUI 走 web.config 表单，Desktop 改文件本身。

import { useEffect, useState } from 'react';
import { FileCode, RefreshCw, Save } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Spinner,
    SyntaxTextEditor,
    type SyntaxMode,
} from '../../../../shared/ui';
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import { useAppConfigText } from '../../../../hooks/apps/useAppInstanceConfig';
import { appFrameworkService } from '../../../../core/services/app-framework.service';
import { pushInfoBar } from '../../../../hooks/ui/globalInfoBarStore';
import { pushAppErrorBar } from '../../../../hooks/apps/pushAppErrorBar';
import { errorText } from '../../../../core/domain/errors';
import { cn } from '../../../../shared/utils/cn';
import { ConfigConflictDialog } from '../ConfigConflictDialog';
import type { AppConfigDocument, AppConfigError } from '../../../../core/ipc/types';

const WORKSPACE = 'flex h-[min(64dvh,560px)] min-h-[22rem] min-w-0 flex-1 flex-col overflow-hidden';

function editorMode(format: AppConfigDocument['format']): SyntaxMode {
    if (format === 'json' || format === 'dot_env' || format === 'toml') return format;
    return 'plain';
}

export const KarinPluginConfigDialog: React.FC<{
    instanceId: string;
    pluginName: string | null;
    onClose: () => void;
}> = ({ instanceId, pluginName, onClose }) => {
    const [docs, setDocs] = useState<AppConfigDocument[]>([]);
    const [docsError, setDocsError] = useState<string | null>(null);
    const [docsLoading, setDocsLoading] = useState(false);
    const [selectedId, setSelectedId] = useState<string | null>(null);

    useEffect(() => {
        if (!pluginName) {
            setDocs([]);
            setSelectedId(null);
            setDocsError(null);
            return;
        }
        let cancelled = false;
        setDocsLoading(true);
        setDocsError(null);
        void appFrameworkService
            .listPluginConfigDocs(instanceId, pluginName)
            .then((next) => {
                if (cancelled) return;
                setDocs(next);
                setSelectedId(next[0]?.id ?? null);
            })
            .catch((e) => {
                if (cancelled) return;
                const raw = errorText(e);
                setDocsError(raw);
                pushAppErrorBar({
                    key: `plugin-cfg-docs:${instanceId}:${pluginName}`,
                    title: '读取插件配置失败',
                    raw,
                });
            })
            .finally(() => {
                if (!cancelled) setDocsLoading(false);
            });
        return () => {
            cancelled = true;
        };
    }, [instanceId, pluginName]);

    const active = docs.find((d) => d.id === selectedId) ?? docs[0] ?? null;

    return (
        <Dialog open={pluginName !== null} onOpenChange={(open) => !open && onClose()}>
            <DialogContent size="sheet" className="gap-0">
                <DialogHeader className="mb-0 shrink-0 pr-8">
                    <DialogTitle>配置</DialogTitle>
                    {pluginName ? (
                        <DialogDescription className="truncate font-mono text-xs">
                            {pluginName}
                        </DialogDescription>
                    ) : (
                        <DialogDescription className="sr-only">插件配置</DialogDescription>
                    )}
                </DialogHeader>

                {docsLoading ? (
                    <div className={cn(WORKSPACE, 'items-center justify-center gap-2 text-sm text-text-tertiary')}>
                        <Spinner size="sm" /> 读取配置…
                    </div>
                ) : docsError ? (
                    <div className={cn(WORKSPACE, 'items-center justify-center px-2')}>
                        <p className="text-sm text-text-secondary">读取失败</p>
                    </div>
                ) : docs.length === 0 ? (
                    <div className={cn(WORKSPACE, 'items-center justify-center gap-2 text-center')}>
                        <ActionMotionIcon icon={FileCode} size={28} className="text-text-tertiary" />
                        <p className="text-sm text-text-secondary">还没有配置文件</p>
                    </div>
                ) : (
                    <PluginConfigWorkspace
                        instanceId={instanceId}
                        docs={docs}
                        active={active}
                        onSelect={setSelectedId}
                    />
                )}
            </DialogContent>
        </Dialog>
    );
};

const PluginConfigWorkspace: React.FC<{
    instanceId: string;
    docs: AppConfigDocument[];
    active: AppConfigDocument | null;
    onSelect: (id: string) => void;
}> = ({ instanceId, docs, active, onSelect }) => {
    const text = useAppConfigText(instanceId, active?.id ?? null);
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
    }, [active?.id]);

    useEffect(() => {
        if (!text.error) return;
        pushAppErrorBar({
            key: `plugin-cfg-read:${instanceId}:${active?.id ?? ''}`,
            title: '读取插件配置失败',
            raw: text.error.message,
        });
    }, [active?.id, instanceId, text.error]);

    const precheck = (): boolean => {
        if (!active) return false;
        if (active.format === 'json') {
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
        if (!active || !text.doc) return;
        if (!precheck()) return;
        try {
            const saved = await text.write({
                text: draft,
                baseRevision: overwrite ? null : (loaded?.revision ?? text.doc.revision),
            });
            setLoaded({ docId: saved.doc_id, revision: saved.revision });
            setConflict(false);
            pushInfoBar({
                key: `plugin-cfg:${instanceId}:${active.id}`,
                tone: 'success',
                title: `${active.label} 已保存`,
                content: active.hot_reload ? undefined : '改完需重启',
                autoDismissMs: 3000,
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
            pushAppErrorBar({
                key: `plugin-cfg-save:${instanceId}:${active.id}`,
                title: '保存失败',
                raw: err.message,
            });
        }
    };

    const reload = async () => {
        setLoaded(null);
        setConflict(false);
        await text.reload();
    };

    return (
        <>
            <div className={cn(WORKSPACE, 'pt-3')}>
                {docs.length > 1 && (
                    <div className="mb-2 flex shrink-0 flex-wrap gap-1">
                        {docs.map((d) => (
                            <button
                                key={d.id}
                                type="button"
                                onClick={() => onSelect(d.id)}
                                className={cn(
                                    'rounded-sm px-2 py-1 font-mono text-[12px] transition-colors',
                                    d.id === active?.id
                                        ? 'bg-brand-soft/70 text-text'
                                        : 'text-text-secondary hover:bg-inset hover:text-text',
                                )}
                            >
                                {d.label}
                            </button>
                        ))}
                    </div>
                )}
                {syntaxError && <p className="mb-2 shrink-0 text-xs text-danger">{syntaxError}</p>}
                <SyntaxTextEditor
                    mode={active ? editorMode(active.format) : 'plain'}
                    value={draft}
                    wrap
                    invalid={!!syntaxError}
                    disabled={text.isLoading || !text.doc}
                    aria-label={active?.label ?? '配置'}
                    onChange={(next) => {
                        setDraft(next);
                        if (syntaxError) setSyntaxError(null);
                    }}
                />
            </div>

            <DialogFooter className="mt-3 mb-0 shrink-0 border-t border-border-subtle pt-3">
                <span className="mr-auto text-2xs text-text-tertiary">{dirty ? '未保存' : ''}</span>
                <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => void reload()}
                    disabled={text.isLoading || text.isWriting}
                >
                    <ActionMotionIcon icon={RefreshCw} size={13} />
                    重载
                </Button>
                <Button
                    size="sm"
                    variant="primary"
                    onClick={() => void save()}
                    disabled={!dirty || text.isWriting}
                >
                    {text.isWriting ? <Spinner size="xs" /> : <ActionMotionIcon icon={Save} size={13} />}
                    保存
                </Button>
            </DialogFooter>

            <ConfigConflictDialog
                open={conflict}
                busy={text.isWriting}
                what={active?.label}
                onCancel={() => setConflict(false)}
                onReload={() => void reload()}
                onOverwrite={() => void save(true)}
            />
        </>
    );
};
