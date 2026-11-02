// 身份 Tab 只负责组合表单区域；各区域独立订阅自己的运行时数据。
import type { ContainerEngine } from '../../../../core/ipc/generated/domain/ContainerEngine';
import type { BotBasicConfig } from '../../../../core/ipc/generated/domain/BotBasicConfig';
import { IdentityAccountSection } from './IdentityAccountSection';
import { IdentityRuntimeSection } from './IdentityRuntimeSection';
import { IdentityRecoverySection } from './IdentityRecoverySection';

export interface IdentityTabProps {
    data: BotBasicConfig;
    onChange: (patch: Partial<BotBasicConfig>) => void;
    isEditMode: boolean;
    isRunning: boolean;
    containerEngine?: ContainerEngine;
    onContainerEngineChange?: (engine: ContainerEngine | undefined) => void;
    containerEngineLocked?: boolean;
}

export function IdentityTab(props: IdentityTabProps) {
    return (
        <div className="flex w-full flex-col gap-6">
            <IdentityAccountSection
                data={props.data}
                onChange={props.onChange}
                isEditMode={props.isEditMode}
                isRunning={props.isRunning}
            />
            <IdentityRuntimeSection {...props} />
            <IdentityRecoverySection data={props.data} onChange={props.onChange} />
        </div>
    );
}
