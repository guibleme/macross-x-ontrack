import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { fileURLToPath, pathToFileURL } from "node:url";
import { check } from "./errors.mjs";
import { Store } from "./store.mjs";

// Restore only to a new directory. An active application's files are never replaced.
export function restoreBackup(source, destination) {
  source = fs.realpathSync(source);
  destination = path.resolve(destination);
  check(!fs.existsSync(destination), "restore_destination_exists", 409);
  const manifest = JSON.parse(
    fs.readFileSync(path.join(source, "manifest.json"), "utf8"),
  );
  check(
    manifest.version === 1 &&
      Array.isArray(manifest.artifacts) &&
      manifest.artifacts.length <= 100000,
    "invalid_backup",
  );
  const database = path.join(source, "macross.sqlite");
  check(fs.lstatSync(database).isFile(), "invalid_backup");
  const blobs = [];
  for (const hash of manifest.artifacts) {
    check(
      typeof hash === "string" && /^[a-f0-9]{64}$/.test(hash),
      "invalid_digest",
    );
    const filename = path.join(source, "artifacts", hash);
    check(fs.lstatSync(filename).isFile(), "invalid_backup");
    const bytes = fs.readFileSync(filename);
    check(
      createHash("sha256").update(bytes).digest("hex") === hash,
      "artifact_corrupt",
      409,
    );
    blobs.push([hash, bytes]);
  }
  fs.mkdirSync(destination, { recursive: true, mode: 0o700 });
  try {
    fs.copyFileSync(
      database,
      path.join(destination, "macross.sqlite"),
      fs.constants.COPYFILE_EXCL,
    );
    fs.chmodSync(path.join(destination, "macross.sqlite"), 0o600);
    fs.mkdirSync(path.join(destination, "artifacts"), { mode: 0o700 });
    for (const [hash, bytes] of blobs)
      fs.writeFileSync(path.join(destination, "artifacts", hash), bytes, {
        mode: 0o600,
        flag: "wx",
      });
    const config = path.join(source, "connections.json");
    if (fs.existsSync(config)) {
      check(fs.lstatSync(config).isFile(), "invalid_backup");
      fs.copyFileSync(config, path.join(destination, "connections.json"));
      fs.chmodSync(path.join(destination, "connections.json"), 0o600);
    }
    const restored = new Store(destination);
    const count = restored.fronts().length;
    restored.close();
    return { stateRoot: destination, fronts: count, artifacts: blobs.length };
  } catch (error) {
    fs.rmSync(destination, { recursive: true, force: true });
    throw error;
  }
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  check(process.argv.length === 4, "usage_restore_source_destination");
  process.stdout.write(
    JSON.stringify(restoreBackup(process.argv[2], process.argv[3])) + "\n",
  );
}
