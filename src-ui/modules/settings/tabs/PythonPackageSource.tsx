// Python 框架安装时使用的默认索引。

import { useState } from 'react';
import { Select, TextField } from '../../../shared/ui';
import { ExpandPresence } from '../../../shared/ui/motion';
import { FieldRow } from '../_shared';

const sourceItems = [
    { value: 'uv', label: '跟随 uv 配置' },
    { value: 'https://pypi.org/simple', label: 'PyPI 原站' },
    { value: 'https://pypi.tuna.tsinghua.edu.cn/simple', label: '清华镜像' },
    { value: 'custom', label: '自定义地址' },
];

export function PythonPackageSource({
    value,
    error,
    onChange,
}: {
    value: string;
    error?: string;
    onChange: (value: string) => void;
}) {
    const [custom, setCustom] = useState(false);
    const selected = custom
        ? 'custom'
        : !value
          ? 'uv'
          : sourceItems.some((item) => item.value === value)
            ? value
            : 'custom';
    return (
        <FieldRow label="Python 包源">
            <div className="flex w-72 min-w-0 max-w-full flex-col gap-2">
                <Select
                    aria-label="Python 包源"
                    value={selected}
                    items={sourceItems}
                    onValueChange={(source) => {
                        setCustom(source === 'custom');
                        if (source === 'custom') {
                            if (!value) onChange('https://');
                        } else {
                            onChange(source === 'uv' ? '' : source);
                        }
                    }}
                />
                <ExpandPresence visible={selected === 'custom'}>
                    <TextField
                        aria-label="Python 自定义包源地址"
                        value={value}
                        onValueChange={onChange}
                        placeholder="https://pypi.example.com/simple"
                        spellCheck={false}
                        error={error}
                    />
                </ExpandPresence>
            </div>
        </FieldRow>
    );
}
