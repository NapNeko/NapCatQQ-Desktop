// 社区主题调色板。每个主题只给角色色，整套 token 由 paletteCss.ts 按明暗公式展开。
//
// 色值取自各主题官方仓库的调色板 / VS Code 主题文件（source 字段）。只用色值不用代码；许可：
// Tokyo Night（folke）是 Apache-2.0，其余 MIT（Gruvbox 仓库没放 LICENSE 文件，README 写的 MIT）。
// 原主题没给的角色（更深一档的底、卡片色、禁用字色等）按相邻色推出来，不硬凑原主题没有的颜色名。
// 角色怎么挑：
//   canvas    编辑器主背景
//   sidebar   侧栏背景（主题自己的 sideBar / 更深一档底色）
//   card      卡片：暗色主题照 Catppuccin 块的做法取比画布深一档的底色，浅色取最亮的面板色
//   elevated  弹层 / 菜单：比画布亮一档的浮层色
//   inset     输入框 / 卡内嵌区：最深（暗色）或略灰（浅色）的一档
//   muted     副表面：高亮 / 选中行一类的底色
//   text~text4  主文字 → 次要 → 注释色 → 禁用
//   brand     主题自己的强调色（焦点框 / 按钮 / 关键字）
//   accent    第二强调色
//   success / warning / danger / info  取调色板里的绿 / 黄 / 红 / 蓝（或主题约定的语义色）
// 族内顺序即选择器里的顺序；族与族的顺序按选择器 4 列拼行排好（3+1、2+2…）。

export type ThemeScheme = 'light' | 'dark';

export interface ThemePalette {
    id: string;
    /** 选择器分组名 */
    family: string;
    /** 卡片上的短名 */
    label: string;
    /** 全名，显示在触发按钮上 */
    name: string;
    scheme: ThemeScheme;
    source: string;
    canvas: string;
    sidebar: string;
    card: string;
    elevated: string;
    inset: string;
    muted: string;
    text: string;
    text2: string;
    text3: string;
    text4: string;
    brand: string;
    accent: string;
    success: string;
    warning: string;
    danger: string;
    info: string;
}

