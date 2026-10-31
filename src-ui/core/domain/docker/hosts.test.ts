import { describe, expect, it } from 'vitest';
import { dockerProbeHosts } from './hosts';

describe('container management host isolation', () => {
    it('includes the local Linux engine without requiring an SSH server', () => {
        expect(dockerProbeHosts('linux', [])).toEqual(['local']);
        expect(dockerProbeHosts('linux', ['one'])).toEqual(['local', 'remote:one']);
    });

    it.each(['windows', 'mac_os'] as const)('keeps %s local engines excluded', (os) => {
        expect(dockerProbeHosts(os, [])).toEqual([]);
        expect(dockerProbeHosts(os, ['one', 'two'])).toEqual(['remote:one', 'remote:two']);
    });
});
