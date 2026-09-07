// JSON 原文编辑：实现在 shared SyntaxTextEditor（CodeMirror），这里保持原导入点。

import { SyntaxTextEditor, type SyntaxTextEditorProps } from '../../../shared/ui/SyntaxTextEditor';

type Props = Pick<SyntaxTextEditorProps, 'value' | 'onChange' | 'invalid' | 'className'> & {
    'aria-label'?: string;
};

export function ThemeJsonEditor(props: Props) {
    return <SyntaxTextEditor mode="json" wrap {...props} />;
}
