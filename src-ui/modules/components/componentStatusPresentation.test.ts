import { describe, expect, it } from 'vitest';
import { shouldOfferManagedNodeInstall } from './componentStatusPresentation';

describe('Node.js component actions', () => {
    it('does not offer installation when a valid external Node.js is detected', () => {
        expect(
            shouldOfferManagedNodeInstall({
                state: 'installed',
                detected: { version: '24.18.0', source: '$PATH/node' },
            }),
        ).toBe(false);
    });

    it('offers installation when the managed Node.js component is detected', () => {
        expect(
            shouldOfferManagedNodeInstall({
                state: 'installed',
                detected: { version: '24.18.0', source: 'C:/ProgramData/NapCatQQ Desktop/NodeJs/node.exe' },
            }),
        ).toBe(true);
    });
});
