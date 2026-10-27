// 设置与 API Key 一次提交：Key 按已保存地址的 origin 存放，所以必须先落盘设置再交 Key。

import { useRef, useState } from 'react';
import type { AssistantSettings } from '../../core/domain/assistant/types';
import { errorText } from '../../core/domain/errors';
import { useAssistantSettings } from './useAssistant';
import { useAssistantModelKey } from './useAssistantModelKey';

export type ModelSubmitResult = 'saved' | 'keySaved' | 'settingsFailed' | 'keyFailed';

export function useAssistantModelSubmit() {
    const { save } = useAssistantSettings();
    const keys = useAssistantModelKey();
    const pending = useRef(false);
    const [saving, setSaving] = useState(false);
    const submit = async (
        settings: AssistantSettings | null,
        apiKey: string,
        expectedOrigin: string,
    ): Promise<ModelSubmitResult | null> => {
        if (pending.current) return null;
        pending.current = true;
        setSaving(true);
        save.reset();
        keys.reset();
        try {
            if (settings) {
                try {
                    // useMutation 级 onSuccess 会等设置查询刷新完，Key 才看得到新地址。
                    await save.mutateAsync(settings);
                } catch {
                    return 'settingsFailed';
                }
            }
            if (!apiKey) return 'saved';
            return (await keys.save(apiKey, expectedOrigin)) ? 'keySaved' : 'keyFailed';
        } finally {
            pending.current = false;
            setSaving(false);
        }
    };
    return {
        submit,
        clearKey: (expectedOrigin: string) => {
            save.reset();
            return keys.clear(expectedOrigin);
        },
        busy: saving || keys.busy,
        error: save.error ? errorText(save.error) : keys.error,
        reset: () => {
            save.reset();
            keys.reset();
        },
    };
}
