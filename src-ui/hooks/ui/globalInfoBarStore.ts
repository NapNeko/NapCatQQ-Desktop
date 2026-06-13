// 全局 InfoBar 显示队列（模块级单例）。
//
// 形态参考 createStore：state + listeners + useSyncExternalStore 订阅，不依赖
// React Context / Provider。任何 hook / service 都可以直接 import push / dismiss
// 推条目。
//
// 跟 componentActionStore 是两件事，不要合并：
//   - componentActionStore 记"哪个 task 跑到多少 %"，是状态机
//   - 本 store 记"屏幕上要显示哪几条 banner"，是显示队列
//   - 旧 useComponentActionErrors 把状态机的终态扫描出来 push 进显示队列
//
// 行为对齐 Fluent InfoBarManager：
//   - 默认右上角堆叠（见 InfoBarStack），margin / spacing 在容器上配置
//   - 同 key 顶替时保持原队列下标，避免反复重试时整条 banner 上下跳动
//   - dismiss / clear 幂等；id 为 key:${key} 或 bar-${n}
//
// 渲染入口：AppNext.tsx 顶层挂一次 <InfoBarStack items={bars} onDismiss={...} />
// 全应用唯一渲染处。跨路由切换 banner 不丢，因为 state 在模块级、跟组件树解耦。

import { createStore } from '../utils/createStore';
import type { InfoBarStackItem } from '../../shared/ui';
import type { InfoBarTone } from '../../shared/ui/InfoBar';
import { resolveInfoBarAutoDismissMs } from '../../core/domain/ui/infoBarDismiss';
import { infoBarDismissPrefsStore } from '../preferences/infoBarDismissPrefsStore';

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
const store = createStore<State>(initialState);

let nextId = 1;

function genId(prefix: string): string {
    return `${prefix}-${nextId++}`;
}

export const globalInfoBarStore = {
    /** 当前快照（同步），useSyncExternalStore 用。 */
    getSnapshot: store.getSnapshot,

    subscribe: store.subscribe,

    /**
     * 推一条 banner，返回它的 id（外部可以用 id 主动 dismiss）。
     *
     * - 传了 `key`：同 key 旧条目被替换；位置维持原次序（不重新排到末尾，
     *   避免反复重试时 banner 抖动）。
     * - 没传 `key`：append 到队列末尾，新 id。
     */
    push(opts: PushInfoBarOptions): string {
        const { key, onUserDismiss, tone, autoDismissMs, ...rest } = opts;
        const resolvedDismiss = resolveInfoBarAutoDismissMs(
            (tone ?? 'info') as InfoBarTone,
            autoDismissMs,
            infoBarDismissPrefsStore.getSnapshot(),
        );
        const item: InfoBarStackItem = {
            ...rest,
            tone,
            autoDismissMs: resolvedDismiss,
            onUserDismiss,
            id: key ? `key:${key}` : genId('bar'),
        };
        const current = store.getSnapshot();

        if (key) {
            const idx = current.bars.findIndex((b) => b.id === item.id);
            if (idx >= 0) {
                const next = current.bars.slice();
                next[idx] = item;
                store.setState({ bars: next });
                return item.id;
            }
        }

        store.setState({ bars: [...current.bars, item] });
        return item.id;
    },

    /** 主动消除一条 banner。多次调用幂等。 */
    dismiss(id: string): void {
        const current = store.getSnapshot();
        const bar = current.bars.find((b) => b.id === id);
        const next = current.bars.filter((b) => b.id !== id);
        if (next.length === current.bars.length) return;
        store.setState({ bars: next });
        bar?.onUserDismiss?.();
    },

    /** 清空（极少用，主要给测试 / 极端 reset 场景）。 */
    clear(): void {
        const current = store.getSnapshot();
        if (current.bars.length === 0) return;
        store.setState({ bars: [] });
    },

    /** 测试 / dev 重置用，生产代码不要碰。 */
    _reset(): void {
        nextId = 1;
        store._reset();
    },
};

// 导出同名顶层方法供非 React 代码直接调用（service / 普通 .ts 文件）。
// React 组件 / hook 通常用 useGlobalInfoBars()，多一层 useCallback 稳定引用。
export const pushInfoBar = globalInfoBarStore.push;
export const dismissInfoBar = globalInfoBarStore.dismiss;
