// 网络和数据卷管理列表。
import { HardDrive, Network, Trash2 } from 'lucide-react';
import { Badge, Button } from '../../shared/ui';
import type { DockerNetworkInfo, DockerVolumeInfo } from '../../core/ipc/types';
import type { RemovalTarget } from './ResourceRemoveDialog';

export function DockerResourceList({
    kind,
    resources,
    disabled,
    onRemove,
}: {
    kind: 'network' | 'volume';
    resources: (DockerNetworkInfo | DockerVolumeInfo)[];
    disabled: boolean;
    onRemove: (target: RemovalTarget) => void;
}) {
    const Icon = kind === 'network' ? Network : HardDrive;
    return (
        <div className="space-y-2">
            {resources.map((resource) => {
                const builtIn =
                    kind === 'network' && ['bridge', 'host', 'none'].includes(resource.name);
                const id = 'id' in resource ? resource.id : resource.name;
                return (
                    <article
                        key={id}
                        className="flex items-center gap-3 rounded-md border border-border-subtle bg-surface p-4"
                    >
                        <Icon size={20} className="shrink-0 text-brand" />
                        <div className="min-w-0 flex-1">
                            <h3 className="break-all text-sm font-medium text-text">
                                {resource.name}
                            </h3>
                            <p className="mt-1 text-xs text-text-tertiary">
                                {resource.driver} · {resource.scope || 'local'}
                            </p>
                            {'id' in resource && (
                                <p
                                    className="mt-1 truncate font-mono text-xs text-text-tertiary"
                                    title={id}
                                >
                                    {id}
                                </p>
                            )}
                        </div>
                        {builtIn ? (
                            <Badge tone="neutral">内置网络</Badge>
                        ) : (
                            <Button
                                size="sm"
                                variant="ghost"
                                disabled={disabled}
                                className="text-danger"
                                onClick={() => onRemove({ kind, id, name: resource.name })}
                            >
                                <Trash2 size={14} />
                                删除
                            </Button>
                        )}
                    </article>
                );
            })}
        </div>
    );
}
