import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { randomUUID } from "node:crypto";
import { createApplication, createServer } from "../server/index.mjs";
import { Store } from "../server/store.mjs";
import { restoreBackup } from "../server/restore.mjs";
import { AppError } from "../server/errors.mjs";

function fixture(t, { fake } = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "macross-test-"));
  const app = createApplication({
    stateRoot: root,
    providers: fake ? { engine: fake } : {},
  });
  app.config.save({
    version: 1,
    revision: 0,
    projects: [
      {
        id: "demo",
        name: "Demo",
        repo: root,
        documentRoots: [root],
        executorId: "engine",
      },
    ],
    connections: [
      {
        id: "engine",
        pluginId: "ontrack",
        name: "Engine",
        projectId: "demo",
        enabled: Boolean(fake),
        config: {},
      },
      {
        id: "docs",
        pluginId: "local-documents",
        name: "Documents",
        projectId: "demo",
        enabled: true,
        config: {},
      },
      {
        id: "sessions",
        pluginId: "agent-session",
        name: "Sessions",
        projectId: "demo",
        enabled: true,
        config: {},
      },
    ],
  });
  t.after(() => {
    try {
      app.close();
    } catch {}
    fs.rmSync(root, { recursive: true, force: true });
  });
  return { app, root };
}
function front(app, title = "A") {
  return app.createFront({
    projectId: "demo",
    title,
    objective: "Preserve context",
    requestId: randomUUID(),
  });
}
function approved(app, f) {
  const plan = app.plan(f.id, {
    title: "First delivery",
    waves: [
      {
        id: "one",
        title: "First wave",
        criteria: ["A visible result"],
        dependsOn: [],
      },
      {
        id: "two",
        title: "Second wave",
        criteria: ["A verified follow-up"],
        dependsOn: ["one"],
      },
    ],
    expectedRevision: f.revision,
    requestId: randomUUID(),
  });
  app.approve(f.id, plan.id, { expectedRevision: 1, requestId: randomUUID() });
  return plan;
}
function provider() {
  const calls = [];
  return {
    calls,
    async call(project, method, params, id) {
      calls.push({ method, params, id });
      if (method === "agents.list")
        return [{ id: "test-agent", name: "Test agent", available: true }];
      if (method === "worktrees.list") return [];
      if (method === "task.create") return { id: "demo-task" };
      if (method === "task.ready") return { id: params.id, ready: true };
      if (method === "session.start") return { sessionId: "test-session" };
      if (method === "session.observe") return { text: "A durable answer" };
      if (method === "session.send") return { status: "submitted" };
      if (method === "capabilities") return { version: 1 };
      throw new Error("Unexpected method");
    },
  };
}

