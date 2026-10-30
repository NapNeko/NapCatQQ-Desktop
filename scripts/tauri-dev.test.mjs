import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { devEnvironment, prepareLinuxDesktop } from './tauri-dev.mjs';

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

function workspaceFixture(t, name = 'source tree') {
    const temporary = mkdtempSync(path.join(tmpdir(), 'ncd-dev-desktop-'));
    t.after(() => rmSync(temporary, { recursive: true, force: true }));
    const workspace = path.join(temporary, name);
    mkdirSync(path.join(workspace, 'src-tauri'), { recursive: true });
    writeFileSync(
        path.join(workspace, 'src-tauri/tauri.conf.json'),
        JSON.stringify({ identifier: 'com.example.NCD', productName: 'NCD Desktop' }),
    );
    return { temporary, workspace };
}

test(
    'Linux dev registers its hidden identity without changing the production entry',
    {
        skip: process.platform !== 'linux',
    },
    (t) => {
        const { temporary, workspace } = workspaceFixture(t);
        const data = path.join(temporary, 'xdg-data');
        const applications = path.join(data, 'applications');
        mkdirSync(applications, { recursive: true });
        const production = path.join(applications, 'com.example.NCD.desktop');
        writeFileSync(production, 'production entry');
        const result = prepareLinuxDesktop({
            env: { XDG_DATA_HOME: data },
            workspace,
            executable: '/opt/node/bin/node',
        });
        assert.equal(result.id, 'com.example.NCD.Development');
        assert.equal(result.desktopFile, path.join(applications, `${result.id}.desktop`));
        const contents = readFileSync(result.desktopFile, 'utf8');
        assert.ok(contents.includes('Name=NCD Desktop (Development)\n'));
        assert.ok(contents.includes(`Icon=${workspace}/src-tauri/icons/256x256.png\n`));
        assert.ok(
            contents.includes(`Exec="/opt/node/bin/node" "${workspace}/scripts/tauri-dev.mjs"\n`),
        );
        assert.ok(contents.includes('NoDisplay=true\n'));
        assert.equal(readFileSync(production, 'utf8'), 'production entry');
    },
);

test(
    'Linux desktop fallback and Exec escaping preserve reserved path characters',
    {
        skip: process.platform !== 'linux',
    },
    (t) => {
        const { temporary, workspace } = workspaceFixture(t, 'source "quoted" \\ $ %f `');
        const home = path.join(temporary, 'home');
        const result = prepareLinuxDesktop({
            env: {},
            home,
            workspace,
            executable: '/opt/node "$\\`%f/bin/node',
        });
        assert.equal(
            result.desktopFile,
            path.join(home, '.local/share/applications', `${result.id}.desktop`),
        );
        const contents = readFileSync(result.desktopFile, 'utf8');
        const command = contents.split('\n').find((line) => line.startsWith('Exec='));
        for (const literal of ['"', '$', '`']) {
            assert.ok(command.includes('\\'.repeat(2) + literal));
        }
        assert.ok(command.includes('\\'.repeat(4)));
        assert.ok(command.includes('%%f'));
        assert.ok(
            contents
                .split('\n')
                .find((line) => line.startsWith('Icon='))
                .includes('%f'),
        );
        assert.equal(contents.includes('Terminal=true'), false);
    },
);
