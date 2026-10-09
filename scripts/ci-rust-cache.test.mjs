import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import {
    mkdtempSync,
    mkdirSync,
    readFileSync,
    rmSync,
    statSync,
    utimesSync,
    writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { test } from 'node:test';
import { captureInputs, restoreInputs, sourceKey } from './ci-rust-cache.mjs';

function fixture(t, files) {
    const root = mkdtempSync(resolve(tmpdir(), 'ncd-ci-cache-'));
    t.after(() => rmSync(root, { recursive: true, force: true }));
    execFileSync('git', ['init', '--quiet'], { cwd: root });
    for (const [file, content] of Object.entries(files)) {
        const path = resolve(root, file);
        mkdirSync(dirname(path), { recursive: true });
        writeFileSync(path, content);
    }
    execFileSync('git', ['add', '--', ...Object.keys(files)], { cwd: root });
    return { root, manifest: resolve(root, 'target/ci-source-mtimes.json') };
}

test('unchanged contents reuse prior timestamps after a fresh checkout', (t) => {
    const { root, manifest } = fixture(t, { 'src/lib.rs': 'pub fn value() -> u8 { 1 }' });
    const file = resolve(root, 'src/lib.rs');
    const oldTime = Date.now() - 60_000;
    utimesSync(file, oldTime / 1000, oldTime / 1000);
    captureInputs(root, manifest);
    writeFileSync(file, readFileSync(file));
    assert.equal(restoreInputs(root, manifest).restored, 1);
    assert.ok(Math.abs(statSync(file).mtimeMs - oldTime) < 2);
});

test('changed contents keep their new timestamps and trigger normal invalidation', (t) => {
    const { root, manifest } = fixture(t, { 'src/lib.rs': 'pub fn value() -> u8 { 1 }' });
    const file = resolve(root, 'src/lib.rs');
    captureInputs(root, manifest);
    writeFileSync(file, 'pub fn value() -> u8 { 2 }');
    const changedTime = statSync(file).mtimeMs;
    assert.deepEqual(restoreInputs(root, manifest), { restored: 0, changed: 1 });
    assert.equal(statSync(file).mtimeMs, changedTime);
});

test('source changes get a new cache key even before staging', (t) => {
    const { root } = fixture(t, { 'src/lib.rs': 'pub fn value() -> u8 { 1 }' });
    const first = sourceKey(root);
    writeFileSync(resolve(root, 'src/lib.rs'), 'pub fn value() -> u8 { 2 }');
    assert.notEqual(sourceKey(root), first);
});

test('directory timestamps are reused only when membership and contents match', (t) => {
    const { root, manifest } = fixture(t, { 'src/lib.rs': 'pub fn value() -> u8 { 1 }' });
    const dir = resolve(root, 'src');
    const oldTime = Date.now() - 60_000;
    utimesSync(dir, oldTime / 1000, oldTime / 1000);
    captureInputs(root, manifest);
    utimesSync(dir, Date.now() / 1000, Date.now() / 1000);
    restoreInputs(root, manifest);
    assert.ok(Math.abs(statSync(dir).mtimeMs - oldTime) < 2);
    writeFileSync(resolve(root, 'src/new.rs'), 'pub fn added() {}');
    const changedTime = statSync(dir).mtimeMs;
    restoreInputs(root, manifest);
    assert.equal(statSync(dir).mtimeMs, changedTime);
});

test('untracked files and paths from the cached manifest cannot be touched', (t) => {
    const { root, manifest } = fixture(t, { 'src/lib.rs': 'pub fn value() -> u8 { 1 }' });
    captureInputs(root, manifest);
    const outside = resolve(root, 'private.txt');
    writeFileSync(outside, 'private');
    const before = statSync(outside).mtimeMs;
    const snapshot = JSON.parse(readFileSync(manifest, 'utf8'));
    snapshot.files['private.txt'] = { ...snapshot.files['src/lib.rs'], mtimeMs: 0 };
    snapshot.files['../outside.txt'] = snapshot.files['private.txt'];
    writeFileSync(manifest, JSON.stringify(snapshot));
    restoreInputs(root, manifest);
    assert.equal(statSync(outside).mtimeMs, before);
});

test('missing and corrupt metadata fall back to Cargo rebuilding', (t) => {
    const { root, manifest } = fixture(t, { 'src/lib.rs': 'pub fn value() -> u8 { 1 }' });
    assert.deepEqual(restoreInputs(root, manifest), { restored: 0, changed: 0 });
    mkdirSync(dirname(manifest), { recursive: true });
    writeFileSync(manifest, '{broken');
    assert.deepEqual(restoreInputs(root, manifest), { restored: 0, changed: 0 });
    writeFileSync(manifest, 'null');
    assert.deepEqual(restoreInputs(root, manifest), { restored: 0, changed: 0 });
    writeFileSync(manifest, JSON.stringify({ version: 2, files: {} }));
    assert.deepEqual(restoreInputs(root, manifest), { restored: 0, changed: 0 });
});

test('generated proxy placeholder is recreated before checking cached inputs', (t) => {
    const proxy = 'crates/ncd-network/src/proxy_constants.rs';
    const { root, manifest } = fixture(t, {
        'crates/ncd-network/src/proxy_constants.template.rs': 'pub const BASE: &str = "";',
    });
    restoreInputs(root, manifest);
    captureInputs(root, manifest);
    rmSync(resolve(root, proxy));
    rmSync(resolve(root, '.env'));
    assert.equal(restoreInputs(root, manifest).restored, 3);
    assert.equal(readFileSync(resolve(root, proxy), 'utf8'), 'pub const BASE: &str = "";');
});

test('existing dotenv contents are preserved and untracked values stay out of metadata', (t) => {
    const { root, manifest } = fixture(t, {
        'crates/ncd-network/src/proxy_constants.template.rs': 'pub const BASE: &str = "";',
    });
    const dotenv = resolve(root, '.env');
    writeFileSync(dotenv, 'EXISTING_CONFIG=keep\n');
    restoreInputs(root, manifest);
    assert.equal(readFileSync(dotenv, 'utf8'), 'EXISTING_CONFIG=keep\n');
    captureInputs(root, manifest);
    assert.equal(JSON.parse(readFileSync(manifest, 'utf8')).files['.env'], undefined);
});

test(
    'Cargo reuses identical inputs and rebuilds a real source change',
    { skip: process.env.CI_RUST_CACHE_TEST_CARGO !== '1' },
    (t) => {
        const { root, manifest } = fixture(t, {
            'Cargo.toml':
                '[package]\nname = "ci-cache-fixture"\nversion = "0.1.0"\nedition = "2021"\n',
            'src/lib.rs': '#[test]\nfn value() { assert_eq!(1 + 1, 2); }\n',
            'build.rs': 'fn main() { println!("cargo:rerun-if-changed=.env"); }\n',
            'crates/ncd-network/src/proxy_constants.template.rs': 'pub const BASE: &str = "";',
        });
        restoreInputs(root, manifest);
        const runCargo = () => {
            return spawnSync('cargo', ['test', '--offline', '--lib', '--color', 'never'], {
                cwd: root,
                encoding: 'utf8',
                stdio: ['ignore', 'pipe', 'pipe'],
                env: {
                    ...process.env,
                    CARGO_TARGET_DIR: resolve(root, 'target'),
                    CARGO_INCREMENTAL: '0',
                    CARGO_BUILD_JOBS: '1',
                    RUST_TEST_THREADS: '1',
                    RUSTFLAGS: '-C debuginfo=0',
                },
            });
        };
        const first = runCargo();
        assert.equal(first.status, 0, first.stderr);
        assert.match(first.stdout, /1 passed; 0 failed/);
        captureInputs(root, manifest);
        const source = resolve(root, 'src/lib.rs');
        writeFileSync(source, readFileSync(source));
        rmSync(resolve(root, '.env'));
        restoreInputs(root, manifest);
        const reused = runCargo();
        assert.equal(reused.status, 0, reused.stderr);
        assert.match(reused.stdout, /1 passed; 0 failed/);
        assert.doesNotMatch(reused.stderr, /Compiling ci-cache-fixture/);
        writeFileSync(source, '#[test]\nfn value() { assert_eq!(1 + 1, 3); }\n');
        assert.equal(restoreInputs(root, manifest).changed, 1);
        const changed = runCargo();
        assert.match(changed.stderr, /Compiling ci-cache-fixture/);
        assert.equal(changed.status, 101);
        assert.match(changed.stdout, /0 passed; 1 failed/);
    },
);
