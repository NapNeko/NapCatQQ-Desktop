import assert from 'node:assert/strict';
import { test } from 'node:test';
import { checkGate } from './ci-gate.mjs';

function needs(areas = {}, results = {}) {
    return {
        changes: {
            result: 'success',
            outputs: { frontend: 'false', rust_core: 'false', rust_shell: 'false', ...areas },
        },
        frontend: { result: 'skipped' },
        'rust-core': { result: 'skipped' },
        'rust-shell': { result: 'skipped' },
        ...Object.fromEntries(Object.entries(results).map(([job, result]) => [job, { result }])),
    };
}

for (const event of ['pull_request', 'push']) {
    test(`${event}: documentation changes can skip all suites`, () => {
        assert.deepEqual(checkGate({ event, needs: needs() }), []);
    });
    test(`${event}: shell-only changes do not require core or frontend`, () => {
        assert.deepEqual(
            checkGate({ event, needs: needs({ rust_shell: 'true' }, { 'rust-shell': 'success' }) }),
            [],
        );
    });
    test(`${event}: required jobs may not be skipped`, () => {
        for (const [area, job] of [
            ['frontend', 'frontend'],
            ['rust_core', 'rust-core'],
            ['rust_shell', 'rust-shell'],
        ]) {
            assert.ok(
                checkGate({ event, needs: needs({ [area]: 'true' }) }).some((s) => s.includes(job)),
            );
        }
    });
    test(`${event}: missing or invalid filter outputs fail closed`, () => {
        for (const changed of [undefined, '', 'unexpected']) {
            assert.ok(checkGate({ event, needs: needs({ rust_core: changed }) }).length);
        }
    });
}

for (const [suite, required] of Object.entries({
    full: ['frontend', 'rust-core', 'rust-shell'],
    frontend: ['frontend'],
    rust: ['rust-core', 'rust-shell'],
    'rust-smoke': ['rust-core'],
})) {
    test(`dispatch ${suite}: only selected suites are required`, () => {
        const results = Object.fromEntries(required.map((job) => [job, 'success']));
        assert.deepEqual(
            checkGate({ event: 'workflow_dispatch', suite, needs: needs({}, results) }),
            [],
        );
        for (const job of required) {
            assert.ok(
                checkGate({
                    event: 'workflow_dispatch',
                    suite,
                    needs: needs({}, { ...results, [job]: 'skipped' }),
                }).length,
            );
        }
    });
}

for (const result of ['failure', 'cancelled', undefined]) {
    test(`unselected job ${result} still fails the gate`, () => {
        assert.ok(checkGate({ event: 'push', needs: needs({}, { frontend: result }) }).length);
    });
    test(`path detection ${result} cannot produce a green gate`, () => {
        assert.ok(checkGate({ event: 'push', needs: needs({}, { changes: result }) }).length);
    });
}

test('unknown manual suite and unsupported events fail', () => {
    assert.ok(checkGate({ event: 'workflow_dispatch', suite: 'unknown', needs: needs() }).length);
    assert.ok(checkGate({ event: 'schedule', needs: needs() }).length);
});
