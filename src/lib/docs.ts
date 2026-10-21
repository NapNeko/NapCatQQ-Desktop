import type { CollectionEntry } from 'astro:content';

export const sections = ['开始使用', '日常使用', '远端与部署', '维护与排错', '文档说明'];
export const baseline = { version: '3.5.2', sha: '85cd7f40739d38a36c40d9d48ff5b390a2056cff' };
export const docsPath = (id: string) => `${import.meta.env.BASE_URL}docs/${id === 'index' ? '' : `${id}/`}`;
export const sortDocs = (entries: CollectionEntry<'docs'>[]) => entries.sort((a, b) => a.data.order - b.data.order);