test("fronts and documents survive restart without external connections", async (t) => {
  const { app, root } = fixture(t),
    f = front(app);
  app.entry(f.id, {
    kind: "decision",
    text: "Keep the scope small",
    expectedRevision: 0,
    requestId: "decision",
  });
  const artifact = app.addArtifact(f.id, {
    name: "evidence.md",
    text: "Complete evidence",
    requestId: "document",
  });
  app.close();
  const reopened = createApplication({ stateRoot: root });
  t.after(() => reopened.close());
  const detail = await reopened.detail(f.id);
  assert.equal(detail.front.revision, 1);
  assert.equal(detail.entries[0].text, "Keep the scope small");
  assert.equal(
    reopened.store.artifact(artifact.digest).text,
    "Complete evidence",
  );
  assert.equal(detail.sessions.length, 0);
});
test("request identity binds the target front and stale writes do not commit", (t) => {
  const { app } = fixture(t),
    a = front(app),
    b = front(app);
  const input = {
    text: "A decision",
    requestId: "stable",
    expectedRevision: 0,
  };
  app.entry(a.id, input);
  assert.equal(app.entry(a.id, input).text, "A decision");
  assert.throws(() => app.entry(b.id, input), /request_id_conflict/);
  assert.throws(
    () => app.entry(a.id, { ...input, requestId: "other" }),
    /stale_revision/,
  );
  assert.equal(app.store.list("entries", a.id).length, 1);
});
test("plans validate dependency graph and immutable approved revisions", (t) => {
  const { app } = fixture(t),
    f = front(app);
  assert.throws(
    () =>
      app.plan(f.id, {
        title: "Bad",
        waves: [{ id: "a", title: "A", criteria: ["Done"], dependsOn: ["a"] }],
        expectedRevision: 0,
        requestId: "bad",
      }),
    /dependency_cycle/,
  );
  const p = approved(app, f);
  assert.throws(
    () => app.approve(f.id, p.id, { expectedRevision: 2, requestId: "again" }),
    /plan_not_draft/,
  );
  assert.equal(app.store.front(f.id).revision, 2);
});
test("one wave binds one Bead and dispatch repeats do not create duplicates", async (t) => {
  const fake = provider(),
    { app } = fixture(t, { fake }),
    f = front(app),
    p = approved(app, f);
  const input = {
    agentId: "test-agent",
    expectedRevision: 2,
    requestId: "dispatch",
  };
  const op = await app.dispatch(f.id, p.waves[0].id, input);
  assert.equal(op.state, "confirmed");
  await app.dispatch(f.id, p.waves[0].id, input);
  assert.equal(fake.calls.filter((c) => c.method === "task.create").length, 1);
  assert.equal(fake.calls.filter((c) => c.method === "task.ready").length, 1);
  assert.equal(app.store.get("waves", p.waves[0].id).beadId, "demo-task");
  const second = (await app.detail(f.id)).waves[1];
  assert.equal(second.dependencies, "pending");
  assert.equal(second.allowedActions.includes("dispatch"), false);
});
test("lost effect response is unknown and is never blindly resubmitted", async (t) => {
  const fake = provider();
  let sends = 0;
  fake.call = async (project, method) => {
    if (method === "agents.list")
      return [{ id: "test-agent", available: true }];
    if (method === "worktrees.list") return [];
    if (method === "task.create") {
      sends++;
      throw new AppError("runtime_timeout", 503);
    }
    throw Error();
  };
  const { app } = fixture(t, { fake }),
    f = front(app),
    p = approved(app, f),
    input = {
      agentId: "test-agent",
      expectedRevision: 2,
      requestId: "uncertain",
    };
  assert.equal(
    (await app.dispatch(f.id, p.waves[0].id, input)).state,
    "unknown",
  );
  await app.dispatch(f.id, p.waves[0].id, input);
  assert.equal(sends, 1);
});
test("restart converts dispatched operations to unknown and retains payload", (t) => {
  const { app, root } = fixture(t),
    f = front(app);
  app.store.prepare("interrupted", "test", { target: "demo" }, f.id, 0, true);
  const op = app.store.operation("interrupted");
  op.state = "dispatched";
  app.store.putOperation(op);
  app.close();
  const reopened = new Store(root);
  t.after(() => reopened.close());
  assert.equal(reopened.operation("interrupted").state, "unknown");
  assert.deepEqual(reopened.operation("interrupted").payload, {
    target: "demo",
  });
});
test("disable ignores a late provider result and keeps history", async (t) => {
  let release;
  const fake = {
    call: () =>
      new Promise((resolve) => {
        release = resolve;
      }),
  };
  const { app } = fixture(t, { fake }),
    f = front(app);
  const pending = app.plugins.call("demo", "agents.list", {}, "read");
  app.config.toggle("engine", false);
  release([{ id: "agent" }]);
  await assert.rejects(pending, /connection_changed/);
  assert.equal(app.store.front(f.id).title, f.title);
  assert.equal(
    app.plugins.settings().connections.find((c) => c.id === "engine").health,
    "disabled",
  );
});
test("sessions are generic and require no wave, Bead or worktree", async (t) => {
  const fake = provider(),
    { app } = fixture(t, { fake }),
    f = front(app);
  const started = await app.startSession(f.id, {
    agentId: "test-agent",
    input: "Discuss the problem",
    requestId: "start",
  });
  assert.equal(started.result.agentId, "test-agent");
  await app.observeSession(f.id, started.result.id);
  await app.observeSession(f.id, started.result.id);
  const detail = await app.detail(f.id);
  assert.equal(detail.waves.length, 0);
  assert.equal(detail.entries.filter((e) => e.kind === "agent").length, 1);
  assert.equal(
    fake.calls.some((c) => c.method === "task.create"),
    false,
  );
});
test("document import rejects escaping symlinks and private files", (t) => {
  const { app, root } = fixture(t),
    f = front(app);
  fs.writeFileSync(path.join(root, ".env"), "sensitive");
  assert.throws(
    () =>
      app.importArtifact(f.id, {
        path: path.join(root, ".env"),
        requestId: "private",
      }),
    /private_document/,
  );
  const outside = fs.mkdtempSync(path.join(os.tmpdir(), "outside-"));
  t.after(() => fs.rmSync(outside, { recursive: true, force: true }));
  fs.writeFileSync(path.join(outside, "record.md"), "Outside");
  fs.symlinkSync(path.join(outside, "record.md"), path.join(root, "link.md"));
  assert.throws(
    () =>
      app.importArtifact(f.id, {
        path: path.join(root, "link.md"),
        requestId: "escape",
      }),
    /document_outside_roots/,
  );
});
test("backup restores database, configuration and verified artifact blobs", async (t) => {
  const { app, root } = fixture(t),
    f = front(app);
  const artifact = app.addArtifact(f.id, {
    name: "proof.md",
    text: "Evidence retained",
    requestId: "doc",
  });
  const result = await app.store.backup("snapshot");
  const target = path.join(root, "restored");
  const restored = restoreBackup(result.path, target);
  assert.equal(restored.fronts, 1);
  const recovered = new Store(target);
  t.after(() => recovered.close());
  assert.equal(recovered.artifact(artifact.digest).text, "Evidence retained");
  assert.ok(fs.existsSync(path.join(target, "connections.json")));
  assert.throws(
    () => restoreBackup(result.path, target),
    /restore_destination_exists/,
  );
});
test("HTTP enforces local origin, bounded JSON and structured errors", async (t) => {
  const { app } = fixture(t);
  const server = createServer(app);
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(() => new Promise((resolve) => server.close(resolve)));
  const url = `http://127.0.0.1:${server.address().port}`;
  assert.equal((await fetch(url + "/api/health")).status, 200);
  const blocked = await fetch(url + "/api/fronts", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Origin: "https://untrusted.invalid",
    },
    body: "{}",
  });
  assert.equal(blocked.status, 403);
  assert.equal((await blocked.json()).error.code, "invalid_origin");
  assert.equal(
    (
      await fetch(url + "/api/settings", {
        method: "PUT",
        headers: { "Content-Type": "text/plain" },
        body: "{}",
      })
    ).status,
    415,
  );
});

