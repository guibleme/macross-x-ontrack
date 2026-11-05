import { spawn } from "node:child_process";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";

export async function ensureSidecar({
  stateRoot,
  script,
  executable = process.execPath,
  port = 4280,
  fetcher = fetch,
  spawnProcess = spawn,
  wait = delay,
}) {
  const root = path.resolve(stateRoot);
  const probe = async () => {
    let response;
    try {
      response = await fetcher(`http://127.0.0.1:${port}/api/health`, {
        signal: AbortSignal.timeout(800),
      });
    } catch {
      return "absent";
    }
    if (!response.ok) return "foreign";
    try {
      const body = await response.json();
      const value = body.data;
      return value?.service === "macross-ontrack" &&
        value.stateRoot === root &&
        value.version === 1
        ? "ready"
        : "foreign";
    } catch {
      return "foreign";
    }
  };
  const first = await probe();
  if (first === "ready") return "reused";
  if (first === "foreign")
    throw new Error(
      `Port ${port} belongs to a different application or state directory.`,
    );
  const child = spawnProcess(executable, [script], {
    cwd: path.dirname(script),
    detached: true,
    stdio: "ignore",
    shell: false,
    env: {
      ...process.env,
      ELECTRON_RUN_AS_NODE: "1",
      MACROSS_STATE_ROOT: root,
      MACROSS_PORT: String(port),
    },
  });
  let spawnError;
  child.once("error", (error) => {
    spawnError = error;
  });
  child.unref();
  for (let attempt = 0; attempt < 60; attempt++) {
    await wait(150);
    if (spawnError)
      throw new Error("The local application server could not start.");
    const status = await probe();
    if (status === "ready") return "started";
    if (status === "foreign")
      throw new Error(`Port ${port} was claimed by a different application.`);
  }
  throw new Error(
    "The local application server did not become ready. Check the server command and state directory.",
  );
}
