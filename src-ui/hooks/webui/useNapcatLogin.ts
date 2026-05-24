// NapCat 登录态聚合 hook。
// 把 5 类后端事件喂给 reducer，并按 invalidationEpoch 触发 3s 自动隐藏定时器。

import { useEffect, useReducer, useRef } from 'react';
import { useDomainEvents } from '../events/useDomainEvents';
import {
    clearInvalidation,
    initialNapcatLoginState,
    reduceNapcatLogin,
    type NapcatLoginState,
} from '../../core/domain/events/login-aggregator';
import type { DomainEvent } from '../../core/ipc/types';

type Action =
    | { type: 'event'; event: DomainEvent }
    | { type: 'clear-invalidation'; botId: string };

function reducer(state: NapcatLoginState, action: Action): NapcatLoginState {
    switch (action.type) {
        case 'event':
            return reduceNapcatLogin(state, action.event);
        case 'clear-invalidation':
            return clearInvalidation(state, action.botId);
        default:
            return state;
    }
}

export function useNapcatLogin() {
    const [state, dispatch] = useReducer(reducer, initialNapcatLoginState);

    // 维护每个 bot 的失效自动消失定时器。当 epoch 变化时重启。
    const timersRef = useRef<Record<string, ReturnType<typeof setTimeout>>>({});
    const lastEpochRef = useRef<Record<string, number>>({});

    useDomainEvents((event) => {
        dispatch({ type: 'event', event });
    });

    useEffect(() => {
        for (const [botId, epoch] of Object.entries(state.invalidationEpoch)) {
            if (lastEpochRef.current[botId] === epoch) continue;
            lastEpochRef.current[botId] = epoch;

            const prevTimer = timersRef.current[botId];
            if (prevTimer) clearTimeout(prevTimer);

            timersRef.current[botId] = setTimeout(() => {
                dispatch({ type: 'clear-invalidation', botId });
                delete timersRef.current[botId];
            }, 3000);
        }
    }, [state.invalidationEpoch]);

    useEffect(() => {
        return () => {
            for (const t of Object.values(timersRef.current)) clearTimeout(t);
            timersRef.current = {};
        };
    }, []);

    return state;
}
