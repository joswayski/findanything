import { access, cp, lstat, mkdir, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const webOutputDirectory = fileURLToPath(new URL("../dist/client/", import.meta.url));
const rootOutputDirectory = fileURLToPath(new URL("../../../dist/", import.meta.url));

await access(join(webOutputDirectory, "index.html"));

try {
  const outputStat = await lstat(rootOutputDirectory);
  if (outputStat.isSymbolicLink()) {
    throw new Error(`Refusing to replace linked output directory: ${rootOutputDirectory}`);
  }
} catch (error) {
  if (!(error instanceof Error && "code" in error && error.code === "ENOENT")) {
    throw error;
  }
}

await rm(rootOutputDirectory, { recursive: true, force: true });
await mkdir(rootOutputDirectory, { recursive: true });
await cp(webOutputDirectory, rootOutputDirectory, { recursive: true });

console.log(`Staged website assets in ${rootOutputDirectory}`);
