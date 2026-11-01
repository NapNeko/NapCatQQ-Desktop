// Run only against a stopped container's snapshot and an inactive data volume.
// Existing destination entries win. A file becomes visible only after its full
// contents and permissions have been staged on the destination filesystem.
'use strict';

const fs = require('node:fs');
const path = require('node:path');

function mergeData(source, target, fileSystem = fs) {
    const sourceRoot = path.resolve(source);
    const targetRoot = path.resolve(target);

    function entry(name) {
        try {
            return fileSystem.lstatSync(name);
        } catch (error) {
            if (error.code === 'ENOENT') return null;
            throw error;
        }
    }

    // Check the entire supplied path, including ancestors, before creating a root.
    // Child links are checked separately and are always treated as existing data.
    function checkAncestors(name) {
        const parent = path.dirname(name);
        if (parent !== name) checkAncestors(parent);
        const stat = entry(name);
        if (stat && (!stat.isDirectory() || stat.isSymbolicLink())) {
            throw new Error('Migration roots must use real directories');
        }
    }

    checkAncestors(sourceRoot);
    const sourceStat = entry(sourceRoot);
    if (!sourceStat?.isDirectory()) {
        throw new Error('Migration source directory is missing');
    }
    checkAncestors(targetRoot);
    if (
        sourceRoot === targetRoot ||
        sourceRoot.startsWith(`${targetRoot}${path.sep}`) ||
        targetRoot.startsWith(`${sourceRoot}${path.sep}`)
    ) {
        throw new Error('Migration source and target must not overlap');
    }
    fileSystem.mkdirSync(targetRoot, { recursive: true, mode: 0o700 });

    function copyFile(sourceFile, targetFile, stat) {
        const staging = fileSystem.mkdtempSync(
            path.join(path.dirname(targetFile), '.ncd-snowluma-migrate-'),
        );
        const temporary = path.join(staging, 'payload');
        try {
            fileSystem.copyFileSync(sourceFile, temporary, fs.constants.COPYFILE_EXCL);
            fileSystem.chmodSync(temporary, stat.mode & 0o7777);
            try {
                // The private staging directory lives on the same filesystem. link is an
                // atomic exclusive publication; it cannot replace a file or a symlink.
                fileSystem.linkSync(temporary, targetFile);
            } catch (error) {
                if (error.code !== 'EEXIST') throw error;
            }
        } finally {
            try {
                fileSystem.unlinkSync(temporary);
            } catch (error) {
                if (error.code !== 'ENOENT') throw error;
            }
            fileSystem.rmdirSync(staging);
        }
    }

    function mergeDirectory(sourceDirectory, targetDirectory) {
        for (const name of fileSystem.readdirSync(sourceDirectory).sort()) {
            const sourceEntry = path.join(sourceDirectory, name);
            const targetEntry = path.join(targetDirectory, name);
            const stat = fileSystem.lstatSync(sourceEntry);
            if (!stat.isDirectory() && !stat.isFile() && !stat.isSymbolicLink()) {
                throw new Error('Migration source contains an unsupported file type');
            }
            let existing = entry(targetEntry);
            if (stat.isDirectory()) {
                let created = false;
                if (!existing) {
                    try {
                        // Keep the new directory writable until all its children are copied.
                        fileSystem.mkdirSync(targetEntry, { mode: (stat.mode & 0o7777) | 0o700 });
                        created = true;
                    } catch (error) {
                        if (error.code !== 'EEXIST') throw error;
                    }
                    existing = fileSystem.lstatSync(targetEntry);
                }
                if (existing.isDirectory() && !existing.isSymbolicLink()) {
                    try {
                        mergeDirectory(sourceEntry, targetEntry);
                    } finally {
                        if (created) fileSystem.chmodSync(targetEntry, stat.mode & 0o7777);
                    }
                }
            } else if (!existing) {
                if (stat.isSymbolicLink()) {
                    try {
                        fileSystem.symlinkSync(fileSystem.readlinkSync(sourceEntry), targetEntry);
                    } catch (error) {
                        if (error.code !== 'EEXIST') throw error;
                    }
                } else {
                    copyFile(sourceEntry, targetEntry, stat);
                }
            }
        }
    }

    mergeDirectory(sourceRoot, targetRoot);
}

module.exports = { mergeData };

// Rust embeds this file and invokes `node -e SCRIPT source target`; retain direct
// script execution as well so the same program is covered by local smoke tests.
if (require.main === module || module.id === '[eval]') {
    const args = require.main === module ? process.argv.slice(2) : process.argv.slice(1);
    try {
        if (args.length !== 2) throw new Error('Expected source and target directories');
        mergeData(args[0], args[1]);
    } catch {
        // No config contents, credentials, paths or untrusted filenames in output.
        console.error('SnowLuma data migration failed; the original snapshot is retained.');
        process.exitCode = 1;
    }
}
