// 任务队列上的取消、删除、清空已结束。列表本身跟着 deployment_task_changed 事件变，
// 这里只发命令，成败由按钮所在的地方提示。

import { useMutation } from '@tanstack/react-query';
import { deploymentTaskService } from '../../core/services/deployment-task.service';

export function useTaskQueueActions() {
    const cancel = useMutation({ mutationFn: deploymentTaskService.cancel });
    const remove = useMutation({ mutationFn: deploymentTaskService.delete });
    const clearFinished = useMutation({ mutationFn: deploymentTaskService.clearFinished });

    return {
        cancelTask: cancel.mutateAsync,
        deleteTask: remove.mutateAsync,
        clearFinished: () => clearFinished.mutateAsync(),
    };
}
