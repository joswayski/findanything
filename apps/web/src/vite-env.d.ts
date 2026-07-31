/// <reference types="vite/client" />

type RecentItem = {
  key: string;
  title: string;
  url: string;
  occurredAt: string;
  detail: string;
};

declare const __RECENT_ITEMS__: readonly RecentItem[];
declare const __RECENT_SECTION_TITLE__: string;
