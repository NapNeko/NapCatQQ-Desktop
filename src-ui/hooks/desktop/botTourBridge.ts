// 机器人页 tour：打开演示新建、切 Tab、演示模式（不落盘）。
// 结束必须 clearBotTourBridge。

type Listener = () => void;

export type BotTourConfigTab = 'identity' | 'connections' | 'advanced';

type BridgeState = {
    /** 为 true 时 BotPage 切到新建配置（botId=null） */
    openCreate: boolean;
    /** 演示模式：预填、拦截保存、页头提示 */
    demoMode: boolean;
    /** 强制配置页 Tab（tour 步进时） */
    forceTab: BotTourConfigTab | null;
    /** 请求回到列表（演示结束或跳过） */
    requestList: boolean;
};

let state: BridgeState = {
    openCreate: false,
    demoMode: false,
    forceTab: null,
    requestList: false,
};

const listeners = new Set<Listener>();

function emit() {
    for (const l of listeners) l();
}

export function getBotTourBridge(): BridgeState {
    return state;
}

export function setBotTourBridge(patch: Partial<BridgeState>): void {
    state = { ...state, ...patch };
    emit();
}

export function clearBotTourBridge(): void {
    state = {
        openCreate: false,
        demoMode: false,
        forceTab: null,
        requestList: false,
    };
    emit();
}

export function subscribeBotTourBridge(listener: Listener): () => void {
    listeners.add(listener);
    return () => {
        listeners.delete(listener);
    };
}

/** 演示用 QQ / 名称（不会写入配置） */
export const BOT_TOUR_DEMO = {
    qqId: 10001,
    name: '演示实例（不会保存）',
} as const;
