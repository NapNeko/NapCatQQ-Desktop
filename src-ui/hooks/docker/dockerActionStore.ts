// 模块级 docker-action 状态表。
//
// 为什么要提到模块级而不是放在 useDockerHosts 的 useMutation.isPending 里？
//   - Docker 安装走的是同步 command（docker_install 一次性返回 DockerInstallReport），
//     不像普通组件那样有 task_id + 进度事件流。前端"正在装"的状态原本绑在
//     useMutation.isPending 上，而 mutation 实例随 ComponentsPageNext 一起生灭。
//   - 用户在 Docker 安装进行中切走页面、再切回来,ComponentsPageNext 卸载重挂,
//     mutation 被重置成 isPending=false,进度 spinner 消失、"安装"按钮重现,看起来
//     像没在装(后端其实还在跑)。这与普通组件早就用 componentActionStore 解决的
//     是同一类问题,Docker 这条之前漏了对齐。
//   - 提到模块级后,状态生命周期对齐"应用窗口"而非"页面挂载",切路由不再丢。
//     按 hostId 记录,顺带修了原来 isInstalling 是全局布尔、多主机分不清谁在装。
//
// 注意:docker_install 是同步 promise,没有中间进度事件,所以这里只存"装没装中"
// 布尔,不存细粒度进度(docker 安装本身也没有 step 进度点)。install 的成败反馈
// 仍由调用方按返回的 DockerInstallReport 走 globalInfoBar / SudoPasswordDialog。

import { createStore } from '../utils/createStore';

export interface DockerActionStoreState {
    /** host_id → 是否正在该主机上安装 docker。 */
    installingByHost: Record<string, boolean>;
}

const initialState: DockerActionStoreState = {
    installingByHost: {},
};

const store = createStore<DockerActionStoreState>(initialState);

export const dockerActionStore = {
    /** 当前快照（同步）。useSyncExternalStore 用。 */
    getSnapshot: store.getSnapshot,

    subscribe: store.subscribe,

    /** 标记某主机开始安装 docker。 */
    markInstalling(hostId: string): void {
        const current = store.getSnapshot();
        if (current.installingByHost[hostId]) return;
        store.setState({
            installingByHost: { ...current.installingByHost, [hostId]: true },
        });
    },

    /** 清除某主机的安装中标记（install promise 落定时调,无论成败）。 */
    clearInstalling(hostId: string): void {
        const current = store.getSnapshot();
        if (!current.installingByHost[hostId]) return;
        const next = { ...current.installingByHost };
        delete next[hostId];
        store.setState({ installingByHost: next });
    },

    /** 测试 / dev 重置用。生产代码不要碰。 */
    _reset(): void {
        store._reset();
    },
};
