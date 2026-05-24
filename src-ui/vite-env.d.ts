/// <reference types="vite/client" />

interface ImportMetaEnv {
    readonly VITE_UI_NEXT?: string;
}

interface ImportMeta {
    readonly env: ImportMetaEnv;
}

declare module '*.svg' {
    const src: string;
    export default src;
}

declare module '*.svg?raw' {
    const content: string;
    export default content;
}

declare module '*.png' {
    const src: string;
    export default src;
}
