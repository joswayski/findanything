import { readFileSync } from "node:fs";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

type SiteData = {
  buildTime: string;
  recentItems: readonly {
    key: string;
    title: string;
    url: string;
    occurredAt: string;
    detail: string;
  }[];
  recentSectionTitle: string;
};

const siteData = JSON.parse(
  readFileSync(new URL("./.generated/site-data.json", import.meta.url), "utf8"),
) as SiteData;

export default defineConfig({
  plugins: [
    tanstackStart({
      prerender: {
        enabled: true,
        crawlLinks: true,
        failOnError: true,
      },
    }),
    react(),
  ],
  define: {
    __BUILD_TIME__: JSON.stringify(siteData.buildTime),
    __RECENT_ITEMS__: JSON.stringify(siteData.recentItems),
    __RECENT_SECTION_TITLE__: JSON.stringify(siteData.recentSectionTitle),
  },
  server: {
    port: 5174,
  },
});
