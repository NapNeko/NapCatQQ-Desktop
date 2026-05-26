// 模块级 store 通用工厂。
//
// 4 个现有 store（globalInfoBar / componentAction / napcatLogin / snowluma）
// 都自己写一遍 state + listeners + emit + getSnapshot + subscribe + _reset 的
// boilerplate，~10 行重复死代码。提到这里集中实现，外层只关心"这个 store 有
// 哪些专属 mutator"。
//
// 设计点：
//   - 不引入 Redux / Zustand 等第三方。React 18 的 useSyncExternalStore 已经够
//     用，加一层简陋包装就好。
//   - setState 走 reference equality 短路：next === state 时不 emit，避免同
//     reducer 跑出同对象时的多余渲染。reducer 自己做不可变更新，这里不深比较。
//   - reset 给测试 / dev 用，生产代码不要碰。

export interface ModuleStore<S> {
    /** useSyncExternalStore 第二个参数。同步返回当前状态。 */
    getSnapshot(): S;
    /** useSyncExternalStore 第一个参数。返回 unsubscribe。 */
    subscribe(listener: () => void): () => void;
    /** 写入新状态并通知订阅者。next === 当前快照时静默跳过。 */
    setState(next: S): void;
    /** 重置回 initialState，并清空所有 listeners 通知（listeners 集合保留）。 */
    _reset(): void;
}

export function createStore<S>(initialState: S): ModuleStore<S> {
    let state: S = initialState;
    const listeners = new Set<() => void>();

    function emit(): void {
        for (const fn of listeners) fn();
    }

    return {
        getSnapshot(): S {
            return state;
        },

        subscribe(listener: () => void): () => void {
            listeners.add(listener);
            return () => {
                listeners.delete(listener);
            };
        },

        setState(next: S): void {
            if (next === state) return;
            state = next;
            emit();
        },

        _reset(): void {
            state = initialState;
            emit();
        },
    };
}
