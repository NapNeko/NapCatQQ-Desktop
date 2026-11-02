import type { ContainerEngine } from '../../../../core/ipc/generated/domain/ContainerEngine';
import type { BotBasicConfig } from '../../../../core/ipc/generated/domain/BotBasicConfig';

export interface IdentityTabProps {
    data: BotBasicConfig;
    onChange: (patch: Partial<BotBasicConfig>) => void;
    isEditMode: boolean;
    isRunning: boolean;
    containerEngine?: ContainerEngine;
    onContainerEngineChange?: (engine: ContainerEngine | undefined) => void;
    containerEngineLocked?: boolean;
}
