// 框架 tour 驱动组件页选中主机 / 是否注入演示远端。
// hostSelectionLocked=true 时才强制 preferredHostId；结束必须 locked=false 且 preferred=null。

type Listener = () => void;

type BridgeState = {
    preferredHostId: string | null;
    includeDemoRemote: boolean;
    /** tour 进行中为 true；结束后 false，否则会钉死主机 tab */
    hostSelectionLocked: boolean;
};

let state: BridgeState = {
    preferredHostId: null,
    includeDemoRemote: false,
    hostSelectionLocked: false,
};

const listeners = new Set<Listener>();

function emit() {
    for (const l of listeners) l();
}

export function getComponentsHostBridge(): BridgeState {
    return state;
}

export function setComponentsHostBridge(patch: Partial<BridgeState>): void {
    state = { ...state, ...patch };
    emit();
}

export function clearComponentsHostBridge(): void {
    state = {
        preferredHostId: null,
        includeDemoRemote: false,
        hostSelectionLocked: false,
    };
    emit();
}

export function subscribeComponentsHostBridge(listener: Listener): () => void {
    listeners.add(listener);
    return () => {
        listeners.delete(listener);
    };
}
