// 组合运行场景表单；选择决策与两类探测分别由专属模型/区域拥有。
import { CircleHelp, Monitor, Server } from 'lucide-react';
import { FormSection, Popover, PopoverContent, PopoverTrigger } from '../../../../shared/ui';
import { ExpandPresence } from '../../../../shared/ui/motion/ExpandPresence';
import { IdentityChoiceCard } from './IdentityChoiceCard';
import { RuntimeHostFields } from './RuntimeHostFields';
import { ContainerRuntimeSettings } from './ContainerRuntimeSettings';
import { NativeRuntimeNotice } from './NativeRuntimeNotice';
import { RuntimeNotice } from './RuntimeNotice';
import { SnowLumaStartModeBlock } from './SnowLumaStartModeBlock';
import { useIdentityRuntimeSelection } from './useIdentityRuntimeSelection';
import type { IdentityTabProps } from './identityConfig';

const HOST_MODES = [
    {
        value: 'local',
        label: '本机',
        summary: '当前桌面主机，Linux 可选 Docker / Podman',
        icon: Monitor,
    },
    { value: 'remote', label: '远程主机', summary: '经 SSH 部署并托管 Linux 主机', icon: Server },
] as const;

export function IdentityRuntimeSection({
    data,
    onChange,
    isRunning,
    containerEngine,
    onContainerEngineChange,
    containerEngineLocked = false,
}: Omit<IdentityTabProps, 'isEditMode'>) {
    const model = useIdentityRuntimeSelection(data, onChange);
    const { selection } = model;
    const nativeNotice = (
        <NativeRuntimeNotice
            hostId={selection.hostId}
            backend={data.backend_type}
            transportFailed={model.transportFailed}
        />
    );
    return (
        <div data-tour-id="bot-runtime-section">
            <FormSection
                title="运行场景"
                layout="none"
                description="Linux 主机支持直接运行与 Docker / Podman；Windows 使用直接运行"
                actions={
                    model.sceneExplanation ? (
                        <SceneExplainer text={model.sceneExplanation} />
                    ) : undefined
                }
            >
                <div className="flex flex-col gap-1.5">
                    <span className="text-xs font-medium text-text-secondary">运行宿主</span>
                    <div
                        role="radiogroup"
                        aria-label="运行宿主"
                        className="grid grid-cols-1 gap-3 sm:grid-cols-2"
                    >
                        {HOST_MODES.map((mode) => (
                            <IdentityChoiceCard
                                key={mode.value}
                                selected={selection.mode === mode.value}
                                onSelect={() => model.selectMode(mode.value)}
                                label={mode.label}
                                summary={mode.summary}
                                icon={mode.icon}
                            />
                        ))}
                    </div>
                </div>
                <ExpandPresence visible={selection.showHostFields}>
                    <div className="pt-3">
                        <div className="flex flex-col rounded-md border border-border-subtle/80 bg-field/30 p-3.5">
                            <ExpandPresence visible={model.noRemoteHosts}>
                                <div className="pb-3">
                                    <RuntimeNotice tone="warn">
                                        请先在「远程主机」页添加 SSH 主机；当前配置仍按远程保存
                                    </RuntimeNotice>
                                </div>
                            </ExpandPresence>
                            <RuntimeHostFields
                                selection={selection}
                                serverItems={model.serverItems}
                                onHostChange={model.selectHost}
                                onDeploymentChange={model.selectDeployment}
                            />
                            <ExpandPresence visible={selection.showContainer}>
                                <ContainerRuntimeSettings
                                    hostId={selection.dockerHostId}
                                    backend={data.backend_type}
                                    engine={containerEngine}
                                    onEngineChange={onContainerEngineChange}
                                    engineLocked={containerEngineLocked || isRunning}
                                />
                            </ExpandPresence>
                            <ExpandPresence
                                visible={
                                    selection.isRemote &&
                                    selection.deploymentType === 'native' &&
                                    selection.hostId != null
                                }
                            >
                                {nativeNotice}
                            </ExpandPresence>
                        </div>
                    </div>
                </ExpandPresence>
                <ExpandPresence
                    visible={!selection.isRemote && selection.deploymentType === 'native'}
                >
                    {nativeNotice}
                </ExpandPresence>
                <ExpandPresence visible={selection.showSnowLumaMode}>
                    <div className="flex flex-col gap-1.5 pt-3">
                        <span className="text-xs font-medium text-text-secondary">
                            SnowLuma 启动模式
                        </span>
                        <SnowLumaStartModeBlock data={data} onChange={onChange} />
                    </div>
                </ExpandPresence>
            </FormSection>
        </div>
    );
}

function SceneExplainer({ text }: { text: string }) {
    return (
        <Popover>
            <PopoverTrigger asChild>
                <button
                    type="button"
                    className="inline-flex h-6 items-center gap-1 rounded-sm px-1.5 text-[11px] text-text-tertiary transition-colors hover:bg-inset hover:text-text"
                >
                    <CircleHelp size={12} />
                    <span>机制说明</span>
                </button>
            </PopoverTrigger>
            <PopoverContent side="bottom" align="end" sideOffset={6}>
                <p className="max-w-xs text-[12px] leading-relaxed text-text-secondary">{text}</p>
            </PopoverContent>
        </Popover>
    );
}