test("every planning and dependency state/event pair has an explicit result or rejection", async () => {
  const { MACHINES, transition } = await import("../server/fsm.mjs");
  for (const [name, machine] of Object.entries(MACHINES))
    for (const state of machine.states)
      for (const event of machine.events) {
        if (machine.transitions[state]?.[event])
          assert.equal(
            transition(name, state, event, {
              criteriaValid: true,
              dependenciesValid: true,
            }),
            machine.transitions[state][event],
          );
        else
          assert.throws(
            () => transition(name, state, event),
            /invalid_transition/,
          );
      }
  assert.throws(
    () => transition("planning", "future", "approve"),
    /unknown_state/,
  );
  assert.throws(
    () => transition("planning", "draft", "approve", { criteriaValid: false }),
    /criteria_required/,
  );
  assert.throws(
    () => transition("planning", "approved", "withdraw", { dispatched: true }),
    /active_wave_cannot_withdraw/,
  );
});
test("concurrent dispatch attempts with different IDs admit one operation", async (t) => {
  const fake = provider(),
    { app } = fixture(t, { fake }),
    f = front(app),
    p = approved(app, f);
  const results = await Promise.allSettled(
    ["first", "second"].map((requestId) =>
      app.dispatch(f.id, p.waves[0].id, {
        agentId: "test-agent",
        expectedRevision: 2,
        requestId,
      }),
    ),
  );
  assert.equal(results.filter((r) => r.status === "fulfilled").length, 1);
  assert.equal(fake.calls.filter((c) => c.method === "task.create").length, 1);
});
test("a new source works through registry registration without changing consumers", async (t) => {
  const { DEFINITIONS } = await import("../server/plugins.mjs");
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "source-test-"));
  const app = createApplication({
    stateRoot: root,
    definitions: [
      ...DEFINITIONS,
      {
        id: "alternate-source",
        name: "Alternate source",
        protocolVersion: 1,
        contracts: ["source"],
        operations: [
          "source.describe",
          "source.validateConfig",
          "source.test",
          "source.snapshot",
        ],
        capabilities: ["source.snapshot"],
        fields: [],
      },
    ],
    providers: {
      "alternate-source": {
        async readSnapshot() {
          return { records: [{ nativeRecordId: "one", label: "A record" }] };
        },
      },
    },
  });
  t.after(() => {
    app.close();
    fs.rmSync(root, { recursive: true, force: true });
  });
  app.config.save({
    version: 1,
    revision: 0,
    projects: [
      {
        id: "demo",
        name: "Demo",
        repo: root,
        documentRoots: [],
        executorId: null,
      },
    ],
    connections: [
      {
        id: "alternate",
        pluginId: "alternate-source",
        name: "Alternate",
        projectId: "demo",
        enabled: true,
        config: {},
      },
    ],
  });
  assert.equal(
    (await app.plugins.readSource("alternate")).snapshot.records[0]
      .nativeRecordId,
    "one",
  );
});
test("a future database version is rejected before schema changes", (t) => {
  const { app, root } = fixture(t);
  app.store.db
    .prepare("UPDATE metadata SET value=? WHERE key=?")
    .run("999", "schema");
  app.close();
  assert.throws(() => new Store(root), /unsupported_store_version/);
});

test("reconciliation resumes ready on the same Bead without repeating creation", async (t) => {
  const fake = provider();
  const base = fake.call;
  let ready = false;
  fake.call = async (project, method, params, id) => {
    if (method === "task.ready") {
      if (!ready) {
        ready = true;
        throw new AppError("runtime_timeout", 503);
      }
      return { id: params.id, ready: true };
    }
    if (method === "operation.get") {
      if (params.id.endsWith(":create"))
        return { state: "confirmed", result: { id: "demo-task" } };
      return { state: "confirmed", result: { id: "demo-task", ready: true } };
    }
    return base(project, method, params, id);
  };
  const { app } = fixture(t, { fake }),
    f = front(app),
    p = approved(app, f);
  const input = {
    agentId: "test-agent",
    expectedRevision: 2,
    requestId: "resume-ready",
  };
  assert.equal(
    (await app.dispatch(f.id, p.waves[0].id, input)).state,
    "unknown",
  );
  assert.equal((await app.reconcile(f.id, input.requestId)).state, "confirmed");
  assert.equal(fake.calls.filter((c) => c.method === "task.create").length, 1);
  assert.equal(app.store.get("waves", p.waves[0].id).beadId, "demo-task");
});

