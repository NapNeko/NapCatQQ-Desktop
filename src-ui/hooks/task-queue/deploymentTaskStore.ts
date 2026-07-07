import { createStore } from '../utils/createStore';
import type { DeploymentTaskList, DeploymentTaskSnapshot } from '../../core/ipc/types';
import { deploymentTaskService } from '../../core/services/deployment-task.service';

export interface DeploymentTaskStoreState {
    tasks: Record<string, DeploymentTaskSnapshot>;
    loaded: boolean;
}

const initialState: DeploymentTaskStoreState = {
    tasks: {},
    loaded: false,
};

const store = createStore<DeploymentTaskStoreState>(initialState);

function keyOf(task: DeploymentTaskSnapshot): string {
    return task.taskId;
}

export const deploymentTaskStore = {
    getSnapshot: store.getSnapshot,
    subscribe: store.subscribe,

    applyList(list: DeploymentTaskList): void {
        const next: Record<string, DeploymentTaskSnapshot> = {};
        for (const task of list.tasks) {
            next[keyOf(task)] = task;
        }
        store.setState({ tasks: next, loaded: true });
    },

    applyTask(task: DeploymentTaskSnapshot): void {
        const current = store.getSnapshot();
        store.setState({
            ...current,
            tasks: {
                ...current.tasks,
                [keyOf(task)]: task,
            },
        });
    },

    removeTask(taskId: string): void {
        const current = store.getSnapshot();
        if (!(taskId in current.tasks)) return;
        const tasks = { ...current.tasks };
        delete tasks[taskId];
        store.setState({ ...current, tasks });
    },

    async load(): Promise<void> {
        const list = await deploymentTaskService.list();
        this.applyList(list);
    },

    _reset(): void {
        store._reset();
    },
};
