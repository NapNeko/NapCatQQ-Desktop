// 所有框架共用的配置文件编辑入口。
import { useState } from 'react';
import { RefreshCw, Save } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    PagePlaceholder,
    Select,
    SyntaxTextEditor,
} from '../../../shared/ui';
import { ActionMotionIcon } from '../../../shared/ui/motion';
import { useAppConfigDocuments, useAppConfigFile } from '../../../hooks/apps/useAppConfigFiles';
import { toAppConfigError } from '../../../core/domain/apps/appConfigError';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import type { AppConfigDocument, AppInstance } from '../../../core/ipc/types';
import { PaneLoading, PaneLoadError } from './PaneStatus';

export function InstanceConfigEditor({ instance }: { instance: AppInstance }) {
    const documents = useAppConfigDocuments(instance.id);
    const [selectedId, setSelectedId] = useState<string>();
    const selected = documents.data?.find((doc) => doc.id === selectedId) ?? documents.data?.[0];

    if (documents.isLoading) return <PaneLoading text="正在读取配置文件…" />;
    if (documents.error)
        return (
            <PaneLoadError
                message={toAppConfigError(documents.error).message}
                onRetry={() => void documents.refetch()}
            />
        );
    if (!selected) return <PagePlaceholder>此框架没有可编辑的配置文件</PagePlaceholder>;

    return (
        <ConfigFileEditor
            key={`${instance.id}:${selected.id}:${selected.rel_path}`}
            instance={instance}
            document={selected}
            documents={documents.data ?? []}
            onSelect={setSelectedId}
        />
    );
}

function ConfigFileEditor({
    instance,
    document,
    documents,
    onSelect,
}: {
    instance: AppInstance;
    document: AppConfigDocument;
    documents: AppConfigDocument[];
    onSelect: (id: string) => void;
}) {
    const file = useAppConfigFile(instance.id, document);
    const [confirm, setConfirm] = useState<'reload' | 'conflict' | null>(null);

    const save = async (overwrite = false) => {
        try {
            await file.save(overwrite);
            setConfirm(null);
            pushInfoBar({
                key: `app-config-saved:${instance.id}`,
                tone: 'success',
                title: `${document.label} 已保存`,
                content:
                    instance.state !== 'running'
                        ? '下次启动实例时生效。'
                        : (document.reload_note ??
                          (document.hot_reload
                              ? '支持自动热重载，生效情况请查看实例日志。'
                              : '重启实例后生效。')),
                autoDismissMs: 7000,
            });
        } catch (error) {
            if (toAppConfigError(error).kind === 'conflict') setConfirm('conflict');
            else setConfirm(null);
        }
    };
    const reload = async () => {
        try {
            await file.reload();
            setConfirm(null);
        } catch {
            setConfirm(null);
        }
    };

    return (
        <div className="flex min-h-0 flex-1 flex-col gap-3 pb-3">
            <div className="flex flex-wrap items-center gap-2">
                <Select
                    aria-label="配置文件"
                    className="min-w-48 flex-1"
                    items={documents.map((doc) => ({ value: doc.id, label: doc.label }))}
                    value={document.id}
                    onValueChange={onSelect}
                    disabled={file.busy}
                />
                {file.dirty && <span className="text-xs text-warning">未保存</span>}
                <Button
                    size="sm"
                    variant="secondary"
                    disabled={file.busy}
                    onClick={() => (file.dirty ? setConfirm('reload') : void reload())}
                >
                    <ActionMotionIcon icon={RefreshCw} size={13} />
                    重新加载
                </Button>
                <Button
                    size="sm"
                    variant="primary"
                    disabled={file.busy || !file.dirty}
                    onClick={() => void save()}
                >
                    <ActionMotionIcon icon={Save} size={13} />
                    保存
                </Button>
            </div>
            <div className="flex items-center justify-between gap-3 text-xs text-text-tertiary">
                <span
                    className="truncate font-mono"
                    title={`${instance.install_dir}/${document.rel_path}`}
                >
                    {document.rel_path}
                </span>
                <span className="shrink-0">
                    {file.base?.revision === 'missing'
                        ? '保存后创建文件'
                        : document.format === 'dot_env'
                          ? 'ENV'
                          : document.format.toUpperCase()}
                </span>
            </div>
            {file.error && (
                <p role="alert" className="whitespace-pre-wrap text-sm text-danger">
                    {file.error.message}
                </p>
            )}
            {file.isLoading && !file.base ? (
                <PaneLoading text="正在读取文件…" />
            ) : file.base ? (
                <SyntaxTextEditor
                    value={file.text}
                    onChange={file.change}
                    mode={document.format}
                    disabled={file.busy}
                    invalid={file.error?.kind === 'invalid'}
                    aria-label={`${document.rel_path} 配置原文`}
                />
            ) : null}
            <Dialog
                open={confirm !== null}
                onOpenChange={(open) => !open && !file.busy && setConfirm(null)}
            >
                <DialogContent size="sm">
                    <DialogHeader>
                        <DialogTitle>
                            {confirm === 'conflict' ? '配置文件已被修改' : '放弃未保存的修改？'}
                        </DialogTitle>
                        <DialogDescription>
                            {confirm === 'conflict'
                                ? '重新加载会丢弃当前草稿；覆盖保存会替换文件里的最新内容。'
                                : '重新加载将用文件里的内容替换当前草稿。'}
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button
                            size="sm"
                            variant="ghost"
                            disabled={file.busy}
                            onClick={() => setConfirm(null)}
                        >
                            取消
                        </Button>
                        <Button
                            size="sm"
                            variant="secondary"
                            disabled={file.busy}
                            onClick={() => void reload()}
                        >
                            重新加载
                        </Button>
                        {confirm === 'conflict' && (
                            <Button
                                size="sm"
                                variant="primary"
                                disabled={file.busy}
                                onClick={() => void save(true)}
                            >
                                覆盖保存
                            </Button>
                        )}
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </div>
    );
}
