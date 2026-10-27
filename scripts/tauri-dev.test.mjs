import assert from 'node:assert/strict';
import test from 'node:test';
import { devEnvironment } from './tauri-dev.mjs';

test('Windows dev cache stays outside the checkout and isolates worktrees', () => {
    const env = Object.freeze({
        LOCALAPPDATA: 'C:\\Users\\dev\\AppData\\Local',
        OTHER: 'retained',
    });
    const first = devEnvironment(env, 'win32', 'D:\\work\\main');
    const second = devEnvironment(env, 'win32', 'D:\\work\\feature');
    assert.ok(first.CARGO_TARGET_DIR.startsWith(`${env.LOCALAPPDATA}\\`));
    assert.notEqual(first.CARGO_TARGET_DIR, second.CARGO_TARGET_DIR);
    assert.equal(first.OTHER, env.OTHER);
    assert.equal(
        first.CARGO_TARGET_DIR,
        devEnvironment(env, 'win32', 'd:\\WORK\\MAIN').CARGO_TARGET_DIR,
    );
});

test('explicit Cargo output is preserved', () => {
    const env = Object.freeze({ CARGO_TARGET_DIR: 'D:\\custom-cache' });
    assert.deepEqual(devEnvironment(env, 'win32'), env);
});

test('other platforms retain the existing Cargo environment', () => {
    const env = Object.freeze({ HOME: '/home/dev' });
    assert.deepEqual(devEnvironment(env, 'linux'), env);
});
