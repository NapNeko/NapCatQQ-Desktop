import { describe, expect, it } from 'vitest';
import {
    parseMarkdownBlocks,
    preprocessReleaseNotesMarkdown,
    tokenizeInlineMarkdown,
} from './release-notes-markdown';

describe('preprocessReleaseNotesMarkdown', () => {
    it('strips NCD auto-release HTML comment markers', () => {
        const raw = [
            '## Tips',
            '- a',
            '',
            '<!-- BEGIN AUTO RELEASE NOTES -->',
            '## 修复功能',
            '- fix ssl',
            '<!-- END AUTO RELEASE NOTES -->',
            '',
            '## 重要提醒',
            '- issue',
        ].join('\n');

        const out = preprocessReleaseNotesMarkdown(raw);
        expect(out).not.toContain('BEGIN AUTO');
        expect(out).not.toContain('END AUTO');
        expect(out).not.toContain('<!--');
        expect(out).toContain('## 修复功能');
        expect(out).toContain('## 重要提醒');
    });
});

describe('parseMarkdownBlocks', () => {
    it('parses NCD-style notes without comment noise', () => {
        const blocks = parseMarkdownBlocks(
            [
                '# NapCatQQ Desktop 更新日志（v2.2.8）',
                '',
                '## Tips',
                '- v2.0 起为破坏性更新',
                '',
                '<!-- BEGIN AUTO RELEASE NOTES -->',
                '## 🐛 修复功能',
                '- 修复证书问题',
                '<!-- END AUTO RELEASE NOTES -->',
                '',
                '## ⚠️ 重要提醒',
                '- 通过 GitHub Issue 反馈',
            ].join('\n'),
        );

        expect(blocks.some((b) => b.kind === 'paragraph' && b.text.includes('BEGIN'))).toBe(
            false,
        );
        expect(blocks.filter((b) => b.kind === 'heading').map((b) => b.text)).toEqual([
            'NapCatQQ Desktop 更新日志（v2.2.8）',
            'Tips',
            '🐛 修复功能',
            '⚠️ 重要提醒',
        ]);
        expect(blocks.filter((b) => b.kind === 'list_item').map((b) => b.text)).toEqual([
            'v2.0 起为破坏性更新',
            '修复证书问题',
            '通过 GitHub Issue 反馈',
        ]);
    });

    it('parses NC ordered lists and standalone link lines', () => {
        const blocks = parseMarkdownBlocks(
            [
                '# v4.18.9',
                '[使用文档](https://napneko.github.io/)',
                '',
                '## 警告',
                '**注意QQ版本推荐使用 40768+ 版本**',
                '',
                '**[9.9.26-44343 X64 Win](https://example.com/qq.exe)**',
                '[LinuxX64 DEB 44343 ](https://example.com/qq.deb)',
                '',
                '## 更新',
                '',
                '### 🐛 修复',
                '1. 修复 WebUI 主题配置问题 (ae42eed6)',
                '2. 另一条修复',
            ].join('\n'),
        );

        const links = blocks.filter(
            (b) => b.kind === 'paragraph' && b.text.includes(']('),
        );
        expect(links.length).toBeGreaterThanOrEqual(3);

        const ordered = blocks.filter((b) => b.kind === 'list_item' && b.ordered);
        expect(ordered).toHaveLength(2);
        expect(ordered[0]?.text).toContain('修复 WebUI');
        expect(ordered[0]?.index).toBe(1);
    });

    it('parses ncd-watch GFM asset tables', () => {
        const blocks = parseMarkdownBlocks(
            [
                '## ncd-watch 0.2.0',
                '',
                '远端 Linux 主机侧监控。',
                '',
                '### Assets',
                '',
                '| 文件 | 目标 |',
                '|------|------|',
                '| ncd-watch-0.2.0-x86_64-unknown-linux-musl | x86_64 |',
                '| ncd-watch-0.2.0-aarch64-unknown-linux-musl | aarch64 |',
                '| SHA256SUMS | 校验和 |',
                '',
                'Tag: `watch-v0.2.0`',
            ].join('\n'),
        );

        const table = blocks.find((b) => b.kind === 'table');
        expect(table).toBeDefined();
        if (!table || table.kind !== 'table') return;
        expect(table.headers).toEqual(['文件', '目标']);
        expect(table.rows).toEqual([
            ['ncd-watch-0.2.0-x86_64-unknown-linux-musl', 'x86_64'],
            ['ncd-watch-0.2.0-aarch64-unknown-linux-musl', 'aarch64'],
            ['SHA256SUMS', '校验和'],
        ]);
        // 分隔行不应再以段落出现
        expect(
            blocks.some((b) => b.kind === 'paragraph' && b.text.includes('------')),
        ).toBe(false);
    });
});

describe('tokenizeInlineMarkdown', () => {
    it('parses bold-wrapped NC download links as links, not raw bold text', () => {
        const tokens = tokenizeInlineMarkdown(
            '**[9.9.26-44343 X64 Win](https://dldir1.qq.com/qq.exe)**',
        );
        expect(tokens).toEqual([
            {
                kind: 'link',
                label: '9.9.26-44343 X64 Win',
                href: 'https://dldir1.qq.com/qq.exe',
            },
        ]);
    });

    it('parses plain markdown links and bold text', () => {
        const tokens = tokenizeInlineMarkdown(
            '见 [使用文档](https://napneko.github.io/) 与 **注意版本**',
        );
        expect(tokens).toEqual([
            { kind: 'text', text: '见 ' },
            { kind: 'link', label: '使用文档', href: 'https://napneko.github.io/' },
            { kind: 'text', text: ' 与 ' },
            { kind: 'bold', text: '注意版本' },
        ]);
    });

    it('does not let bold swallow a following link', () => {
        const tokens = tokenizeInlineMarkdown(
            '**默认WebUi密钥为随机密码** [安装运行库](https://aka.ms/vc)',
        );
        expect(tokens.some((t) => t.kind === 'link' && t.href.includes('aka.ms'))).toBe(
            true,
        );
        expect(tokens.some((t) => t.kind === 'bold' && t.text.includes('默认WebUi'))).toBe(
            true,
        );
    });
});
