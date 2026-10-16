// 密码 / 密钥输入字段：默认 type="password"、关闭拼写检查与自动补全，内置右侧眼睛显隐按钮。

import { forwardRef } from 'react';
import { TextField, type TextFieldProps } from './TextField';

export type SecretFieldProps = Omit<TextFieldProps, 'type'>;

export const SecretField = forwardRef<HTMLInputElement, SecretFieldProps>(
    ({ autoComplete = 'off', spellCheck = false, ...rest }, ref) => (
        <TextField
            ref={ref}
            type="password"
            autoComplete={autoComplete}
            spellCheck={spellCheck}
            {...rest}
        />
    ),
);
SecretField.displayName = 'SecretField';