test("connection test requires the runtime owner, not only static capabilities", async (t) => {
  const fake = {
    async call(project, method) {
      if (method === "capabilities") return { version: 1 };
      throw new AppError("runtime_unavailable", 503);
    },
  };
  const { app } = fixture(t, { fake });
  const tested = await app.plugins.test("engine");
  assert.equal(tested.status, "unavailable");
  assert.equal(tested.error, "runtime_unavailable");
});
test("disabled session connection cannot publish a late start result", async (t) => {
  const fake = provider(),
    base = fake.call;
  let release, announced;
  const entered = new Promise((resolve) => {
    announced = resolve;
  });
  fake.call = async (...args) => {
    if (args[1] === "session.start") {
      announced();
      return new Promise((resolve) => {
        release = resolve;
      });
    }
    return base(...args);
  };
  const { app } = fixture(t, { fake }),
    f = front(app);
  const pending = app.startSession(f.id, {
    agentId: "test-agent",
    input: "Discuss",
    requestId: "late-session",
  });
  await entered;
  app.config.toggle("sessions", false);
  release({ sessionId: "provider-reference" });
  const op = await pending;
  assert.equal(op.state, "unknown");
  assert.equal(op.error, "session_connection_changed");
  assert.equal(app.store.list("sessions", f.id).length, 0);
});
test("context includes verified document text and explicit omissions within the wire budget", (t) => {
  const { app } = fixture(t),
    f = front(app);
  const small = app.addArtifact(f.id, {
    name: "selected.md",
    text: "Full immutable evidence",
    requestId: "small",
  });
  const large = app.addArtifact(f.id, {
    name: "large.md",
    text: "x".repeat(40000),
    requestId: "large",
  });
  const context = app.context(f.id);
  assert.equal(context.artifacts[0].text, "Full immutable evidence");
  assert.equal(context.artifacts[0].digest, small.digest);
  assert.equal(context.coverage.complete, false);
  assert.equal(context.coverage.omittedArtifacts[0].digest, large.digest);
  assert.ok(Buffer.byteLength(JSON.stringify(context)) < 49152);
  assert.throws(() => app.context(f.id, ["missing"]), /unknown_evidence/);
  const selected = app.context(f.id, [small.digest]);
  assert.equal(selected.coverage.complete, true);
  assert.equal(selected.artifacts.length, 1);
});
test("opaque provider sessions use independent Macross IDs", async (t) => {
  const fake = provider(),
    base = fake.call,
    opaque = "opaque/" + "x".repeat(300);
  fake.call = async (...args) =>
    args[1] === "session.start" ? { sessionId: opaque } : base(...args);
  const { app } = fixture(t, { fake }),
    f = front(app);
  const op = await app.startSession(f.id, {
    agentId: "test-agent",
    input: "Discuss",
    requestId: "opaque",
  });
  assert.equal(op.state, "confirmed");
  assert.match(op.result.id, /^[a-f0-9-]{36}$/);
  assert.equal(op.result.providerRef, opaque);
});
test("top-level cross-site navigation opens the app but API access stays blocked", async (t) => {
  const { request } = await import("node:http");
  const { app, root } = fixture(t);
  fs.writeFileSync(path.join(root, "index.html"), "<title>Macross</title>");
  const server = createServer(app, { dist: root });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(() => new Promise((resolve) => server.close(resolve)));
  const read = (pathname) =>
    new Promise((resolve, reject) => {
      const req = request(
        {
          host: "127.0.0.1",
          port: server.address().port,
          path: pathname,
          headers: {
            "Sec-Fetch-Site": "cross-site",
            "Sec-Fetch-Mode": "navigate",
          },
        },
        (res) => {
          res.resume();
          res.on("end", () => resolve(res.statusCode));
        },
      );
      req.on("error", reject);
      req.end();
    });
  assert.equal(await read("/"), 200);
  assert.equal(await read("/api/fronts"), 403);
});
test("adapter rejects oversized wire requests before spawning any process", async () => {
  const { OntrackAdapter } = await import("../server/plugins.mjs");
  let spawned = false;
  const adapter = new OntrackAdapter({
    spawnProcess() {
      spawned = true;
      throw Error();
    },
  });
  assert.throws(
    () =>
      adapter.call(
        { repo: "." },
        "session.start",
        { input: "x".repeat(70000) },
        "test",
      ),
    /invalid_input/,
  );
  assert.equal(spawned, false);
});

test("pre-dispatch withdrawal works offline, is idempotent and invalidates dependencies", async (t) => {
  const { app } = fixture(t),
    f = front(app),
    p = approved(app, f);
  const before = await app.detail(f.id);
  assert.ok(before.waves[0].allowedActions.includes("withdraw"));
  const input = { requestId: "withdraw", expectedRevision: 2 };
  assert.equal(
    app.withdrawWave(f.id, p.waves[0].id, input).planning,
    "withdrawn",
  );
  assert.equal(
    app.withdrawWave(f.id, p.waves[0].id, input).planning,
    "withdrawn",
  );
  const after = await app.detail(f.id);
  assert.equal(after.waves[0].allowedActions.includes("withdraw"), false);
  assert.equal(after.waves[1].dependencies, "invalid");
  assert.equal(after.waves[1].allowedActions.includes("dispatch"), false);
  assert.equal(app.store.get("waves", p.waves[0].id).beadId, null);
});
test("withdrawal requires current revision and rejects admitted or uncertain waves", async (t) => {
  const fake = provider(),
    { app } = fixture(t, { fake }),
    f = front(app),
    p = approved(app, f);
  assert.throws(
    () =>
      app.withdrawWave(f.id, p.waves[0].id, {
        requestId: "stale-withdraw",
        expectedRevision: 1,
      }),
    /stale_revision/,
  );
  assert.throws(
    () =>
      app.withdrawWave(f.id, p.waves[0].id, { requestId: "missing-revision" }),
    /expected_revision_required/,
  );
  await app.dispatch(f.id, p.waves[0].id, {
    agentId: "test-agent",
    expectedRevision: 2,
    requestId: "admitted",
  });
  assert.throws(
    () =>
      app.withdrawWave(f.id, p.waves[0].id, {
        requestId: "withdraw-admitted",
        expectedRevision: 3,
      }),
    /wave_already_admitted/,
  );
  assert.equal(app.store.front(f.id).revision, 3);
  const second = app.store.get("waves", p.waves[1].id);
  second.operationId = "uncertain-admission";
  app.store.put("waves", second);
  app.store.putOperation({ id: second.operationId, state: "unknown" });
  assert.throws(
    () =>
      app.withdrawWave(f.id, second.id, {
        requestId: "withdraw-uncertain",
        expectedRevision: 3,
      }),
    /wave_already_admitted/,
  );
});

