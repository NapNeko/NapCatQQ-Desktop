import { useEffect, useRef } from 'react';
import { TabsContent } from '../../../../shared/ui';
import { KarinBasicTab } from './KarinBasicTab';
import { KarinConnectionsTab } from './KarinConnectionsTab';
import { KarinPermissionsTab } from './KarinPermissionsTab';
import { KarinPluginsTab } from './KarinPluginsTab';
import { KarinRenderStorageTab } from './KarinRenderStorageTab';
import { KarinRulesTab } from './KarinRulesTab';
import { useKarinConfigForm } from '../useKarinConfigForm';
import { PaneLoadError, PaneLoading } from '../PaneStatus';
import type { FrameworkDetailProps, FrameworkUiModule } from '../frameworkUi';

const TYPED_TAB_VALUES = ['basic', 'permissions', 'connections', 'rules', 'render'] as const;

const EXTRA_TABS = [
    { value: 'basic', label: '基础' },
    { value: 'permissions', label: '权限' },
    { value: 'connections', label: '连接' },
    { value: 'rules', label: '响应规则' },
    { value: 'render', label: '渲染与存储' },
    { value: 'plugins', label: '插件' },
] as const;

const TYPED_TABS = new Set(['basic', 'permissions', 'connections', 'rules', 'render']);
const FILL_PANE = new Set(['plugins']);

function tabForIssue(path: string): string {
    const root = path.split('/')[0];
    switch (root) {
        case 'config':
            return 'permissions';
        case 'adapter':
            return 'connections';
        case 'groups':
        case 'privates':
            return 'rules';
        case 'render':
        case 'redis':
            return 'render';
        case 'env':
            if (path.startsWith('env/http_') || path.startsWith('env/ws_server_auth_key')) {
                return 'connections';
            }
            return 'basic';
        default:
            return 'basic';
    }
}

function KarinFrameworkDetail({ instance, onSaveHandle }: FrameworkDetailProps) {
    const form = useKarinConfigForm(instance.id, true, instance.display_name);
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

    if (form.isLoading && !form.form) {
        return (
            <>
                {TYPED_TAB_VALUES.map((value) => (
                    <TabsContent key={value} value={value} className="flex min-h-0 flex-1 flex-col pt-2">
                        <PaneLoading text="正在读取配置…" />
                    </TabsContent>
                ))}
                <PluginsPane instance={instance} />
            </>
        );
    }
    if (form.loadError && !form.form) {
        return (
            <>
                {TYPED_TAB_VALUES.map((value) => (
                    <TabsContent key={value} value={value} className="flex min-h-0 flex-1 flex-col pt-2">
                        <PaneLoadError
                            message="读取配置失败"
                            onRetry={() => void form.reloadDiscard()}
                        />
                    </TabsContent>
                ))}
                <PluginsPane instance={instance} />
            </>
        );
    }
    if (!form.form) return <PluginsPane instance={instance} />;

    const tabProps = {
        config: form.form,
        onChange: form.setForm,
        errors: form.errors,
        linked: !!instance.link,
        disabled: form.saving,
    };

    return (
        <>
            <TabsContent value="basic" className="pb-8 pt-2">
                <KarinBasicTab {...tabProps} />
            </TabsContent>
            <TabsContent value="permissions" className="pb-8 pt-2">
                <KarinPermissionsTab {...tabProps} />
            </TabsContent>
            <TabsContent value="connections" className="pb-8 pt-2">
                <KarinConnectionsTab {...tabProps} />
            </TabsContent>
            <TabsContent value="rules" className="pb-8 pt-2">
                <KarinRulesTab {...tabProps} />
            </TabsContent>
            <TabsContent value="render" className="pb-8 pt-2">
                <KarinRenderStorageTab {...tabProps} />
            </TabsContent>
            <PluginsPane instance={instance} />
        </>
    );
}

function PluginsPane({ instance }: { instance: FrameworkDetailProps['instance'] }) {
    return (
        <TabsContent value="plugins" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
            <KarinPluginsTab instance={instance} />
        </TabsContent>
    );
}

export const karinFrameworkUi: FrameworkUiModule = {
    extraTabs: EXTRA_TABS,
    defaultTab: 'basic',
    typedTabs: TYPED_TABS,
    fillPaneTabs: FILL_PANE,
    tabForIssue,
    Detail: KarinFrameworkDetail,
};
