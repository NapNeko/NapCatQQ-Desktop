'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { test } = require('node:test');
const { mergeData } = require('./snowluma-data-migrate.cjs');

function fixture(t) {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ncd-snowluma-migrate-test-'));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    const source = path.join(root, 'source');
    const target = path.join(root, 'target');
    fs.mkdirSync(source);
    fs.mkdirSync(target);
    const write = (base, name, contents) => {
        const file = path.join(base, name);
        fs.mkdirSync(path.dirname(file), { recursive: true });
        fs.writeFileSync(file, contents);
        return file;
    };
    return { root, source, target, write };
}

test('migrates complete data including all accounts, hidden files and empty directories', (t) => {
    const { source, target, write } = fixture(t);
    const files = {
        'config/runtime.json': '{"runtime":"legacy"}',
        'config/webui.json': '{"webui":"legacy"}',
        'config/consent.json': '{"version":"test-consent"}',
        'config/onebot_10001.json': '{"account":10001}',
        'config/onebot_10002.json': '{"account":10002}',
        'records/session.bin': Buffer.from([0, 255, 1, 17]),
        '.hidden': 'preserved',
        'nested/.hidden/config.json': '{}',
        "names with spaces/quote'\nname": 'untrusted filename',
    };
    for (const [name, contents] of Object.entries(files)) write(source, name, contents);
    fs.mkdirSync(path.join(source, 'empty'));
    fs.chmodSync(path.join(source, 'config/runtime.json'), 0o640);
    fs.chmodSync(path.join(source, 'empty'), 0o750);

    mergeData(source, target);

    for (const name of Object.keys(files)) {
        assert.deepEqual(
            fs.readFileSync(path.join(target, name)),
            fs.readFileSync(path.join(source, name)),
        );
    }
    assert.deepEqual(fs.readdirSync(path.join(target, 'empty')), []);
    assert.equal(fs.statSync(path.join(target, 'config/runtime.json')).mode & 0o777, 0o640);
    assert.equal(fs.statSync(path.join(target, 'empty')).mode & 0o777, 0o750);
});

test('existing files, directory metadata and conflicting entry types win on every run', (t) => {
    const { source, target, write } = fixture(t);
    write(source, 'config/runtime.json', 'legacy runtime');
    write(source, 'config/onebot_10002.json', 'legacy second account');
    write(source, 'source-directory/child', 'legacy child');
    write(source, 'source-file', 'legacy file');
    write(target, 'config/runtime.json', 'new runtime');
    write(target, 'source-directory', 'new conflicting file');
    fs.mkdirSync(path.join(target, 'source-file'));
    fs.chmodSync(path.join(target, 'config'), 0o750);

    mergeData(source, target);
    write(target, 'config/onebot_10002.json', 'edited after migration');
    mergeData(source, target);

    assert.equal(fs.readFileSync(path.join(target, 'config/runtime.json'), 'utf8'), 'new runtime');
    assert.equal(
        fs.readFileSync(path.join(target, 'config/onebot_10002.json'), 'utf8'),
        'edited after migration',
    );
    assert.equal(
        fs.readFileSync(path.join(target, 'source-directory'), 'utf8'),
        'new conflicting file',
    );
    assert.deepEqual(fs.readdirSync(path.join(target, 'source-file')), []);
    assert.equal(fs.statSync(path.join(target, 'config')).mode & 0o777, 0o750);
});

test('preserves source symlinks without following them and never traverses target symlinks', (t) => {
    const { root, source, target, write } = fixture(t);
    const outside = path.join(root, 'outside');
    fs.mkdirSync(outside);
    write(outside, 'protected', 'outside data');
    write(source, 'linked-directory/injected', 'must not reach outside');
    write(source, 'linked-file', 'must not replace link');
    write(source, 'dangling-directory/injected', 'must not create link destination');
    fs.symlinkSync(outside, path.join(target, 'linked-directory'));
    fs.symlinkSync(path.join(outside, 'protected'), path.join(target, 'linked-file'));
    fs.symlinkSync(path.join(root, 'absent'), path.join(target, 'dangling-directory'));
    fs.symlinkSync('../missing', path.join(source, 'source-link'));
    fs.symlinkSync(outside, path.join(source, 'outside-link'));

    mergeData(source, target);
    mergeData(source, target);

    assert.deepEqual(fs.readdirSync(outside), ['protected']);
    assert.equal(fs.readFileSync(path.join(outside, 'protected'), 'utf8'), 'outside data');
    assert.equal(fs.existsSync(path.join(root, 'absent')), false);
    assert.equal(fs.readlinkSync(path.join(target, 'source-link')), '../missing');
    assert.equal(fs.readlinkSync(path.join(target, 'outside-link')), outside);
    assert.equal(fs.lstatSync(path.join(target, 'linked-file')).isSymbolicLink(), true);
});

