import { TabsContent } from '../../../../shared/ui';
import { NeoBotConsoleTab } from './NeoBotConsoleTab';
import { NeoBotMemoryTab } from './NeoBotMemoryTab';
import { NeoBotModelsTab } from './NeoBotModelsTab';
import { NeoBotOverviewTab } from './NeoBotOverviewTab';
import { NeoBotPluginsTab } from './NeoBotPluginsTab';
import { NeoBotPromptsTab } from './NeoBotPromptsTab';
import type { FrameworkDetailProps, FrameworkNavGroup, FrameworkUiModule } from '../frameworkUi';

// 除「Web 控制台」外都是**只读**面板数据页：一个页签一个端点，各页自己取数、
// 自己处理「没填密码 / 面板没起来」这些状态（见 PanelStateView）。
// 「Web 控制台」是例外：它放面板凭据与能力地图，并负责把用户送到面板本体。
// 原始文件、日志由外壳追加。
const NAV: readonly FrameworkNavGroup[] = [
    { id: 'home', items: [{ value: 'overview', label: '概览' }] },
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

const FILL_PANE = new Set(['overview', 'models', 'prompts', 'memory', 'plugins', 'console']);

function NeoBotFrameworkDetail({ instance, onOpenWebUi, onGoTab }: FrameworkDetailProps) {
    const instanceId = instance.id;
    return (
        <>
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
    defaultTab: 'overview',
    // 都不挂保存条：各页只读，控制台只有凭据一个输入（自己存）
    typedTabs: new Set(),
    fillPaneTabs: FILL_PANE,
    tabForIssue: () => 'overview',
    Detail: NeoBotFrameworkDetail,
};