export const PALETTE_THEMES = [
    // ── Rosé Pine ──────────────────────────────────────────────────────────────
    {
        id: 'rose-pine', family: 'Rosé Pine', label: 'Main', name: 'Rosé Pine', scheme: 'dark',
        source: 'https://github.com/rose-pine/rose-pine-palette',
        canvas: '#191724', sidebar: '#1f1d2e', card: '#1f1d2e', elevated: '#26233a',
        inset: '#16141f', muted: '#26233a',
        text: '#e0def4', text2: '#908caa', text3: '#6e6a86', text4: '#524f67',
        brand: '#ebbcba', accent: '#c4a7e7',
        success: '#9ccfd8', warning: '#f6c177', danger: '#eb6f92', info: '#31748f',
    },
    {
        id: 'rose-pine-moon', family: 'Rosé Pine', label: 'Moon', name: 'Rosé Pine Moon', scheme: 'dark',
        source: 'https://github.com/rose-pine/rose-pine-palette',
        canvas: '#232136', sidebar: '#2a273f', card: '#2a273f', elevated: '#393552',
        inset: '#1f1d30', muted: '#393552',
        text: '#e0def4', text2: '#908caa', text3: '#6e6a86', text4: '#56526e',
        brand: '#ea9a97', accent: '#c4a7e7',
        success: '#9ccfd8', warning: '#f6c177', danger: '#eb6f92', info: '#3e8fb0',
    },
    {
        id: 'rose-pine-dawn', family: 'Rosé Pine', label: 'Dawn', name: 'Rosé Pine Dawn', scheme: 'light',
        source: 'https://github.com/rose-pine/rose-pine-palette',
        canvas: '#faf4ed', sidebar: '#f4ede8', card: '#fffaf3', elevated: '#fffaf3',
        inset: '#f2e9e1', muted: '#dfdad9',
        text: '#464261', text2: '#797593', text3: '#9893a5', text4: '#cecacd',
        brand: '#d7827e', accent: '#907aa9',
        success: '#56949f', warning: '#ea9d34', danger: '#b4637a', info: '#286983',
    },
    // ── Dracula ────────────────────────────────────────────────────────────────
    {
        id: 'dracula', family: 'Dracula', label: 'Dracula', name: 'Dracula', scheme: 'dark',
        source: 'https://github.com/dracula/dracula-theme',
        canvas: '#282a36', sidebar: '#21222c', card: '#21222c', elevated: '#343746',
        inset: '#191a21', muted: '#44475a',
        text: '#f8f8f2', text2: '#c5c8d6', text3: '#6272a4', text4: '#4d5578',
        brand: '#bd93f9', accent: '#ff79c6',
        success: '#50fa7b', warning: '#f1fa8c', danger: '#ff5555', info: '#8be9fd',
    },
    // ── Tokyo Night ────────────────────────────────────────────────────────────
    {
        id: 'tokyo-night', family: 'Tokyo Night', label: 'Night', name: 'Tokyo Night', scheme: 'dark',
        source: 'https://github.com/folke/tokyonight.nvim',
        canvas: '#1a1b26', sidebar: '#16161e', card: '#16161e', elevated: '#292e42',
        inset: '#0c0e14', muted: '#292e42',
        text: '#c0caf5', text2: '#a9b1d6', text3: '#737aa2', text4: '#545c7e',
        brand: '#7aa2f7', accent: '#bb9af7',
        success: '#9ece6a', warning: '#e0af68', danger: '#f7768e', info: '#7dcfff',
    },
    {
        id: 'tokyo-night-storm', family: 'Tokyo Night', label: 'Storm', name: 'Tokyo Night Storm', scheme: 'dark',
        source: 'https://github.com/folke/tokyonight.nvim',
        canvas: '#24283b', sidebar: '#1f2335', card: '#1f2335', elevated: '#292e42',
        inset: '#1b1e2d', muted: '#292e42',
        text: '#c0caf5', text2: '#a9b1d6', text3: '#737aa2', text4: '#545c7e',
        brand: '#7aa2f7', accent: '#bb9af7',
        success: '#9ece6a', warning: '#e0af68', danger: '#f7768e', info: '#7dcfff',
    },
    {
        id: 'tokyo-night-day', family: 'Tokyo Night', label: 'Day', name: 'Tokyo Night Day', scheme: 'light',
        source: 'https://github.com/folke/tokyonight.nvim',
        canvas: '#e1e2e7', sidebar: '#d0d5e3', card: '#eceef3', elevated: '#eceef3',
        inset: '#d0d5e3', muted: '#c4c8da',
        text: '#3760bf', text2: '#6172b0', text3: '#848cb5', text4: '#a8aecb',
        brand: '#2e7de9', accent: '#9854f1',
        success: '#587539', warning: '#8c6c3e', danger: '#f52a65', info: '#007197',
    },
    // ── Nord ───────────────────────────────────────────────────────────────────
    {
        id: 'nord', family: 'Nord', label: 'Nord', name: 'Nord', scheme: 'dark',
        source: 'https://github.com/nordtheme/nord',
        canvas: '#2e3440', sidebar: '#2b303b', card: '#3b4252', elevated: '#434c5e',
        inset: '#272c36', muted: '#434c5e',
        text: '#eceff4', text2: '#d8dee9', text3: '#9aa3b5', text4: '#4c566a',
        brand: '#88c0d0', accent: '#b48ead',
        success: '#a3be8c', warning: '#ebcb8b', danger: '#bf616a', info: '#81a1c1',
    },
    // ── One ────────────────────────────────────────────────────────────────────
    {
        id: 'one-dark-pro', family: 'One', label: 'Dark Pro', name: 'One Dark Pro', scheme: 'dark',
        source: 'https://github.com/Binaryify/OneDark-Pro',
        canvas: '#282c34', sidebar: '#21252b', card: '#21252b', elevated: '#2c313c',
        inset: '#1b1d23', muted: '#3e4451',
        text: '#abb2bf', text2: '#9da5b4', text3: '#7f848e', text4: '#5c6370',
        brand: '#61afef', accent: '#c678dd',
        success: '#98c379', warning: '#e5c07b', danger: '#e06c75', info: '#56b6c2',
    },
    {
        id: 'one-light', family: 'One', label: 'Light', name: 'One Light', scheme: 'light',
        source: 'https://github.com/akamud/vscode-theme-onelight',
        canvas: '#fafafa', sidebar: '#eaeaeb', card: '#ffffff', elevated: '#ffffff',
        inset: '#f0f0f1', muted: '#e5e5e6',
        text: '#383a42', text2: '#696c77', text3: '#a0a1a7', text4: '#c2c2c3',
        brand: '#4078f2', accent: '#a626a4',
        success: '#50a14f', warning: '#c18401', danger: '#e45649', info: '#0184bc',
    },
    // ── GitHub ─────────────────────────────────────────────────────────────────
    {
        id: 'github-dark', family: 'GitHub', label: 'Dark', name: 'GitHub Dark', scheme: 'dark',
        source: 'https://github.com/primer/github-vscode-theme',
        canvas: '#0d1117', sidebar: '#010409', card: '#161b22', elevated: '#161b22',
        inset: '#010409', muted: '#21262d',
        text: '#e6edf3', text2: '#c9d1d9', text3: '#7d8590', text4: '#484f58',
        brand: '#2f81f7', accent: '#bc8cff',
        success: '#3fb950', warning: '#d29922', danger: '#f85149', info: '#58a6ff',
    },
    {
        id: 'github-light', family: 'GitHub', label: 'Light', name: 'GitHub Light', scheme: 'light',
        source: 'https://github.com/primer/github-vscode-theme',
        canvas: '#ffffff', sidebar: '#f6f8fa', card: '#ffffff', elevated: '#ffffff',
        inset: '#f6f8fa', muted: '#eaeef2',
        text: '#1f2328', text2: '#424a53', text3: '#656d76', text4: '#8c959f',
        brand: '#0969da', accent: '#8250df',
        success: '#1a7f37', warning: '#9a6700', danger: '#cf222e', info: '#0969da',
    },
    // ── Gruvbox ────────────────────────────────────────────────────────────────
    {
        id: 'gruvbox-dark', family: 'Gruvbox', label: 'Dark', name: 'Gruvbox Dark', scheme: 'dark',
        source: 'https://github.com/morhetz/gruvbox',
        canvas: '#282828', sidebar: '#1d2021', card: '#32302f', elevated: '#3c3836',
        inset: '#1d2021', muted: '#3c3836',
        text: '#ebdbb2', text2: '#d5c4a1', text3: '#a89984', text4: '#665c54',
        brand: '#fe8019', accent: '#d3869b',
        success: '#b8bb26', warning: '#fabd2f', danger: '#fb4934', info: '#83a598',
    },
    {
        id: 'gruvbox-light', family: 'Gruvbox', label: 'Light', name: 'Gruvbox Light', scheme: 'light',
        source: 'https://github.com/morhetz/gruvbox',
        canvas: '#fbf1c7', sidebar: '#f2e5bc', card: '#f9f5d7', elevated: '#f9f5d7',
        inset: '#ebdbb2', muted: '#ebdbb2',
        text: '#3c3836', text2: '#504945', text3: '#7c6f64', text4: '#bdae93',
        brand: '#af3a03', accent: '#8f3f71',
        success: '#79740e', warning: '#b57614', danger: '#9d0006', info: '#076678',
    },
    // ── Solarized ──────────────────────────────────────────────────────────────
    {
        id: 'solarized-dark', family: 'Solarized', label: 'Dark', name: 'Solarized Dark', scheme: 'dark',
        source: 'https://github.com/altercation/solarized',
        canvas: '#002b36', sidebar: '#00212b', card: '#00212b', elevated: '#073642',
        inset: '#001e27', muted: '#073642',
        text: '#93a1a1', text2: '#839496', text3: '#657b83', text4: '#586e75',
        brand: '#268bd2', accent: '#d33682',
        success: '#859900', warning: '#b58900', danger: '#dc322f', info: '#2aa198',
    },
    {
        id: 'solarized-light', family: 'Solarized', label: 'Light', name: 'Solarized Light', scheme: 'light',
        source: 'https://github.com/altercation/solarized',
        canvas: '#fdf6e3', sidebar: '#eee8d5', card: '#fffbef', elevated: '#fffbef',
        inset: '#eee8d5', muted: '#eee8d5',
        text: '#586e75', text2: '#657b83', text3: '#839496', text4: '#93a1a1',
        brand: '#268bd2', accent: '#d33682',
        success: '#859900', warning: '#b58900', danger: '#dc322f', info: '#2aa198',
    },
    // ── Everforest ─────────────────────────────────────────────────────────────
    {
        id: 'everforest-dark', family: 'Everforest', label: 'Dark', name: 'Everforest Dark', scheme: 'dark',
        source: 'https://github.com/sainnhe/everforest',
        canvas: '#2d353b', sidebar: '#232a2e', card: '#232a2e', elevated: '#3d484d',
        inset: '#1e2326', muted: '#343f44',
        text: '#d3c6aa', text2: '#9da9a0', text3: '#859289', text4: '#7a8478',
        brand: '#a7c080', accent: '#d699b6',
        success: '#a7c080', warning: '#dbbc7f', danger: '#e67e80', info: '#7fbbb3',
    },
    {
        id: 'everforest-light', family: 'Everforest', label: 'Light', name: 'Everforest Light', scheme: 'light',
        source: 'https://github.com/sainnhe/everforest',
        canvas: '#fdf6e3', sidebar: '#efebd4', card: '#fffbef', elevated: '#fffbef',
        inset: '#f4f0d9', muted: '#efebd4',
        text: '#5c6a72', text2: '#829181', text3: '#939f91', text4: '#a6b0a0',
        brand: '#8da101', accent: '#df69ba',
        success: '#8da101', warning: '#dfa000', danger: '#f85552', info: '#3a94c5',
    },
    // ── Kanagawa / Monokai ─────────────────────────────────────────────────────
    {
        id: 'kanagawa', family: 'Kanagawa', label: 'Wave', name: 'Kanagawa Wave', scheme: 'dark',
        source: 'https://github.com/rebelot/kanagawa.nvim',
        canvas: '#1f1f28', sidebar: '#181820', card: '#1a1a22', elevated: '#2a2a37',
        inset: '#16161d', muted: '#363646',
        text: '#dcd7ba', text2: '#c8c093', text3: '#727169', text4: '#54546d',
        brand: '#7e9cd8', accent: '#957fb8',
        success: '#98bb6c', warning: '#e6c384', danger: '#e46876', info: '#7fb4ca',
    },
    {
        id: 'monokai', family: 'Monokai', label: 'Monokai', name: 'Monokai', scheme: 'dark',
        source: 'https://github.com/microsoft/vscode/tree/main/extensions/theme-monokai',
        canvas: '#272822', sidebar: '#1e1f1c', card: '#1e1f1c', elevated: '#414339',
        inset: '#1a1a17', muted: '#3e3d32',
        text: '#f8f8f2', text2: '#ccccc7', text3: '#88846f', text4: '#5b5a4c',
        brand: '#fd971f', accent: '#ae81ff',
        success: '#a6e22e', warning: '#e6db74', danger: '#f92672', info: '#66d9ef',
    },
] as const satisfies readonly ThemePalette[];

export type PaletteThemeId = (typeof PALETTE_THEMES)[number]['id'];
