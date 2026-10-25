// 多请求页面只呈现一次公共故障，局部失败保留在对应小节。

/** 一个面板请求的收窄结果。unauthorized 与 notFound 的区别决定用户该去填密码还是去升级。 */
export type PanelState<T> =
    | { kind: 'ok'; data: T }
    /** 该框架不提供面板转发（panelCall 返回 null） */
    | { kind: 'unsupported' }
    /** 没填面板密码，或密码不对 */
    | { kind: 'unauthorized'; message: string }
    /** 面板打不通：实例没运行，或端口 / 会话不对 */
    | { kind: 'unreachable'; message: string }
    /** 面板没有这个接口（404）：实例的 NeoBot 版本低于该接口要求的版本 */
    | { kind: 'notFound'; message: string }
    | { kind: 'failed'; message: string }
    /** 面板答了但形状不认识（多半是面板改版） */
    | { kind: 'malformed' };

export type PanelBlockedState = Exclude<PanelState<unknown>, { kind: 'ok' }>;

export function panelQueryState<T>(query: {
    data?: PanelState<T>;
    isError: boolean;
    error: Error | null;
}): PanelState<T> | undefined {
    // 后台刷新失败不覆盖已读取的内容；首次请求异常仍需给出可重试状态。
    if (query.data?.kind === 'ok') return query.data;
    if (query.isError) return { kind: 'failed', message: query.error?.message || '读取面板失败' };
    return query.data;
}

/** 整页一条话的故障类型：前三种是「整页都读不了」，unavailable 是兜底。 */
export type PanelPageGateKind =
    'loading' | 'unsupported' | 'unauthorized' | 'unreachable' | 'unavailable';

export interface PanelPageGate {
    kind: PanelPageGateKind;
    /** 面板/请求自己给的话术，unavailable 时拼进正文 */
    message: string;
    /** 已经返回结果的请求数 / 本页请求数，用来判断「是真故障还是还在读」 */
    blocked: number;
    total: number;
}

/** 整页级故障的取舍顺序：框架不支持 > 没凭据 > 打不通。 */
const PAGE_KIND_ORDER = ['unsupported', 'unauthorized', 'unreachable'] as const;

function messageOf(state: PanelState<unknown>): string {
    return 'message' in state ? state.message : '';
}

/**
 * 把一页的多个请求状态压成一条整页提示；只要有一条成功就返回 null。
 *
 * 调用方传进来的 `undefined` 表示请求还没返回。全部未返回时给 loading，让页面画一个
 * 安静的读取态，而不是先闪六块「先填面板密码」再跳成正文。
 */
export function panelPageGate(
    states: ReadonlyArray<PanelState<unknown> | undefined>,
): PanelPageGate | null {
    const total = states.length;
    if (total === 0) return null;

    const known = states.filter((state): state is PanelState<unknown> => state !== undefined);
    if (known.some((state) => state.kind === 'ok')) return null;
    if (known.length === 0) return { kind: 'loading', message: '', blocked: 0, total };

    for (const kind of PAGE_KIND_ORDER) {
        const hit = known.find((state) => state.kind === kind);
        if (hit) return { kind, message: messageOf(hit), blocked: known.length, total };
    }
    return {
        kind: 'unavailable',
        message: known.map(messageOf).find((text) => text !== '') ?? '',
        blocked: known.length,
        total,
    };
}
