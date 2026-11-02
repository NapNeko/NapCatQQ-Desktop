// 兼容公开入口；内部模块不反向依赖此文件。
export { ChatAccountStore } from './store/ChatAccountStore';
export type { ChatSnapshot } from './store/chatStore.types';
export { useChatSnapshot, useChatSelector } from './store/useChatSnapshot';
export {
    chatAccount,
    selectChatBot,
    getChatSelectedBot,
    reconcileChatAccounts,
    restoreChatView,
    loadChatView,
    releaseChatAccount,
    prepareChatHandoff,
} from './store/chatRegistry';
