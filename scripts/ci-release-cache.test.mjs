import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { cleanReleaseCache } from './ci-release-cache.mjs';

function fixture(t, files = []) {
    const root = mkdtempSync(join(tmpdir(), 'ncd-release-cache-'));
    t.after(() => rmSync(root, { recursive: true, force: true }));
    for (const file of files) {
        const path = join(root, file);
        mkdirSync(dirname(path), { recursive: true });
        writeFileSync(path, 'fixture');
    }
    return root;
}

test('prunes workspace outputs on both target layouts and preserves dependencies and uploaded assets', (t) => {
    const removed = [
        'target/release/deps/libncd_network-123.rlib',
        'target/release/deps/ncd_runtime-123.rcgu.o',
        'target/release/.fingerprint/ncd-network-123/run-build-script.json',
        'target/release/build/ncd-network-123/out/proxy_constants.rs',
        'target/release/NapCatQQ-Desktop.exe',
        'target/release/bundle/msi/app.msi',
        'target/release/wix/main.wixobj',
        'target/x86_64-pc-windows-msvc/release/ncd-agent.exe',
        'target/x86_64-pc-windows-msvc/release/deps/ncd_agent-123.exe',
        'target/debug/incremental/session/data.bin',
    ];
    const kept = [
        'target/release/deps/libserde-123.rlib',
        'target/release/.fingerprint/serde-123/lib-serde.json',
        'target/release/build/openssl-sys-123/out/libssl.a',
        'target/x86_64-pc-windows-msvc/release/deps/tokio-123.rlib',
        'release-assets/NapCatQQ-Desktop-x64.msi',
    ];
    const root = fixture(t, [...removed, ...kept]);
    assert.throws(() => cleanReleaseCache(root, { check: true }), /workspace artifacts/);
    for (const file of removed) assert.ok(existsSync(join(root, file)));
    assert.equal(cleanReleaseCache(root), removed.length);
    for (const file of removed) assert.equal(existsSync(join(root, file)), false);
    for (const file of kept) assert.equal(readFileSync(join(root, file), 'utf8'), 'fixture');
    assert.equal(cleanReleaseCache(root, { check: true }), 0);
});

test('missing target is a cold cache', (t) => {
    assert.equal(cleanReleaseCache(fixture(t), { check: true }), 0);
});

test('rejects a linked target without touching its destination', (t) => {
    const root = fixture(t, ['outside/ncd-agent.exe']);
    symlinkSync(join(root, 'outside'), join(root, 'target'), 'junction');
    assert.throws(() => cleanReleaseCache(root), /linked target/);
    assert.equal(readFileSync(join(root, 'outside/ncd-agent.exe'), 'utf8'), 'fixture');
});

test('rejects nested links before removing any workspace outputs', (t) => {
    const root = fixture(t, ['target/release/ncd-agent.exe', 'outside/keep.txt']);
    symlinkSync(join(root, 'outside'), join(root, 'target/release/external'), 'junction');
    assert.throws(() => cleanReleaseCache(root), /linked cache entry/);
    assert.ok(existsSync(join(root, 'target/release/ncd-agent.exe')));
    assert.equal(readFileSync(join(root, 'outside/keep.txt'), 'utf8'), 'fixture');
});
