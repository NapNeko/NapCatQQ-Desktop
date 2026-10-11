import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { SimpleMarkdown } from './SimpleMarkdown';

describe('Markdown display', () => {
    it('keeps links and images usable through host callbacks', () => {
        const onOpenLink = vi.fn();
        const onOpenImage = vi.fn();
        render(
            <SimpleMarkdown
                text={
                    '[文档](https://example.com/docs)\n![示意图 #320px #200px](https://example.com/image.png)'
                }
                onOpenLink={onOpenLink}
                onOpenImage={onOpenImage}
            />,
        );
        fireEvent.click(screen.getByRole('button', { name: '文档' }));
        expect(onOpenLink).toHaveBeenCalledWith('https://example.com/docs');
        expect(screen.getByRole('img', { name: '示意图' })).toHaveAttribute(
            'src',
            'https://example.com/image.png',
        );
        fireEvent.click(screen.getByRole('button', { name: '查看图片：示意图' }));
        expect(onOpenImage).toHaveBeenCalledWith('https://example.com/image.png');
    });
    it('renders fenced and inline code literally without creating actions or dropping comments', () => {
        const code =
            '<!-- keep -->\n[](%7B%22version%22%3A2%7D)\n![literal](https://example.com/code.png)';
        const { container } = render(
            <SimpleMarkdown
                text={`\`\`\`text\n${code}\n\`\`\`\n\n\`![inline](https://example.com/inline.png)\``}
            />,
        );
        expect(container.querySelector('pre code')).toHaveTextContent('<!-- keep -->');
        expect(container.querySelector('pre code')?.textContent).toBe(code);
        expect(screen.getByText('![inline](https://example.com/inline.png)')).toBeInTheDocument();
        expect(screen.queryByRole('img')).not.toBeInTheDocument();
        expect(screen.queryByRole('button')).not.toBeInTheDocument();
    });
    it('does not load non-web image schemes', () => {
        render(
            <SimpleMarkdown text="![file](file:///C:/private.png)\n![script](javascript:alert(1))" />,
        );
        expect(screen.queryByRole('img')).not.toBeInTheDocument();
    });
});
