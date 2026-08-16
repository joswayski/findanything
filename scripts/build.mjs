import { spawnSync } from "node:child_process";

// Workers Builds sets WORKERS_CI=1 and still runs `npm run build`.
// Locally that script builds the Tauri UI; on Cloudflare it must build the site.
const workersCi = process.env.WORKERS_CI === "1" || process.env.WORKERS_CI === "true";
const script = workersCi ? "build:web" : "build:desktop";
const result = spawnSync("npm", ["run", script], {
  stdio: "inherit",
  shell: process.platform === "win32",
});
process.exit(result.status ?? 1);
