// 容器引擎选择和探测一起挂载；退出容器场景后释放其订阅。
import { useMemo } from 'react';
import { Select } from '../../../../shared/ui';
import { useDockerHosts } from '../../../../hooks/docker/useDockerHosts';
import { dockerHostTargetKey } from '../../../../core/domain/docker/engine';
import { dockerReadinessNotice } from '../../../../core/domain/bot/docker-start-gate';
import type { ContainerEngine } from '../../../../core/ipc/generated/domain/ContainerEngine';
import type { BackendType } from '../../../../core/ipc/generated/domain/BackendType';
import { RuntimeNotice } from './RuntimeNotice';

export function ContainerRuntimeSettings({
    hostId,
    backend,
    engine,
    onEngineChange,
    engineLocked,
}: {
    hostId: string | null;
    backend: BackendType;
    engine?: ContainerEngine;
    onEngineChange?: (engine: ContainerEngine | undefined) => void;
    engineLocked: boolean;
}) {
    const targets = useMemo(() => (hostId ? [{ hostId, engine }] : []), [hostId, engine]);
    const { statusByHost, probingByHost, imageReadyByHost } = useDockerHosts(targets);
    const key = hostId ? dockerHostTargetKey({ hostId, engine }) : '';
    const notice = dockerReadinessNotice({
        engine,
        flavorLabel: backend === 'snowluma' ? 'SnowLuma' : 'NapCat',
        status: statusByHost[key],
        probing: probingByHost[key] ?? false,
        imageReady: imageReadyByHost[key]?.[backend],
    });
    return (
        <div className="pt-3">
            {onEngineChange && (
                <label className="mb-3 flex items-center gap-3 text-xs text-text-secondary">
                    容器引擎
                    <Select
                        aria-label="容器引擎"
                        value={engine ?? 'default'}
                        disabled={engineLocked}
                        items={[
                            { value: 'default', label: '主机当前引擎' },
                            { value: 'docker', label: 'Docker' },
                            { value: 'podman', label: 'Podman' },
                        ]}
                        onValueChange={(value) =>
                            onEngineChange(
                                value === 'default' ? undefined : (value as ContainerEngine),
                            )
                        }
                    />
                </label>
            )}
            {notice && <RuntimeNotice tone={notice.tone}>{notice.text}</RuntimeNotice>}
        </div>
    );
}