test("confirmed session start and send recover canonical links after lost responses", async (t) => {
  const fake = provider(),
    base = fake.call;
  let starts = 0,
    sends = 0;
  fake.call = async (project, method, params, id) => {
    if (method === "session.start") {
      starts++;
      throw new AppError("runtime_timeout", 503);
    }
    if (method === "session.send") {
      sends++;
      throw new AppError("runtime_timeout", 503);
    }
    if (method === "operation.get")
      return params.id === "start-lost"
        ? { state: "confirmed", result: { sessionId: "native-session" } }
        : { state: "confirmed", result: { status: "pending" } };
    return base(project, method, params, id);
  };
  const { app } = fixture(t, { fake }),
    f = front(app);
  app.addArtifact(f.id, {
    name: "context.md",
    text: "Pinned source text",
    requestId: "context",
  });
  const start = {
    agentId: "test-agent",
    input: "Discuss carefully",
    requestId: "start-lost",
  };
  assert.equal((await app.startSession(f.id, start)).state, "unknown");
  assert.equal(app.store.list("sessions", f.id).length, 0);
  assert.equal(
    app.store.operation(start.requestId).payload.context.artifacts[0].text,
    "Pinned source text",
  );
  const resolved = await app.reconcile(f.id, start.requestId);
  assert.equal(resolved.state, "confirmed");
  const session = resolved.result;
  assert.equal(session.providerRef, "native-session");
  assert.equal(
    app.store
      .list("entries", f.id)
      .filter((e) => e.requestId === start.requestId).length,
    1,
  );
  assert.equal((await app.startSession(f.id, start)).state, "confirmed");
  assert.equal(starts, 1);
  const send = { text: "A follow-up", requestId: "send-lost" };
  assert.equal(
    (await app.sendSession(f.id, session.id, send)).state,
    "unknown",
  );
  assert.equal((await app.reconcile(f.id, send.requestId)).state, "confirmed");
  assert.equal(
    app.store
      .list("entries", f.id)
      .filter((e) => e.requestId === send.requestId).length,
    1,
  );
  await app.sendSession(f.id, session.id, send);
  assert.equal(sends, 1);
});
test("a guaranteed pre-admission failure permits a new attempt while keeping its ledger", async (t) => {
  const fake = provider(),
    base = fake.call;
  let attempts = 0;
  fake.call = async (...args) => {
    if (args[1] === "task.create" && attempts++ === 0)
      throw new AppError("runtime_unavailable", 503);
    return base(...args);
  };
  const { app } = fixture(t, { fake }),
    f = front(app),
    p = approved(app, f);
  const failed = await app.dispatch(f.id, p.waves[0].id, {
    agentId: "test-agent",
    expectedRevision: 2,
    requestId: "not-admitted",
  });
  assert.equal(failed.state, "failed");
  assert.equal(app.store.get("waves", p.waves[0].id).beadId, null);
  assert.ok(
    (await app.detail(f.id)).waves[0].allowedActions.includes("dispatch"),
  );
  const retried = await app.dispatch(f.id, p.waves[0].id, {
    agentId: "test-agent",
    expectedRevision: 3,
    requestId: "retry-admission",
  });
  assert.equal(retried.state, "confirmed");
  assert.equal(app.store.operation("not-admitted").state, "failed");
  assert.equal(fake.calls.filter((c) => c.method === "task.create").length, 1);
});
test("agent answers retain all permitted bytes and reject oversized output explicitly", async (t) => {
  const fake = provider(),
    base = fake.call;
  let answer = "A".repeat(60000);
  fake.call = async (...args) =>
    args[1] === "session.observe" ? { text: answer } : base(...args);
  const { app } = fixture(t, { fake }),
    f = front(app),
    started = await app.startSession(f.id, {
      agentId: "test-agent",
      input: "Discuss",
      requestId: "long-answer",
    });
  await app.observeSession(f.id, started.result.id);
  assert.equal(
    app.store.list("entries", f.id).find((e) => e.kind === "agent").text.length,
    60000,
  );
  answer = "x".repeat(65537);
  await assert.rejects(
    () => app.observeSession(f.id, started.result.id),
    /agent_response_limit/,
  );
  assert.equal(
    app.store.list("entries", f.id).filter((e) => e.kind === "agent").length,
    1,
  );
});
test("delivery closure requires current engine acceptance and integration and targets the same Bead", async (t) => {
  const fake = provider(),
    base = fake.call;
  let accepted = false,
    closed = false;
  fake.call = async (...args) => {
    if (args[1] === "worktrees.list")
      return [
        {
          id: "handoff",
          bead_id: "demo-task",
          status: "integrated",
          execution: "succeeded",
          integration: "integrated",
          resultRevision: "a".repeat(40),
          review: {
            state: accepted ? "accepted" : "pending",
            validity: "current",
          },
        },
      ];
    if (args[1] === "board.list")
      return [
        {
          record: {
            id: "demo-task",
            status: closed ? "closed" : "in_progress",
          },
        },
      ];
    if (args[1] === "delivery.close") {
      assert.equal(args[2].id, "demo-task");
      closed = true;
      return { id: "demo-task", closed: true };
    }
    return base(...args);
  };
  const { app } = fixture(t, { fake }),
    f = front(app),
    p = approved(app, f);
  await app.dispatch(f.id, p.waves[0].id, {
    agentId: "test-agent",
    expectedRevision: 2,
    requestId: "admit-close",
  });
  await assert.rejects(
    () => app.closeWave(f.id, p.waves[0].id, { requestId: "refuse-close" }),
    /delivery_not_accepted_and_integrated/,
  );
  accepted = true;
  assert.ok((await app.detail(f.id)).waves[0].allowedActions.includes("close"));
  const op = await app.closeWave(f.id, p.waves[0].id, {
    requestId: "close-delivery",
  });
  assert.equal(op.result.closed, true);
  const after = await app.detail(f.id);
  assert.equal(after.waves[0].closed, true);
  assert.equal(after.waves[0].allowedActions.includes("close"), false);
});
test("unadmitted offline waves remain not started and not integrated", async (t) => {
  const { app } = fixture(t),
    f = front(app);
  approved(app, f);
  const detail = await app.detail(f.id);
  for (const wave of detail.waves) {
    assert.equal(wave.execution, "not_started");
    assert.equal(wave.integration, "not_integrated");
    assert.equal(wave.sourceStatus, "unavailable");
  }
});

