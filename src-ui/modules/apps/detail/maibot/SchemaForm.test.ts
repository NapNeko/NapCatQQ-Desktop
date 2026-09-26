import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { UiNode } from '../../../../core/domain/apps/maibotSchema';
import { advancedHasError, hasAdvanced, type SchemaCtx } from './SchemaForm';
import { useAdvancedSections, useRevealOnError } from './advancedToggle';

const SEC: UiNode = {
    fields: [
        { name: 'name', type: 'string' },
        { name: 'retry', type: 'integer', advanced: true },
        { name: 'port', type: 'integer', advanced: true },
        { name: 'rules', type: 'array' },
        { name: 'extra', type: 'object' },
    ],
    nested: {
        rules: {
            fields: [
                { name: 'value', type: 'number' },
                { name: 'weight', type: 'number', advanced: true },
            ],
        },
        extra: { uiAdvanced: true, fields: [{ name: 'flag', type: 'boolean' }] },
    },
};

const ctx = (opts: { errors?: Record<string, string>; skip?: string[]; advanced?: string[] } = {}): SchemaCtx => ({
    value: {},
    onChange: () => {},
    errors: opts.errors ?? {},
    prefix: 'bot',
    showAdvanced: false,
    skip: new Set(opts.skip),
    advanced: new Set(opts.advanced),
});

describe('hasAdvanced', () => {
    it('sees advanced fields, advanced list items and advanced sub groups', () => {
        expect(hasAdvanced(ctx(), ['sec'], SEC)).toBe(true);
        const itemOnly: UiNode = { fields: [{ name: 'rules', type: 'array' }], nested: { rules: SEC.nested!.rules } };
        expect(hasAdvanced(ctx(), ['sec'], itemOnly)).toBe(true);
    });

    it('ignores advanced fields this page skips, and counts ones the page marks advanced', () => {
        const node: UiNode = {
            fields: [
                { name: 'name', type: 'string' },
                { name: 'port', type: 'integer', advanced: true },
            ],
        };
        expect(hasAdvanced(ctx({ skip: ['sec.port'] }), ['sec'], node)).toBe(false);
        expect(hasAdvanced(ctx({ skip: ['sec.port'], advanced: ['sec.name'] }), ['sec'], node)).toBe(true);
    });
});

describe('advancedHasError', () => {
    const errored = (key: string, opts: { skip?: string[]; advanced?: string[] } = {}) =>
        advancedHasError(ctx({ ...opts, errors: { [key]: 'x' } }), ['sec'], SEC);

    it('only errors on fields that collapse count', () => {
        expect(errored('bot/sec/name')).toBe(false);
        expect(errored('bot/sec/retry')).toBe(true);
        expect(errored('bot/sec/name', { advanced: ['sec.name'] })).toBe(true);
    });

    it('walks list items and sub groups', () => {
        expect(errored('bot/sec/rules/0/weight')).toBe(true);
        expect(errored('bot/sec/rules/1/value')).toBe(false);
        expect(errored('bot/sec/extra/flag')).toBe(true);
    });

    it('leaves skipped fields and other sections alone', () => {
        expect(errored('bot/sec/port', { skip: ['sec.port'] })).toBe(false);
        expect(errored('bot/other/retry')).toBe(false);
        expect(errored('bot/section/retry')).toBe(false);
    });
});

describe('useAdvancedSections', () => {
    it('toggles sections independently', () => {
        const { result } = renderHook(() => useAdvancedSections());
        act(() => result.current.toggle('talk/chat'));
        expect(result.current.isOpen('talk/chat')).toBe(true);
        expect(result.current.isOpen('talk/emoji')).toBe(false);
        act(() => result.current.toggle('talk/chat'));
        expect(result.current.isOpen('talk/chat')).toBe(false);
    });

    it('opens errored sections once and lets the user close them again', () => {
        const { result, rerender } = renderHook(
            ({ keys }: { keys: string[] }) => {
                const sections = useAdvancedSections();
                useRevealOnError(sections, keys);
                return sections;
            },
            { initialProps: { keys: ['advanced/log'] } },
        );
        expect(result.current.isOpen('advanced/log')).toBe(true);
        act(() => result.current.toggle('advanced/log'));
        rerender({ keys: ['advanced/log'] });
        expect(result.current.isOpen('advanced/log')).toBe(false);
        rerender({ keys: ['advanced/log', 'advanced/database'] });
        expect(result.current.isOpen('advanced/database')).toBe(true);
    });
});
