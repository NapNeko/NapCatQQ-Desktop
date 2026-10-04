import { TabsContent } from '../../../../shared/ui';
import { NeoBotConsoleTab } from './NeoBotConsoleTab';
import { NeoBotDeployTab } from './NeoBotDeployTab';
import { NeoBotMemoryTab } from './NeoBotMemoryTab';
import { NeoBotModelsTab } from './NeoBotModelsTab';
import { NeoBotOverviewTab } from './NeoBotOverviewTab';
import { NeoBotPluginsTab } from './NeoBotPluginsTab';
import { NeoBotPromptsTab } from './NeoBotPromptsTab';
import type { FrameworkDetailProps, FrameworkNavGroup, FrameworkUiModule } from '../frameworkUi';

// 「部署」放第一位并作默认页：新实例第一件该做的事是把它跑起来、连上 QQ，而不是看统计。
// 其余页都是只读的面板数据（一个页签一个端点，状态处理见 PanelStateView）。
// 原始文件、日志由外壳追加。
const NAV: readonly FrameworkNavGroup[] = [
    { id: 'home', items: [{ value: 'deploy', label: '部署' }, { value: 'overview', label: '概览' }] },
    {
        id: 'ai',
        label: 'AI',
        items: [
            { value: 'models', label: '模型' },
            { value: 'prompts', label: '提示词' },
            { value: 'memory', label: '记忆' },
        ],
    },
    { id: 'extend', label: '扩展', items: [{ value: 'plugins', label: '插件' }] },
    { id: 'console', items: [{ value: 'console', label: 'Web 控制台' }] },
];

const FILL_PANE = new Set([
    'deploy',
    'overview',
    'models',
    'prompts',
    'memory',
    'plugins',
    'console',
]);

function NeoBotFrameworkDetail({ instance, onOpenWebUi, onGoTab }: FrameworkDetailProps) {
    const instanceId = instance.id;
    return (
        <>
            <TabsContent value="deploy" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                <NeoBotDeployTab instance={instance} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="overview" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                <NeoBotOverviewTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="models" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                <NeoBotModelsTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="prompts" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                <NeoBotPromptsTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="memory" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                <NeoBotMemoryTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="plugins" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                <NeoBotPluginsTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="console" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                <NeoBotConsoleTab instance={instance} onOpenWebUi={onOpenWebUi} />
            </TabsContent>
        </>
    );
}

export const neobotFrameworkUi: FrameworkUiModule = {
    nav: NAV,
    defaultTab: 'deploy',
    // 都不挂保存条：各页只读，或自己把动作 POST 给面板
    typedTabs: new Set(),
    fillPaneTabs: FILL_PANE,
    tabForIssue: () => 'deploy',
    Detail: NeoBotFrameworkDetail,
};
