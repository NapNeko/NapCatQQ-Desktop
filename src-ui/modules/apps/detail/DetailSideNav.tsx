import { TabsSideList, TabsSideTrigger } from '../../../shared/ui';

export const DetailSideNav: React.FC<{ withVersionTab: boolean }> = ({ withVersionTab }) => (
    <div className="w-[8.5rem] shrink-0 border-r border-border-subtle py-3 pr-2">
        <TabsSideList aria-label="实例管理">
            <TabsSideTrigger value="config">配置</TabsSideTrigger>
            <TabsSideTrigger value="log">日志</TabsSideTrigger>
            {withVersionTab && <TabsSideTrigger value="version">版本</TabsSideTrigger>}
        </TabsSideList>
    </div>
);
