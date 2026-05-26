// 全局 InfoBar 显示队列（模块级单例）。
//
// 形态参考 componentActionStore：模块级 state + listeners + useSyncExternalStore
// 订阅，不依赖 React Context / Provider。任何 hook / service 都可以直接 import
// push / dismiss 推条目。
//
// 跟 componentActionStore 是两件事，不要合并：
//   - componentActionStore 记"哪个 task 跑到多少 %"，是状态机
//   - 本 store 记"屏幕上要显示哪几条 banner"，是显示队列
//   - 旧 useComponentActionErrors 把状态机的终态扫描出来 push 进显示队列
//
// 渲染入口：AppNext.tsx 顶层挂一次 <InfoBarStack items={bars} onDismiss={...} />
// 全应用唯一渲染处。跨路由切换 banner 不丢，因为 state 在模块级、跟组件树解耦。

import type { InfoBarStackItem } from '../../shared/ui';

/// push 接口：与 InfoBarStackItem 一致，但 id 由 store 自己生成或走 key 顶替；
/// 同时加 key 字段做"同一来源去重"——同 key 再 push 时，旧条目被新条目替换，
/// 不会堆叠 N 条。典型用例：SSH 反复重连失败、文件刷新连续报错。
///
/// 没传 key 走纯 append（典型用例：每次 component-action 失败都要单独显示）。
export interface PushInfoBarOptions extends Omit<InfoBarStackItem, 'id'> {
    /** 同 key 顶替；不传则走 append 累计。 */
    key?: string;
}

interface State {
    bars: InfoBarStackItem[];
}

const initialState: State = { bars: [] };
let state: State = initialState;
const listeners = new Set<() => void>();
let nextId = 1;

function emit(): void {
    for (const fn of listeners) fn();
}

function genId(prefix: string): string {
    return `${prefix}-${nextId++}`;
}

export const globalInfoBarStore = {
    /** 当前快照（同步），useSyncExternalStore 用。 */
    getSnapshot(): State {
        return state;
    },

    subscribe(listener: () => void): () => void {
        listeners.add(listener);
        return () => listeners.delete(listener);
    },

    /**
     * 推一条 banner，返回它的 id（外部可以用 id 主动 dismiss）。
     *
     * - 传了 `key`：同 key 旧条目被替换；位置维持原次序（不重新排到末尾，
     *   避免反复重试时 banner 抖动）。
     * - 没传 `key`：append 到队列末尾，新 id。
     */
    push(opts: PushInfoBarOptions): string {
        const { key, ...rest } = opts;
        const item: InfoBarStackItem = {
            ...rest,
            id: key ? `key:${key}` : genId('bar'),
        };

        if (key) {
            const idx = state.bars.findIndex((b) => b.id === item.id);
            if (idx >= 0) {
                const next = state.bars.slice();
                next[idx] = item;
                state = { bars: next };
                emit();
                return item.id;
            }
        }

        state = { bars: [...state.bars, item] };
        emit();
        return item.id;
    },

    /** 主动消除一条 banner。多次调用幂等。 */
    dismiss(id: string): void {
        const next = state.bars.filter((b) => b.id !== id);
        if (next.length === state.bars.length) return;
        state = { bars: next };
        emit();
    },

    /** 清空（极少用，主要给测试 / 极端 reset 场景）。 */
    clear(): void {
        if (state.bars.length === 0) return;
        state = { bars: [] };
        emit();
    },

    /** 测试 / dev 重置用，生产代码不要碰。 */
    _reset(): void {
        state = initialState;
        nextId = 1;
        emit();
    },
};

// 导出同名顶层方法供非 React 代码直接调用（service / 普通 .ts 文件）。
// React 组件 / hook 通常用 useGlobalInfoBars()，多一层 useCallback 稳定引用。
export const pushInfoBar = globalInfoBarStore.push;
export const dismissInfoBar = globalInfoBarStore.dismiss;
