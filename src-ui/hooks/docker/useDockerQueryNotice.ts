import { useEffect } from 'react';
import { errorText } from '../../core/domain/errors';
import { dismissInfoBar } from '../ui/globalInfoBarStore';
import { pushErrorBar } from '../ui/pushErrorBar';

export function useDockerQueryNotice(key: string, title: string, error: unknown) {
    const message = error == null ? null : errorText(error);
    useEffect(() => {
        if (message) pushErrorBar({ key, title, raw: message });
        else dismissInfoBar(`key:${key}`);
        return () => dismissInfoBar(`key:${key}`);
    }, [key, title, message]);
}
