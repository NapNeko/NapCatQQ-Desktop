// SnowLuma daemon + per-bot 聚合 hook。

import { useReducer } from 'react';
import { useDomainEvents } from '../events/useDomainEvents';
import {
    initialSnowlumaState,
    reduceSnowluma,
    type SnowlumaState,
} from '../../core/domain/events/snowluma-aggregator';
import type { DomainEvent } from '../../core/ipc/types';

function reducer(state: SnowlumaState, event: DomainEvent): SnowlumaState {
    return reduceSnowluma(state, event);
}

export function useSnowlumaState() {
    const [state, dispatch] = useReducer(reducer, initialSnowlumaState);

    useDomainEvents((event) => {
        dispatch(event);
    });

    return state;
}
