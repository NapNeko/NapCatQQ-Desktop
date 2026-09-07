// 应用端日志 Tab：和 Bot 日志页同一套 LogConsole。

import { LogConsole } from '../../../shared/log/LogConsole';
import { useAppInstanceLog } from '../../../hooks/apps/useAppInstances';
import type { AppInstance } from '../../../core/ipc/types';

export const InstanceLogTab: React.FC<{ instance: AppInstance }> = ({ instance }) => {
    const { logs, clear } = useAppInstanceLog(instance.id);
    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <LogConsole
                logs={logs}
                onClear={clear}
                emptyTitle={instance.state === 'running' ? '等待新日志…' : '暂无日志'}
                emptyBody={
                    instance.state === 'running'
                        ? '进程已启动，输出会出现在这里'
                        : '启动实例后日志会出现在这里'
                }
            />
        </div>
    );
};
