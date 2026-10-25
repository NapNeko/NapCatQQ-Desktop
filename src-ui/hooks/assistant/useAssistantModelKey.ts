// API Key 不进入 React Query 的 mutation variables 或任何共享草稿。

import { useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { assistantService } from '../../core/services/assistant.service';
import { errorText } from '../../core/domain/errors';
import { assistantModelStatusKey } from './useAssistant';

export function useAssistantModelKey() {
    const client = useQueryClient();
    const pending = useRef(false);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const save = async (apiKey: string): Promise<boolean> => {
        if (pending.current) return false;
        pending.current = true;
        setBusy(true);
        setError(null);
        try {
            await client.cancelQueries({ queryKey: assistantModelStatusKey });
            const status = await assistantService.setApiKey(apiKey);
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
    const clear = async (): Promise<boolean> => {
        if (pending.current) return false;
        pending.current = true;
        setBusy(true);
        setError(null);
        try {
            await client.cancelQueries({ queryKey: assistantModelStatusKey });
            const status = await assistantService.clearApiKey();
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
    return { save, clear, busy, error };
}
