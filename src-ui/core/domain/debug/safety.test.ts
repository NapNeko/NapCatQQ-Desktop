import { describe, expect, it } from 'vitest';
import type { DebugActionSummary } from '../../ipc/generated/debug/DebugActionSummary';
import { SAFETY_DOT_CLASS, SAFETY_LABEL, SAFETY_TONE, onlyBackendLabel } from './safety';

const summary = (other: boolean | null): DebugActionSummary => ({
    name: 'nc_get_rkey',
    aliases: [],
    summary: '',
    category: 'extension',
    safety: 'read_only',
    stream: false,
    supported: true,
    other_backend_present: other,
    param_diff: false,
});

describe('safety', () => {
    it('色点和徽章是同一套颜色', () => {
        for (const s of ['read_only', 'side_effect', 'dangerous'] as const) {
            expect(SAFETY_DOT_CLASS[s]).toBe(`bg-${SAFETY_TONE[s]}`);
            expect(SAFETY_LABEL[s]).not.toBe('');
        }
    });

    it('只有确定另一个后端没有时才标「仅 NC / 仅 SL」', () => {
        expect(onlyBackendLabel(summary(false), 'napcat')).toBe('仅 NC');
        expect(onlyBackendLabel(summary(false), 'snowluma')).toBe('仅 SL');
        expect(onlyBackendLabel(summary(true), 'napcat')).toBeNull();
        expect(onlyBackendLabel(summary(null), 'napcat')).toBeNull();
    });
});
