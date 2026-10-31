import { useEffect } from 'react';
import { useContainerEngine } from '../../hooks/docker/useContainerEngine';
import { pushErrorBar } from '../../hooks/ui/pushErrorBar';
import { errorText } from '../../core/domain/errors';
import type { ContainerEngine } from '../../core/ipc/generated/domain/ContainerEngine';

export function ContainerEngineSelect({
    hostId,
    disabled,
    onEngine,
}: {
    hostId: string;
    disabled: boolean;
    onEngine: (engine: ContainerEngine) => void;
}) {
    const state = useContainerEngine(hostId);
    useEffect(() => onEngine(state.engine), [onEngine, state.engine]);
    return (
        <label className="mb-2 flex items-center gap-2 text-xs text-text-secondary">
            容器引擎
            <select
                aria-label="选择容器引擎"
                value={state.engine}
                disabled={disabled || state.loading || state.switching}
                className="rounded-sm border border-border-subtle bg-canvas px-2 py-1 text-text"
                onChange={(event) =>
                    void state.select(event.target.value as ContainerEngine).catch((error) =>
                        pushErrorBar({
                            key: `container-engine:${hostId}`,
                            title: '无法切换容器引擎',
                            raw: errorText(error),
                        }),
                    )
                }
            >
                <option value="docker">Docker</option>
                <option value="podman">Podman</option>
            </select>
            <span>各引擎的容器和镜像独立</span>
        </label>
    );
}
