import { describe, expect, it } from 'vitest';
import { deserializeOscValue, parseOsc133, parseOsc633, parseOsc7, parseOsc9 } from './osc';

describe('deserializeOscValue', () => {
    it('decodes the escapes the shell scripts emit', () => {
        expect(deserializeOscValue('C:\\\\Users\\\\a')).toBe('C:\\Users\\a');
        expect(deserializeOscValue('echo a\\x3b echo b')).toBe('echo a; echo b');
        expect(deserializeOscValue('line\\x0anext')).toBe('line\nnext');
        // 没转义的反斜杠（cmd 那种原样路径）不动
        expect(deserializeOscValue('C:\\Windows')).toBe('C:\\Windows');
    });
});

describe('parseOsc633', () => {
    it('reads prompt and command marks', () => {
        expect(parseOsc633('A')).toEqual({ kind: 'prompt_start' });
        expect(parseOsc633('B')).toEqual({ kind: 'prompt_end' });
        expect(parseOsc633('C')).toEqual({ kind: 'command_start' });
        expect(parseOsc633('D;0')).toEqual({ kind: 'command_end', exitCode: 0 });
        expect(parseOsc633('D;127')).toEqual({ kind: 'command_end', exitCode: 127 });
        expect(parseOsc633('D')).toEqual({ kind: 'command_end', exitCode: null });
    });

    it('reads the command line and the cwd', () => {
        expect(parseOsc633('E;ls -la\\x3b pwd')).toEqual({
            kind: 'command_line',
            command: 'ls -la; pwd',
        });
        expect(parseOsc633('E;uv pip list;nonce123')).toEqual({
            kind: 'command_line',
            command: 'uv pip list',
        });
        expect(parseOsc633('E;')).toEqual({ kind: 'command_line', command: '' });
        expect(parseOsc633('P;Cwd=/home/u/麦麦')).toEqual({ kind: 'cwd', path: '/home/u/麦麦' });
        expect(parseOsc633('P;Cwd=C:\\\\Windows')).toEqual({ kind: 'cwd', path: 'C:\\Windows' });
        expect(parseOsc633('P;IsWindows=True')).toBeNull();
        expect(parseOsc633('Z')).toBeNull();
    });
});

describe('other sequences', () => {
    it('reads FinalTerm 133', () => {
        expect(parseOsc133('A')).toEqual({ kind: 'prompt_start' });
        expect(parseOsc133('D;2')).toEqual({ kind: 'command_end', exitCode: 2 });
        expect(parseOsc133('D')).toEqual({ kind: 'command_end', exitCode: null });
    });

    it('reads OSC 7 file urls', () => {
        expect(parseOsc7('file://vps1/home/u/my%20dir')).toEqual({
            kind: 'cwd',
            path: '/home/u/my dir',
        });
        expect(parseOsc7('file:///C:/Users/x')).toEqual({ kind: 'cwd', path: 'C:\\Users\\x' });
        expect(parseOsc7('http://x/y')).toBeNull();
    });

    it('reads Windows Terminal cwd and ConEmu progress', () => {
        expect(parseOsc9('9;C:\\Windows')).toEqual({ kind: 'cwd', path: 'C:\\Windows' });
        expect(parseOsc9('9;"D:\\a b"')).toEqual({ kind: 'cwd', path: 'D:\\a b' });
        expect(parseOsc9('4;1;42')).toEqual({ kind: 'progress', state: 1, value: 42 });
        expect(parseOsc9('4;0;0')).toEqual({ kind: 'progress', state: 0, value: 0 });
        expect(parseOsc9('4;9;1')).toBeNull();
        expect(parseOsc9('hello')).toBeNull();
    });
});