test("historical accepted receipts with unknown validity cannot deliver, close or release dependencies", async (t) => {
  const fake = provider(),
    base = fake.call;
  let validity = "unknown";
  fake.call = async (...args) => {
    if (args[1] === "worktrees.list")
      return [
        {
          id: "handoff",
          bead_id: "demo-task",
          status: "integrated",
          execution: "succeeded",
          integration: "integrated",
          resultRevision: "a".repeat(40),
          review: { state: "accepted", validity },
        },
      ];
    return base(...args);
  };
  const { app } = fixture(t, { fake }),
    f = front(app),
    p = approved(app, f);
  await app.dispatch(f.id, p.waves[0].id, {
    agentId: "test-agent",
    expectedRevision: 2,
    requestId: "admit-validity",
  });
  const uncertain = await app.detail(f.id);
  assert.equal(uncertain.waves[0].review, "accepted");
  assert.equal(uncertain.waves[0].reviewValidity, "unknown");
  assert.equal(uncertain.waves[0].delivered, false);
  assert.equal(uncertain.waves[0].allowedActions.includes("close"), false);
  assert.equal(uncertain.waves[1].dependencies, "pending");
  assert.equal(uncertain.waves[1].allowedActions.includes("dispatch"), false);
  await assert.rejects(
    () => app.closeWave(f.id, p.waves[0].id, { requestId: "no-current-close" }),
    /delivery_not_accepted_and_integrated/,
  );
  validity = "current";
  const current = await app.detail(f.id);
  assert.equal(current.waves[0].delivered, true);
  assert.equal(current.waves[0].reviewValidity, "current");
  assert.equal(current.waves[1].dependencies, "satisfied");
});
test("legacy accepted strings are historical, not current acceptance proof", async () => {
  const { dependencyState } = await import("../server/fsm.mjs");
  assert.equal(
    dependencyState([
      {
        review: "accepted",
        integration: "integrated",
        reviewValidity: "unknown",
      },
    ]),
    "pending",
  );
  assert.equal(
    dependencyState([{ review: "accepted", integration: "integrated" }]),
    "pending",
  );
  assert.equal(
    dependencyState([
      {
        review: "accepted",
        integration: "integrated",
        reviewValidity: "current",
      },
    ]),
    "satisfied",
  );
});

