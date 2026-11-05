import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import path from "node:path";
import test from "node:test";
import { ensureSidecar } from "../electron/sidecar.mjs";

const stateRoot = path.resolve("/tmp/macross-sidecar-unit");
const ready = () => ({
  ok: true,
  json: async () => ({
    data: { service: "macross-ontrack", version: 1, stateRoot },
  }),
});
const options = {
  stateRoot,
  script: "/application/ui/server/index.mjs",
  executable: "/application/electron",
  port: 4280,
  wait: async () => {},
};

test("reuses only a server with matching service, protocol and state identity", async () => {
  let spawned = false;
  assert.equal(
    await ensureSidecar({
      ...options,
      fetcher: async () => ready(),
      spawnProcess: () => {
        spawned = true;
      },
    }),
    "reused",
  );
  assert.equal(spawned, false);
});

test("does not replace a live server from another state directory", async () => {
  let spawned = false;
  const response = {
    ok: true,
    json: async () => ({
      data: {
        service: "macross-ontrack",
        version: 1,
        stateRoot: "/another/state",
      },
    }),
  };
  await assert.rejects(
    ensureSidecar({
      ...options,
      fetcher: async () => response,
      spawnProcess: () => {
        spawned = true;
      },
    }),
    /different application or state directory/,
  );
  assert.equal(spawned, false);
});

test("does not replace another application or incompatible health response", async () => {
  for (const value of [
    { service: "other", version: 1, stateRoot },
    { service: "macross-ontrack", version: 2, stateRoot },
  ]) {
    await assert.rejects(
      ensureSidecar({
        ...options,
        fetcher: async () => ({
          ok: true,
          json: async () => ({ data: value }),
        }),
        spawnProcess: () => assert.fail("must not spawn"),
      }),
      /different application/,
    );
  }
});

test("starts only the fixed local script with argument arrays and detaches its lifetime", async () => {
  let reads = 0,
    invocation,
    detached = false,
    killed = false;
  const child = new EventEmitter();
  child.unref = () => {
    detached = true;
  };
  child.kill = () => {
    killed = true;
  };
  const result = await ensureSidecar({
    ...options,
    fetcher: async () => {
      if (++reads === 1) throw new Error("connection refused");
      return ready();
    },
    spawnProcess: (...args) => {
      invocation = args;
      return child;
    },
  });
  assert.equal(result, "started");
  assert.deepEqual(invocation.slice(0, 2), [
    "/application/electron",
    ["/application/ui/server/index.mjs"],
  ]);
  assert.equal(invocation[2].shell, false);
  assert.equal(invocation[2].detached, true);
  assert.equal(invocation[2].env.MACROSS_STATE_ROOT, stateRoot);
  assert.equal(invocation[2].env.MACROSS_PORT, "4280");
  assert.equal(invocation[2].env.ELECTRON_RUN_AS_NODE, "1");
  assert.equal(detached, true);
  assert.equal(killed, false);
});

test("reports a spawn failure without treating the application as ready", async () => {
  const child = new EventEmitter();
  child.unref = () => {};
  await assert.rejects(
    ensureSidecar({
      ...options,
      fetcher: async () => {
        throw new Error("connection refused");
      },
      spawnProcess: () => {
        queueMicrotask(() =>
          child.emit("error", new Error("missing executable")),
        );
        return child;
      },
    }),
    /could not start/,
  );
});

test("rejects a foreign service that claims the port during startup", async () => {
  let reads = 0;
  const child = new EventEmitter();
  child.unref = () => {};
  await assert.rejects(
    ensureSidecar({
      ...options,
      fetcher: async () => {
        if (++reads === 1) throw new Error("connection refused");
        return { ok: false };
      },
      spawnProcess: () => child,
    }),
    /claimed by a different application/,
  );
});
