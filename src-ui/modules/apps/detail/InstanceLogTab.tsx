// 应用端日志 Tab：和 Bot 日志页同一套 LogConsole。

import { useEffect } from 'react';
import { LogConsole } from '../../../shared/log/LogConsole';
import { useAppInstanceLog } from '../../../hooks/apps/useAppInstances';
import { appFrameworkService } from '../../../core/services/app-framework.service';
import type { AppInstance } from '../../../core/ipc/types';

export const InstanceLogTab: React.FC<{ instance: AppInstance }> = ({ instance }) => {
    const { logs, clear } = useAppInstanceLog(instance.id);
    useEffect(() => {
        if (instance.state !== 'running') return;
        void appFrameworkService.refresh(instance.id).catch(() => {});
    }, [instance.id, instance.state]);
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
