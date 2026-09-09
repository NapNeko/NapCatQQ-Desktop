import { describe, expect, it } from 'vitest';
import {
    isPosixAbsolute,
    joinPosix,
    normalizePosix,
    parentPosix,
    posixSegments,
    remoteServerIdFromHostId,
} from './posixPath';

describe('normalizePosix', () => {
    it('empty becomes root', () => {
        expect(normalizePosix('')).toBe('/');
        expect(normalizePosix('   ')).toBe('/');
    });

    it('collapses dots and slashes', () => {
        expect(normalizePosix('/root/../home/./u')).toBe('/home/u');
        expect(normalizePosix('root/game')).toBe('/root/game');
        expect(normalizePosix('/root/game/')).toBe('/root/game');
    });
});

describe('join / parent / segments', () => {
    it('joins under root and nested', () => {
        expect(joinPosix('/', 'root')).toBe('/root');
        expect(joinPosix('/root', 'game-qqbot')).toBe('/root/game-qqbot');
        expect(joinPosix('/root/game', '..')).toBe('/root');
    });

    it('parent of root is root', () => {
        expect(parentPosix('/')).toBe('/');
        expect(parentPosix('/root')).toBe('/');
        expect(parentPosix('/root/a')).toBe('/root');
    });

    it('segments skip root', () => {
        expect(posixSegments('/')).toEqual([]);
        expect(posixSegments('/root/a')).toEqual(['root', 'a']);
    });
});

describe('host id', () => {
    it('reads remote server id', () => {
        expect(isPosixAbsolute('/root')).toBe(true);
        expect(isPosixAbsolute('root')).toBe(false);
        expect(remoteServerIdFromHostId('remote:kunming-4-8')).toBe('kunming-4-8');
        expect(remoteServerIdFromHostId('local')).toBeNull();
    });
});
