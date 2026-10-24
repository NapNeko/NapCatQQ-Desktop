import { useCallback } from 'react';
import { clipboardService } from '../../core/services/clipboard.service';

export function useClipboardText() {
    return useCallback((text: string) => clipboardService.writeText(text), []);
}
