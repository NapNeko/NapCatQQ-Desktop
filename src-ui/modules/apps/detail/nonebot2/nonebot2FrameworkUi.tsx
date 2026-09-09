import { useEffect, useRef } from 'react';
import { RefreshCw } from 'lucide-react';
import { Button, Spinner, TabsContent } from '../../../../shared/ui';
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import { NoneBot2ConnectionsTab } from './NoneBot2ConnectionsTab';
import { NoneBot2StoreTab } from './NoneBot2StoreTab';
import { useNoneBot2ConfigForm } from '../useNoneBot2ConfigForm';
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
                <TabsContent value="connections" className="pb-8 pt-2">
                    <div className="flex items-center gap-2 py-10 text-sm text-text-tertiary">
                        <Spinner size="sm" /> 读取配置…
                    </div>
                </TabsContent>
            ) : form.loadError && !form.form ? (
                <TabsContent value="connections" className="pb-8 pt-2">
                    <div className="flex flex-col items-start gap-2 py-10">
                        <p className="text-sm text-text-secondary">读取配置失败</p>
                        <Button size="sm" variant="secondary" onClick={() => void form.reloadDiscard()}>
                            <ActionMotionIcon icon={RefreshCw} size={13} />
                            重试
                        </Button>
                    </div>
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
