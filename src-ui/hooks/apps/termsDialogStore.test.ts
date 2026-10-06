import { afterEach, describe, expect, it } from 'vitest';
import { _resetTermsDialog, peekTermsDialog, requestTermsConsent } from './termsDialogStore';
import type { AppPendingTerms } from '../../core/ipc/types';

const eula: AppPendingTerms = {
    id: 'eula',
    title: 'EULA',
    url: 'https://example.com',
    text: '# EULA',
};

afterEach(() => _resetTermsDialog());

describe('requestTermsConsent', () => {
    it('resolves with the answer the dialog host gives, then closes', async () => {
        const answer = requestTermsConsent('麦麦', [eula]);
        const state = peekTermsDialog();
        expect(state?.instanceName).toBe('麦麦');
        expect(state?.terms).toEqual([eula]);
        state?.settle(true);
        await expect(answer).resolves.toBe(true);
        expect(peekTermsDialog()).toBeNull();
    });

    it('treats an unanswered earlier dialog as cancelled', async () => {
        const first = requestTermsConsent('a', [eula]);
        const second = requestTermsConsent('b', [eula]);
        await expect(first).resolves.toBe(false);
        expect(peekTermsDialog()?.instanceName).toBe('b');
        peekTermsDialog()?.settle(false);
        await expect(second).resolves.toBe(false);
    });
});