test("active retry replaces historical accepted attempt for the same Bead", async (t) => {
  const fake = provider(),
    base = fake.call;
  fake.call = async (...args) =>
    args[1] === "worktrees.list"
      ? [
          {
            id: "old-attempt",
            bead_id: "demo-task",
            active: false,
            status: "integrated",
            execution: "succeeded",
            integration: "integrated",
            resultRevision: "a".repeat(40),
            review: { state: "accepted", validity: "current" },
          },
          {
            id: "new-attempt",
            bead_id: "demo-task",
            active: true,
            status: "in_progress",
            execution: "running",
            integration: "not_integrated",
            resultRevision: "b".repeat(40),
            review: { state: "pending", validity: "unknown" },
          },
        ]
      : base(...args);
  const { app } = fixture(t, { fake }),
    f = front(app),
    p = approved(app, f);
  await app.dispatch(f.id, p.waves[0].id, {
    agentId: "test-agent",
    expectedRevision: 2,
    requestId: "admit-retry",
  });
  const detail = await app.detail(f.id),
    wave = detail.waves[0];
  assert.equal(wave.runId, "new-attempt");
  assert.equal(wave.execution, "running");
  assert.equal(wave.review, "pending");
  assert.equal(wave.delivered, false);
  assert.equal(wave.allowedActions.includes("close"), false);
  assert.equal(detail.waves[1].dependencies, "pending");
});
test("ambiguous active runs cannot reuse an accepted historical receipt", async (t) => {
  const fake = provider(),
    base = fake.call;
  fake.call = async (...args) =>
    args[1] === "worktrees.list"
      ? ["first", "second"].map((id) => ({
          id,
          bead_id: "demo-task",
          active: true,
          status: "integrated",
          execution: "succeeded",
          integration: "integrated",
          resultRevision: "a".repeat(40),
          review: { state: "accepted", validity: "current" },
        }))
      : base(...args);
  const { app } = fixture(t, { fake }),
    f = front(app),
    p = approved(app, f);
  await app.dispatch(f.id, p.waves[0].id, {
    agentId: "test-agent",
    expectedRevision: 2,
    requestId: "admit-ambiguous",
  });
  const wave = (await app.detail(f.id)).waves[0];
  assert.equal(wave.runId, null);
  assert.equal(wave.execution, "unknown");
  assert.equal(wave.integration, "unknown");
  assert.equal(wave.delivered, false);
  assert.ok(wave.blockReasons.includes("ambiguous_active_run"));
  assert.equal(wave.allowedActions.includes("review"), false);
  assert.equal(wave.allowedActions.includes("close"), false);
});

test("host runtime environment adds the local binary directory without changing PATH precedence", async () => {
  const { runtimeEnvironment, OntrackAdapter } =
    await import("../server/plugins.mjs");
  const localBin = path.join(os.homedir(), ".local", "bin");
  const env = runtimeEnvironment({
    PATH: "/host/tools:/usr/bin",
    EXAMPLE_VALUE: "kept",
  });
  assert.equal(
    env.PATH,
    ["/host/tools", "/usr/bin", localBin].join(path.delimiter),
  );
  assert.equal(env.EXAMPLE_VALUE, "kept");
  assert.equal(runtimeEnvironment({ PATH: localBin }).PATH, localBin);
  const { EventEmitter } = await import("node:events"),
    { PassThrough } = await import("node:stream");
  const calls = [];
  const adapter = new OntrackAdapter({
    binary: "/host/ontrack",
    spawnProcess(program, args, options) {
      calls.push({ program, args, options });
      const child = new EventEmitter();
      child.stdin = new PassThrough();
      child.stdout = new PassThrough();
      child.stderr = new PassThrough();
      child.unref = () => {};
      child.kill = () => {};
      if (args[0] === "api")
        process.nextTick(() => {
          child.stdout.write(
            JSON.stringify({
              version: 1,
              id: "read",
              result: { state: "running" },
            }),
          );
          child.emit("close", 0);
        });
      return child;
    },
  });
  await adapter.call({ repo: "/host/project" }, "runtime.status", {}, "read");
  adapter.start({ repo: "/host/project" });
  assert.equal(calls.length, 2);
  for (const call of calls) {
    assert.equal(call.program, "/host/ontrack");
    assert.equal(call.options.shell, false);
    assert.ok(call.options.env.PATH.split(path.delimiter).includes(localBin));
    assert.ok(Array.isArray(call.args));
    assert.equal(call.options.cwd, "/host/project");
  }
  assert.deepEqual(calls[0].args, ["api"]);
  assert.deepEqual(calls[1].args, ["serve"]);
});

test("integration requires a current accepted result and never closes its Bead", async (t) => {
  const fake = provider(),
    base = fake.call;
  let accepted = false,
    integrations = 0;
  fake.call = async (...args) => {
    if (args[1] === "worktrees.list")
      return [
        {
          id: "handoff",
          bead_id: "demo-task",
          execution: "succeeded",
          integration: "not_integrated",
          resultRevision: "a".repeat(40),
          review: {
            state: accepted ? "accepted" : "pending",
            validity: "current",
          },
        },
      ];
    if (args[1] === "worktree.integrate") {
      integrations++;
      assert.equal(args[2].id, "handoff");
      return { requested: true };
    }
    if (args[1] === "delivery.close")
      assert.fail("Integration must not close a Bead");
    return base(...args);
  };
  const { app } = fixture(t, { fake }),
    f = front(app),
    p = approved(app, f),
    wave = p.waves[0];
  await app.dispatch(f.id, wave.id, {
    agentId: "test-agent",
    expectedRevision: 2,
    requestId: "admit-integrate",
  });
  await assert.rejects(
    () =>
      app.integrateWave(f.id, wave.id, {
        resultRevision: "a".repeat(40),
        requestId: "not-accepted",
      }),
    /result_not_integrable/,
  );
  accepted = true;
  await assert.rejects(
    () =>
      app.integrateWave(f.id, wave.id, {
        resultRevision: "b".repeat(40),
        requestId: "stale-integration",
      }),
    /stale_result/,
  );
  const input = {
    resultRevision: "a".repeat(40),
    requestId: "integrate-result",
  };
  assert.equal(
    (await app.integrateWave(f.id, wave.id, input)).state,
    "confirmed",
  );
  await app.integrateWave(f.id, wave.id, input);
  assert.equal(integrations, 1);
});

