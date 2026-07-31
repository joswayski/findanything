import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const REPOSITORY = "joswayski/findanything";
const DEFAULT_BRANCH = "main";
const ITEM_COUNT = 6;

type GitHubRelease = {
  id: number;
  tag_name: string;
  name: string | null;
  html_url: string;
  published_at: string | null;
  created_at: string;
  prerelease: boolean;
  draft: boolean;
};

type GitHubCommit = {
  sha: string;
  html_url: string;
  commit: {
    message: string;
    committer: { date: string } | null;
    author: { date: string } | null;
  };
};

type RecentItem = {
  key: string;
  title: string;
  url: string;
  occurredAt: string;
  detail: string;
};

const githubHeaders = {
  Accept: "application/vnd.github+json",
  "User-Agent": "findanything-web-build",
  "X-GitHub-Api-Version": "2022-11-28",
};

async function fetchGitHub<T>(path: string): Promise<T> {
  const response = await fetch(`https://api.github.com/repos/${REPOSITORY}${path}`, {
    headers: githubHeaders,
  });

  if (!response.ok) {
    throw new Error(`GitHub request failed with ${response.status}: ${path}`);
  }

  return (await response.json()) as T;
}

async function fetchRecentReleases(): Promise<RecentItem[]> {
  const releases = await fetchGitHub<GitHubRelease[]>(`/releases?per_page=${ITEM_COUNT}`);

  return releases
    .filter((release) => !release.draft)
    .map((release) => ({
      key: String(release.id),
      title: release.name?.trim() || release.tag_name,
      url: release.html_url,
      occurredAt: release.published_at ?? release.created_at,
      detail: release.prerelease ? `${release.tag_name} · Pre-release` : release.tag_name,
    }));
}

function pullRequestNumber(title: string) {
  return (
    title.match(/\(#(\d+)\)$/u)?.[1] ??
    title.match(/^Merge pull request #(\d+)/u)?.[1] ??
    null
  );
}

function toRecentChange(entry: GitHubCommit): RecentItem {
  const title = entry.commit.message.split("\n", 1)[0]?.trim();
  const occurredAt = entry.commit.committer?.date ?? entry.commit.author?.date;

  if (!entry.sha || !entry.html_url || !title || !occurredAt) {
    throw new Error("GitHub returned an incomplete commit entry");
  }

  const prNumber = pullRequestNumber(title);

  return {
    key: entry.sha,
    title: prNumber ? title.replace(/\s+\(#\d+\)$/u, "") : title,
    url: prNumber
      ? `https://github.com/${REPOSITORY}/pull/${prNumber}`
      : entry.html_url,
    occurredAt,
    detail: prNumber ? `PR #${prNumber}` : entry.sha.slice(0, 7),
  };
}

async function fetchRecentChanges(): Promise<RecentItem[]> {
  const url = new URL(`https://api.github.com/repos/${REPOSITORY}/commits`);
  url.searchParams.set("sha", DEFAULT_BRANCH);
  url.searchParams.set("per_page", String(ITEM_COUNT));

  const response = await fetch(url, { headers: githubHeaders });
  if (!response.ok) {
    throw new Error(`GitHub history request failed with ${response.status}`);
  }

  const entries = (await response.json()) as GitHubCommit[];
  if (!Array.isArray(entries) || entries.length === 0) {
    throw new Error(`GitHub returned no commits for ${DEFAULT_BRANCH}`);
  }

  return entries.map(toRecentChange);
}

export default defineConfig(async () => {
  const releases = await fetchRecentReleases();
  const recentItems = releases.length > 0 ? releases : await fetchRecentChanges();
  const recentSectionTitle = releases.length > 0 ? "Recent releases" : "Latest changes";

  console.log(`Fetched ${recentItems.length} ${recentSectionTitle.toLowerCase()} from GitHub.`);

  return {
    plugins: [react()],
    define: {
      __RECENT_ITEMS__: JSON.stringify(recentItems),
      __RECENT_SECTION_TITLE__: JSON.stringify(recentSectionTitle),
    },
    server: {
      port: 5174,
    },
  };
});
