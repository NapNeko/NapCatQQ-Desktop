// 配置 Tab 排版：成对字段用两栏，组距略收，避免拉满整宽的目录行。

import type { ReactNode } from 'react';

export const CONFIG_PAIR = 'grid grid-cols-1 gap-x-5 gap-y-4 sm:grid-cols-2';

export const ConfigForm: React.FC<{ children: ReactNode }> = ({ children }) => (
    <div className="flex w-full flex-col gap-10">{children}</div>
);