test("connector protocol versions are explicit and incompatible registries preserve configuration", async (t) => {
  const { DEFINITIONS, Plugins } = await import("../server/plugins.mjs");
  const { Configuration } = await import("../server/config.mjs");
  const { CONNECTOR_CONTRACT, validateDefinitions, requireConnectorOperation } =
    await import("../server/plugin-contracts.mjs");
  const { app, root } = fixture(t),
    before = fs.readFileSync(path.join(root, "connections.json"), "utf8");
  assert.ok(
    app.plugins.definitions().every((def) => def.protocolVersion === 1),
  );
  for (const descriptor of DEFINITIONS) {
    for (const contract of descriptor.contracts)
      for (const operation of CONNECTOR_CONTRACT.requiredOperations[contract])
        assert.ok(descriptor.operations.includes(operation));
  }
  const incompatible = DEFINITIONS.map((def) => ({
    ...def,
    protocolVersion: 2,
  }));
  assert.throws(
    () => new Configuration(root, incompatible),
    /incompatible_plugin_version/,
  );
  assert.throws(
    () => new Plugins(app.config, { definitions: incompatible }),
    /incompatible_plugin_version/,
  );
  assert.equal(
    fs.readFileSync(path.join(root, "connections.json"), "utf8"),
    before,
  );
  assert.throws(
    () => validateDefinitions([{ ...DEFINITIONS[0], operations: [] }]),
    /plugin_operation_missing/,
  );
  assert.throws(
    () => requireConnectorOperation(DEFINITIONS[0], "session.interrupt"),
    /capability_unavailable/,
  );
  assert.throws(
    () => requireConnectorOperation(DEFINITIONS[0], "session.resume"),
    /capability_unavailable/,
  );
  assert.throws(
    () => requireConnectorOperation(DEFINITIONS[0], "intelligence.propose"),
    /capability_unavailable/,
  );
  assert.equal(
    app.plugins
      .definitions()
      .some((def) => def.contracts.includes("intelligence")),
    false,
  );
});

test("concurrent front observations share one native read without a stale completed cache", async (t) => {
  const fake = provider(),
    base = fake.call;
  let release;
  const wait = new Promise((resolve) => (release = resolve));
  let reads = 0;
  fake.call = async (...args) => {
    if (args[1] === "worktrees.list") {
      reads++;
      await wait;
      return [];
    }
    return base(...args);
  };
  const { app } = fixture(t, { fake }),
    f = front(app);
  approved(app, f);
  const first = app.detail(f.id),
    second = app.detail(f.id);
  await Promise.resolve();
  assert.equal(reads, 1);
  release();
  await Promise.all([first, second]);
  await app.detail(f.id);
  assert.equal(reads, 2);
});

test("conversation-only fronts do not wait for an execution backend", async (t) => {
  const fake = provider();
  fake.call = async () => {
    assert.fail("No native observation is needed before a plan exists");
  };
  const { app } = fixture(t, { fake }),
    f = front(app);
  const detail = await app.detail(f.id);
  assert.equal(detail.front.id, f.id);
  assert.deepEqual(detail.waves, []);
});

test("switching test agent providers preserves the front and carries selected canonical context", async (t) => {
  const fake = provider(),
    base = fake.call,
    starts = [];
  fake.call = async (...args) => {
    const [, method, params] = args;
    if (method === "agents.list")
      return [
        { id: "provider-alpha", available: true },
        { id: "provider-beta", available: true },
      ];
    if (method === "session.start") {
      starts.push(params);
      return { sessionId: `opaque-${params.agentId}` };
    }
    if (method === "session.observe")
      return { text: `Answer from ${params.sessionId}` };
    return base(...args);
  };
  const { app } = fixture(t, { fake }),
    f = front(app);
  const first = await app.startSession(f.id, {
    agentId: "provider-alpha",
    input: "Discuss first",
    requestId: "first-provider",
  });
  await app.observeSession(f.id, first.result.id);
  const second = await app.startSession(f.id, {
    agentId: "provider-beta",
    input: "Continue with the selected context",
    requestId: "second-provider",
  });
  await app.observeSession(f.id, second.result.id);
  const detail = await app.detail(f.id);
  assert.equal(detail.front.id, f.id);
  assert.equal(detail.sessions.length, 2);
  assert.notEqual(first.result.id, second.result.id);
  assert.notEqual(first.result.providerRef, second.result.providerRef);
  assert.ok(
    starts[1].context.entries.some(
      (entry) =>
        entry.kind === "agent" &&
        entry.text === "Answer from opaque-provider-alpha",
    ),
  );
  assert.equal(
    detail.entries.filter((entry) => entry.kind === "agent").length,
    2,
  );
  assert.equal(detail.waves.length, 0);
  assert.ok(!fake.calls.some((call) => call.method.startsWith("task.")));
});