test('copy failure publishes no partial destination and the unchanged snapshot can be retried', (t) => {
    const { source, target, write } = fixture(t);
    write(source, 'a.json', 'complete first file');
    write(source, 'b.json', 'complete second file');
    write(target, 'new.json', 'new configuration');
    const existingStaging = '.ncd-snowluma-migrate-preexisting';
    write(target, `${existingStaging}/payload`, 'unrelated data');
    const failingFs = {
        ...fs,
        copyFileSync(from, to, flags) {
            assert.equal(fs.statSync(path.dirname(to)).mode & 0o777, 0o700);
            if (path.basename(from) === 'b.json') {
                fs.writeFileSync(to, 'partial');
                throw Object.assign(new Error('injected copy failure'), { code: 'EIO' });
            }
            return fs.copyFileSync(from, to, flags);
        },
    };

    assert.throws(() => mergeData(source, target, failingFs), /injected copy failure/);
    assert.equal(fs.readFileSync(path.join(target, 'a.json'), 'utf8'), 'complete first file');
    assert.equal(fs.existsSync(path.join(target, 'b.json')), false);
    assert.deepEqual(fs.readdirSync(target), [existingStaging, 'a.json', 'new.json']);
    assert.equal(
        fs.readFileSync(path.join(target, existingStaging, 'payload'), 'utf8'),
        'unrelated data',
    );
    assert.equal(fs.readFileSync(path.join(source, 'b.json'), 'utf8'), 'complete second file');

    mergeData(source, target);
    assert.equal(fs.readFileSync(path.join(target, 'b.json'), 'utf8'), 'complete second file');
    assert.equal(fs.readFileSync(path.join(target, 'new.json'), 'utf8'), 'new configuration');
});

test('publication failure cleans staging and an entry created concurrently still wins', (t) => {
    const { source, target, write } = fixture(t);
    write(source, 'file', 'complete legacy data');
    const failureFs = {
        ...fs,
        linkSync() {
            throw Object.assign(new Error('injected publication failure'), { code: 'EIO' });
        },
    };
    assert.throws(() => mergeData(source, target, failureFs), /injected publication failure/);
    assert.deepEqual(fs.readdirSync(target), []);

    const concurrentFs = {
        ...fs,
        linkSync(from, to) {
            fs.writeFileSync(to, 'new concurrent data');
            return fs.linkSync(from, to);
        },
    };
    mergeData(source, target, concurrentFs);
    assert.equal(fs.readFileSync(path.join(target, 'file'), 'utf8'), 'new concurrent data');
    assert.deepEqual(fs.readdirSync(target), ['file']);
});

test('rejects special files and roots that overlap or use target symlink ancestors', (t) => {
    const { root, source, target, write } = fixture(t);
    write(source, 'special', 'placeholder');
    const specialFs = {
        ...fs,
        lstatSync(name) {
            const stat = fs.lstatSync(name);
            if (name === path.join(source, 'special')) {
                return {
                    ...stat,
                    isDirectory: () => false,
                    isFile: () => false,
                    isSymbolicLink: () => false,
                };
            }
            return stat;
        },
    };
    assert.throws(() => mergeData(source, target, specialFs), /unsupported file type/);
    assert.deepEqual(fs.readdirSync(target), []);
    assert.throws(() => mergeData(source, source), /must not overlap/);
    assert.throws(() => mergeData(source, path.join(source, 'nested')), /must not overlap/);
    assert.throws(() => mergeData(source, root), /must not overlap/);
    fs.symlinkSync(target, path.join(root, 'target-link'));
    assert.throws(() => mergeData(source, path.join(root, 'target-link')), /real directories/);
    assert.throws(
        () => mergeData(source, path.join(root, 'target-link', 'nested')),
        /real directories/,
    );
});

test('supports the Rust embedded node -e call and direct script invocation without logging data', (t) => {
    const { source, target, write } = fixture(t);
    const script = path.join(__dirname, 'snowluma-data-migrate.cjs');
    write(source, 'config/webui.json', 'test-only-secret-that-must-not-be-logged');
    const embedded = spawnSync(
        process.execPath,
        ['-e', fs.readFileSync(script, 'utf8'), source, target],
        { encoding: 'utf8' },
    );
    assert.equal(embedded.status, 0, embedded.stderr);
    assert.equal(embedded.stdout, '');
    assert.equal(embedded.stderr, '');
    assert.equal(
        fs.readFileSync(path.join(target, 'config/webui.json'), 'utf8'),
        'test-only-secret-that-must-not-be-logged',
    );
    const direct = spawnSync(process.execPath, [script, source, target], { encoding: 'utf8' });
    assert.equal(direct.status, 0, direct.stderr);
    const failure = spawnSync(
        process.execPath,
        ['-e', fs.readFileSync(script, 'utf8'), source, source],
        { encoding: 'utf8' },
    );
    assert.equal(failure.status, 1);
    assert.equal(failure.stdout, '');
    assert.equal(
        failure.stderr,
        'SnowLuma data migration failed; the original snapshot is retained.\n',
    );
});
