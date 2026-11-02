// 身份 Tab 只负责组合表单区域；各区域独立订阅自己的运行时数据。
import { IdentityAccountSection } from './IdentityAccountSection';
import { IdentityRuntimeSection } from './IdentityRuntimeSection';
import { IdentityRecoverySection } from './IdentityRecoverySection';

import type { IdentityTabProps } from './identityConfig';

export function IdentityTab(props: IdentityTabProps) {
    return (
        <div className="flex w-full flex-col gap-6">
            <IdentityAccountSection
                data={props.data}
                onChange={props.onChange}
                isEditMode={props.isEditMode}
                isRunning={props.isRunning}
            />
            <IdentityRuntimeSection
                data={props.data}
                onChange={props.onChange}
                isRunning={props.isRunning}
                containerEngine={props.containerEngine}
                onContainerEngineChange={props.onContainerEngineChange}
                containerEngineLocked={props.containerEngineLocked}
            />
            <IdentityRecoverySection data={props.data} onChange={props.onChange} />
        </div>
    );
}
