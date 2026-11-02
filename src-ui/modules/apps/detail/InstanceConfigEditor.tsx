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
    if (documents.error && !documents.data)
        return (
            <PaneLoadError
                message="配置文件列表暂不可用"
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
            retryDocuments={
                documents.error ? () => documents.refetch({ throwOnError: true }) : undefined
            }
        />
    );
}

function ConfigFileEditor({
    instance,
    document,
    documents,
    onSelect,
    retryDocuments,
}: {
    instance: AppInstance;
    document: AppConfigDocument;
    documents: AppConfigDocument[];
    onSelect: (id: string) => void;
    retryDocuments?: () => Promise<unknown>;
}) {
    const file = useAppConfigFile(instance.id, document);
    const [confirm, setConfirm] = useState<'reload' | 'conflict' | null>(null);
    const [reloading, setReloading] = useState(false);
    const busy = file.busy || reloading;

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
        setReloading(true);
        try {
            await retryDocuments?.();
            await file.reload();
        } catch {
            // 读取失败由 hook 提示，保留当前草稿。
        } finally {
            setConfirm(null);
            setReloading(false);
        }
    };

    return (
        <div className="flex min-h-0 flex-1 flex-col gap-3 pb-3">
            <div className="flex flex-wrap items-center gap-2">
                <Select
                    size="sm"
                    aria-label="配置文件"
                    title={`${instance.install_dir}/${document.rel_path}`}
                    className="min-w-48 flex-1"
                    items={documents.map((doc) => ({ value: doc.id, label: doc.label }))}
                    value={document.id}
                    onValueChange={onSelect}
                    disabled={busy}
                />
                {file.dirty && <span className="text-xs text-warning">未保存</span>}
                <Button
                    size="sm"
                    variant="secondary"
                    disabled={busy}
                    onClick={() => (file.dirty ? setConfirm('reload') : void reload())}
                >
                    <ActionMotionIcon icon={RefreshCw} size={13} />
                    重新加载
                </Button>
                <Button
                    size="sm"
                    variant="primary"
                    disabled={busy || !file.dirty}
                    onClick={() => void save()}
                >
                    <ActionMotionIcon icon={Save} size={13} />
                    {file.base?.revision === 'missing' ? '创建并保存' : '保存'}
                </Button>
            </div>
            {file.isLoading && !file.base ? (
                <PaneLoading text="正在读取文件…" />
            ) : file.base ? (
                <SyntaxTextEditor
                    value={file.text}
                    onChange={file.change}
                    mode={document.format}
                    disabled={busy}
                    invalid={file.error?.kind === 'invalid'}
                    aria-label={`${document.rel_path} 配置原文`}
                />
            ) : null}
            <Dialog
                open={confirm !== null}
                onOpenChange={(open) => !open && !busy && setConfirm(null)}
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
                            disabled={busy}
                            onClick={() => setConfirm(null)}
                        >
                            取消
                        </Button>
                        <Button
                            size="sm"
                            variant="secondary"
                            disabled={busy}
                            onClick={() => void reload()}
                        >
                            重新加载
                        </Button>
                        {confirm === 'conflict' && (
                            <Button
                                size="sm"
                                variant="primary"
                                disabled={busy}
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
