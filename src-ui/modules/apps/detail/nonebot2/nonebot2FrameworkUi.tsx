import { useEffect, useRef } from 'react';
import { TabsContent } from '../../../../shared/ui';
import { NoneBot2ConnectionsTab } from './NoneBot2ConnectionsTab';
import { NoneBot2StoreTab } from './NoneBot2StoreTab';
import { useNoneBot2ConfigForm } from '../useNoneBot2ConfigForm';
import { PaneLoadError, PaneLoading } from '../PaneStatus';
import type { FrameworkDetailProps, FrameworkUiModule } from '../frameworkUi';

const EXTRA_TABS = [
    { value: 'adapters', label: '适配器' },
    { value: 'plugins', label: '插件' },
    { value: 'connections', label: '连接' },
] as const;

const TYPED_TABS = new Set(['connections']);
const FILL_PANE = new Set(['adapters', 'plugins']);

function NoneBot2FrameworkDetail({ instance, onSaveHandle }: FrameworkDetailProps) {
    const form = useNoneBot2ConfigForm(instance.id, true, instance.display_name);
    const onSaveHandleRef = useRef(onSaveHandle);
    onSaveHandleRef.current = onSaveHandle;

    useEffect(() => {
        onSaveHandleRef.current({
            dirty: form.dirty,
            saving: form.saving,
            issueCount: form.clientIssues.length,
            save: form.save,
            reset: form.reset,
            conflict: form.conflict,
            dismissConflict: form.dismissConflict,
            reloadDiscard: form.reloadDiscard,
        });
    }, [
        form.dirty,
        form.saving,
        form.clientIssues.length,
        form.save,
        form.reset,
        form.conflict,
        form.dismissConflict,
        form.reloadDiscard,
    ]);

    useEffect(() => () => onSaveHandleRef.current(null), []);

    return (
        <>
            <TabsContent value="adapters" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                <NoneBot2StoreTab instance={instance} resource="adapter" />
            </TabsContent>
            <TabsContent value="plugins" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                <NoneBot2StoreTab instance={instance} resource="plugin" />
            </TabsContent>
            {form.isLoading && !form.form ? (
                <TabsContent value="connections" className="flex min-h-0 flex-1 flex-col pt-2">
                    <PaneLoading text="正在读取配置…" />
                </TabsContent>
            ) : form.loadError && !form.form ? (
                <TabsContent value="connections" className="flex min-h-0 flex-1 flex-col pt-2">
                    <PaneLoadError
                        message="读取配置失败"
                        onRetry={() => void form.reloadDiscard()}
                    />
                </TabsContent>
            ) : form.form ? (
                <TabsContent value="connections" className="pb-8 pt-2">
                    <NoneBot2ConnectionsTab
                        config={form.form}
                        onChange={form.setForm}
                        errors={form.errors}
                        linked={!!instance.link}
                        disabled={form.saving}
                    />
                </TabsContent>
            ) : null}
        </>
    );
}

export const nonebot2FrameworkUi: FrameworkUiModule = {
    extraTabs: EXTRA_TABS,
    defaultTab: 'adapters',
    typedTabs: TYPED_TABS,
    fillPaneTabs: FILL_PANE,
    tabForIssue: () => 'connections',
    Detail: NoneBot2FrameworkDetail,
};
