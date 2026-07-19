import { describe, expect, it } from 'vitest';
import {
    compareComponentVersion,
    compareSemver,
    normalizeComponentVersion,
} from './normalize';

describe('normalizeComponentVersion', () => {
    it('strips clap binary prefix and watch-/v prefix', () => {
        expect(normalizeComponentVersion('ncd-watch 0.2.0')).toBe('0.2.0');
        expect(normalizeComponentVersion('watch-v0.2.0')).toBe('0.2.0');
        expect(normalizeComponentVersion('v1.2.3')).toBe('1.2.3');
    });
});

describe('compareComponentVersion', () => {
    it('compares QQ core then numeric build id', () => {
        expect(compareComponentVersion('9.9.31-49738', '9.9.32')).toBeGreaterThan(0);
        expect(compareComponentVersion('9.9.32-50969', '9.9.32-51000')).toBeGreaterThan(0);
        expect(compareComponentVersion('9.9.32-51000', '9.9.32-50969')).toBeLessThan(0);
    });

    it('treats missing numeric build as official shorthand, not older', () => {
        expect(compareComponentVersion('9.9.32-50969', '9.9.32')).toBe(0);
        expect(compareComponentVersion('9.9.32', '9.9.32-50969')).toBe(0);
    });
});

describe('compareSemver', () => {
    it('does not flag QQ hot-update build as outdated vs pcConfig shorthand', () => {
        // 本机 versions/config.json curVersion vs pcConfig Windows.version
        expect(compareSemver('9.9.32-50969', '9.9.32')).toBe(0);
        expect(compareSemver('9.9.32', '9.9.32-50969')).toBe(0);
        expect(compareSemver('9.9.32-50969', '9.9.32')).not.toBeGreaterThan(0);
    });

    it('still reports real QQ upgrades', () => {
        expect(compareSemver('9.9.31-49738', '9.9.32')).toBeGreaterThan(0);
        expect(compareSemver('9.9.32-50969', '9.9.32-51000')).toBeGreaterThan(0);
        expect(compareSemver('9.9.32', '9.9.32')).toBe(0);
    });

    it('keeps SemVer pre-release older than release', () => {
        expect(compareSemver('1.2.3-rc1', '1.2.3')).toBeGreaterThan(0);
        expect(compareSemver('1.2.3', '1.2.3-rc1')).toBeLessThan(0);
    });

    it('compares plain semver cores', () => {
        expect(compareSemver('0.2.0', '0.2.1')).toBeGreaterThan(0);
        expect(compareSemver('v1.0.0', '1.0.0')).toBe(0);
        expect(compareSemver('watch-v0.2.0', '0.2.0')).toBe(0);
    });
});
