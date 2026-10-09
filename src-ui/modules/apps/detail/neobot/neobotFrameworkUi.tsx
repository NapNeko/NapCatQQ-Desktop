import { useMemo } from 'react';
import { TabsContent } from '../../../../shared/ui';
import { NeoBotDraftContext } from '../../../../hooks/apps/neoBotDraftContext';
import { NeoBotConsoleTab } from './NeoBotConsoleTab';
import { NeoBotDeployTab } from './NeoBotDeployTab';
import { NeoBotMemoryTab } from './NeoBotMemoryTab';
import { NeoBotModelsTab } from './NeoBotModelsTab';
import { NeoBotOverviewTab } from './NeoBotOverviewTab';
import { NeoBotPluginsTab } from './NeoBotPluginsTab';
import { NeoBotPromptsTab } from './NeoBotPromptsTab';
import { NeoBotConfigTab } from './NeoBotConfigTab';
import { NeoBotEnvTab } from './NeoBotEnvTab';
import { NeoBotRuntimeTab } from './NeoBotRuntimeTab';
import { NeoBotScheduledTab } from './NeoBotScheduledTab';
import { NeoBotStatisticsTab } from './NeoBotStatisticsTab';
import { NeoBotBillingTab } from './NeoBotBillingTab';
import { NeoBotFlowsTab } from './NeoBotFlowsTab';
import { NeoBotAnalysisTab } from './NeoBotAnalysisTab';
import type { FrameworkDetailProps, FrameworkNavGroup, FrameworkUiModule } from '../frameworkUi';

// 部署作为新实例的默认入口，运行期数据与编辑操作由面板统一落盘。
const NAV: readonly FrameworkNavGroup[] = [
    {
        id: 'home',
        items: [
            { value: 'deploy', label: '部署' },
            { value: 'overview', label: '概览' },
            { value: 'runtime', label: '系统与运行' },
            { value: 'statistics', label: '统计与用量' },
        ],
    },
    {
        id: 'ai',
        label: 'AI',
        items: [
            { value: 'models', label: '模型' },
            { value: 'prompts', label: '提示词' },
            { value: 'memory', label: '记忆' },
            { value: 'flows', label: '对话流' },
            { value: 'analysis', label: '提示词分析' },
            { value: 'billing', label: '计费' },
        ],
    },
    {
        id: 'extend',
        label: '扩展',
        items: [
            { value: 'plugins', label: '插件' },
            { value: 'scheduled', label: '定时任务' },
        ],
    },
    {
        id: 'config',
        label: '配置',
        items: [
            { value: 'config', label: '本体配置' },
            { value: 'env', label: '供应商与密钥' },
            { value: 'console', label: '面板凭据' },
        ],
    },
];

// 页面由详情外壳滚动，fillPane 的 overflow-hidden 会裁掉长表单。
const FILL_PANE = new Set<string>();

const PANE = 'flex min-h-0 flex-1 flex-col pt-2';

function NeoBotFrameworkDetail({
    instance,
    onOpenWebUi,
    onGoTab,
    onNavigate,
}: FrameworkDetailProps) {
    const instanceId = instance.id;
    const drafts = useMemo(
        () => ({ instanceId, values: new Map<string, unknown>() }),
        [instanceId],
    );
    return (
        <NeoBotDraftContext.Provider value={drafts}>
            <TabsContent value="deploy" className={PANE}>
                <NeoBotDeployTab instance={instance} onGoTab={onGoTab} onNavigate={onNavigate} />
            </TabsContent>
            <TabsContent value="overview" className={PANE}>
                <NeoBotOverviewTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="models" className={PANE}>
                <NeoBotModelsTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="prompts" className={PANE}>
                <NeoBotPromptsTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="memory" className={PANE}>
                <NeoBotMemoryTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="plugins" className={PANE}>
                <NeoBotPluginsTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="console" className={PANE}>
                <NeoBotConsoleTab instance={instance} onOpenWebUi={onOpenWebUi} />
            </TabsContent>
            <TabsContent value="config" className={PANE}>
                <NeoBotConfigTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="env" className={PANE}>
                <NeoBotEnvTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="runtime" className={PANE}>
                <NeoBotRuntimeTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="scheduled" className={PANE}>
                <NeoBotScheduledTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="statistics" className={PANE}>
                <NeoBotStatisticsTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="billing" className={PANE}>
                <NeoBotBillingTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="flows" className={PANE}>
                <NeoBotFlowsTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="analysis" className={PANE}>
                <NeoBotAnalysisTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
        </NeoBotDraftContext.Provider>
    );
}

export const neobotFrameworkUi: FrameworkUiModule = {
    nav: NAV,
    defaultTab: 'deploy',
    // 编辑页各自保存并带上面板 revision，不挂实例冷配置的保存条。
    typedTabs: new Set(),
    fillPaneTabs: FILL_PANE,
    tabForIssue: () => 'deploy',
    Detail: NeoBotFrameworkDetail,
};
