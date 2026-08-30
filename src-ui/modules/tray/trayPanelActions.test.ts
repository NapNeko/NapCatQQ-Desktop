import { describe, expect, it } from 'vitest';
import { trayActionErrorMessage } from './trayPanelActions';

describe('tray panel action errors', () => {
    it('keeps Error messages visible to the user', () => {
        expect(trayActionErrorMessage(new Error('配置漂移'))).toBe('配置漂移');
    });

    it('stringifies non-Error failures', () => {
        expect(trayActionErrorMessage('远端不可达')).toBe('远端不可达');
    });
});
