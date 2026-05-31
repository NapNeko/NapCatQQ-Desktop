/// <reference types="vite/client" />

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
