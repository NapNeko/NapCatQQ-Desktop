// API Key 不进入 React Query 的 mutation variables 或任何共享草稿。

import { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { assistantService } from '../../core/services/assistant.service';
import { errorText } from '../../core/domain/errors';
import { assistantModelStatusKey } from './useAssistant';
import { modelOrigin } from '../../core/domain/assistant/model';

export function useAssistantModelKeyDraft(baseUrl: string) {
    const origin = modelOrigin(baseUrl);
    const [draft, setDraft] = useState<{ value: string; origin: string | null }>({
        value: '',
        origin: null,
    });
    const [notice, setNotice] = useState<string | null>(null);
    const originChanged = !!draft.value && draft.origin !== origin;
    useEffect(() => {
        if (!originChanged) return;
        setDraft({ value: '', origin: null });
        setNotice('服务地址已换源，未保存的 API Key 已清空。');
    }, [originChanged]);
    return {
        value: originChanged ? '' : draft.value,
        origin,
        notice,
        change: (value: string, expectedOrigin = origin) => {
            setDraft({ value, origin: value ? expectedOrigin : null });
            setNotice(null);
        },
        clear: () => {
            setDraft({ value: '', origin: null });
            setNotice(null);
        },
    };
}

export function useAssistantModelKey() {
    const client = useQueryClient();
    const pending = useRef(false);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const save = async (apiKey: string, expectedOrigin: string): Promise<boolean> => {
        if (!expectedOrigin) {
            setError('请先填写有效的服务地址。');
            return false;
        }
        if (pending.current) return false;
        pending.current = true;
        setBusy(true);
        setError(null);
        try {
            await client.cancelQueries({ queryKey: assistantModelStatusKey });
            const status = await assistantService.setApiKey(apiKey, expectedOrigin);
            client.setQueryData(assistantModelStatusKey, status);
            return true;
        } catch (cause) {
            const text = errorText(cause);
            setError(apiKey ? text.split(apiKey).join('[REDACTED]') : text);
            return false;
        } finally {
            pending.current = false;
            setBusy(false);
        }
    };
    const clear = async (expectedOrigin: string): Promise<boolean> => {
        if (!expectedOrigin) {
            setError('请先保存有效的服务地址。');
            return false;
        }
        if (pending.current) return false;
        pending.current = true;
        setBusy(true);
        setError(null);
        try {
            await client.cancelQueries({ queryKey: assistantModelStatusKey });
            const status = await assistantService.clearApiKey(expectedOrigin);
            client.setQueryData(assistantModelStatusKey, status);
            return true;
        } catch (cause) {
            setError(errorText(cause));
            return false;
        } finally {
            pending.current = false;
            setBusy(false);
        }
    };
    return { save, clear, busy, error, reset: () => setError(null) };
}
