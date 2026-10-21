export type ImageSource = { src: string; srcset: string };
export type ThemeImageSources = { light: ImageSource; dark: ImageSource };
export const emptyImage: string;
export function imageAsset(source: string, base?: string): ImageSource;
export function themeImageSources(poster: string, base?: string): ThemeImageSources;
