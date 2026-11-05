import path from "node:path";
import { randomUUID } from "node:crypto";
import fs from "node:fs";
import { Store, digest } from "./store.mjs";
import { Configuration } from "./config.mjs";
import { Plugins, DEFINITIONS, OntrackAdapter } from "./plugins.mjs";
import { transition, dependencyState } from "./fsm.mjs";
import { AppError, check, text, identifier, object } from "./errors.mjs";

const now = () => new Date().toISOString();
function request(input) {
  return identifier(input.requestId);
}
function checkGraph(waves) {
  check(
    Array.isArray(waves) && waves.length > 0 && waves.length <= 100,
    "invalid_waves",
  );
  const ids = new Set();
  for (const wave of waves) {
    object(wave, ["id", "title", "criteria", "dependsOn"]);
    identifier(wave.id);
    check(wave.id.length <= 64, "wave_id_too_long");
    check(!ids.has(wave.id), "duplicate_wave");
    ids.add(wave.id);
    text(wave.title, 200);
    check(
      Array.isArray(wave.criteria) &&
        wave.criteria.length > 0 &&
        wave.criteria.length <= 30,
      "criteria_required",
    );
    wave.criteria.forEach((c) => text(c, 1000));
    check(
      Array.isArray(wave.dependsOn) && wave.dependsOn.length <= 100,
      "invalid_dependencies",
    );
  }
  const visited = new Set(),
    visiting = new Set();
  const visit = (id) => {
    check(ids.has(id), "unknown_dependency");
    check(!visiting.has(id), "dependency_cycle");
    if (visited.has(id)) return;
    visiting.add(id);
    for (const dep of waves.find((w) => w.id === id).dependsOn) visit(dep);
    visiting.delete(id);
    visited.add(id);
  };
  waves.forEach((w) => visit(w.id));
}

export class Application {
  constructor({
    stateRoot = path.resolve("state"),
    ontrack,
    providers = {},
    definitions = DEFINITIONS,
  } = {}) {
    this.store = new Store(stateRoot);
    this.config = new Configuration(this.store.root, definitions);
    this.plugins = new Plugins(this.config, {
      ontrack: ontrack || new OntrackAdapter(),
      providers,
      definitions,
    });
    this.observationReads = new Map();
  }
  close() {
    this.store.close();
  }
  health() {
    return {
      service: "macross-ontrack",
      version: 1,
      stateRoot: this.store.root,
    };
  }
  fronts() {
    return this.store.fronts();
  }
  createFront(input) {
    object(input, ["title", "objective", "projectId", "requestId"]);
    const title = text(input.title, 200),
      objective = text(input.objective, 4000);
    this.config.project(input.projectId);
    return this.store.local(
      request(input),
      "front.create",
      input,
      null,
      undefined,
      () => {
        const front = {
          id: randomUUID(),
          title,
          objective,
          projectId: input.projectId,
          revision: 0,
          createdAt: now(),
          updatedAt: now(),
        };
        this.store.put("fronts", front);
        return front;
      },
    );
  }
  entry(frontId, input) {
    object(input, ["text", "kind", "expectedRevision", "requestId"]);
    check(
      Number.isSafeInteger(input.expectedRevision),
      "expected_revision_required",
    );
    const kind = input.kind || "note";
    check(["note", "decision", "summary"].includes(kind), "invalid_entry_kind");
    const content = text(input.text, 32000);
    return this.store.local(
      request(input),
      "entry.append",
      input,
      frontId,
      input.expectedRevision,
      () => {
        const entry = {
          id: randomUUID(),
          frontId,
          kind,
          text: content,
          at: now(),
          requestId: input.requestId,
        };
        this.store.put("entries", entry);
        return entry;
      },
    );
  }
  plan(frontId, input) {
    object(input, ["title", "waves", "expectedRevision", "requestId"]);
    check(
      Number.isSafeInteger(input.expectedRevision),
      "expected_revision_required",
    );
    checkGraph(input.waves);
    const title = text(input.title, 200);
    return this.store.local(
      request(input),
      "plan.create",
      input,
      frontId,
      input.expectedRevision,
      () => {
        const prior = this.store.list("plans", frontId);
        const plan = {
          id: randomUUID(),
          frontId,
          title,
          revision: prior.length + 1,
          status: "draft",
          createdAt: now(),
        };
        this.store.put("plans", plan);
        const ids = new Map(
          input.waves.map((w) => [w.id, `${plan.id}:${w.id}`]),
        );
        for (const item of input.waves) {
          const wave = {
            id: ids.get(item.id),
            localId: item.id,
            frontId,
            planId: plan.id,
            title: item.title,
            criteria: item.criteria,
            dependsOn: item.dependsOn.map((id) => ids.get(id)),
            planning: "draft",
            beadId: null,
            operationId: null,
          };
          this.store.put("waves", wave);
        }
        return {
          ...plan,
          waves: this.store
            .list("waves", frontId)
            .filter((w) => w.planId === plan.id),
        };
      },
    );
  }
  approve(frontId, planId, input) {
    object(input, ["expectedRevision", "requestId"]);
    check(
      Number.isSafeInteger(input.expectedRevision),
      "expected_revision_required",
    );
    return this.store.local(
      request(input),
      "plan.approve",
      { ...input, planId },
      frontId,
      input.expectedRevision,
      () => {
        const plan = this.store.get("plans", planId);
        check(plan.frontId === frontId, "wrong_front", 409);
        check(plan.status === "draft", "plan_not_draft", 409);
        const waves = this.store
          .list("waves", frontId)
          .filter((w) => w.planId === planId);
        checkGraph(
          waves.map((w) => ({
            id: w.localId,
            title: w.title,
            criteria: w.criteria,
            dependsOn: w.dependsOn.map(
              (dep) => waves.find((other) => other.id === dep)?.localId || dep,
            ),
          })),
        );
        plan.status = transition("planning", plan.status, "approve", {
          criteriaValid: true,
          dependenciesValid: true,
        });
        plan.approvedAt = now();
        this.store.put("plans", plan);
        for (const wave of waves) {
          wave.planning = "approved";
          this.store.put("waves", wave);
        }
        return plan;
      },
    );
  }
  async observations(front) {
    if (this.observationReads.has(front.projectId))
      return this.observationReads.get(front.projectId);
    const read = (async () => {
      try {
        const result = await this.plugins.call(
          front.projectId,
          "worktrees.list",
          {},
          "worktree-snapshot",
        );
        let agents = [],
          tasks = [];
        try {
          agents = await this.plugins.agents(front.projectId);
        } catch {}
        try {
          const board = await this.plugins.call(
            front.projectId,
            "board.list",
            {},
            "board-snapshot",
          );
          tasks = Array.isArray(board) ? board : board.entries || [];
        } catch {}
        return {
          available: true,
          agents,
          tasks,
          runs: Array.isArray(result)
            ? result
            : result.worktrees || result.runs || [],
        };
      } catch {
        return { available: false, runs: [] };
      }
    })();
    this.observationReads.set(front.projectId, read);
    try {
      return await read;
    } finally {
      if (this.observationReads.get(front.projectId) === read)
        this.observationReads.delete(front.projectId);
    }
  }
  projectWaves(front, observed) {
    const rows = this.store.list("waves", front.id);
    const projected = rows.map((wave) => {
      const candidates = wave.beadId
        ? observed.runs.filter(
            (run) =>
              (run.bead_id === wave.beadId || run.beadId === wave.beadId) &&
              run.active !== false,
          )
        : [];
      const run = candidates.length === 1 ? candidates[0] : null;
      const runAmbiguous = candidates.length > 1;
      const native = run?.status || run?.status_code || null;
      const task = wave.beadId
        ? observed.tasks?.find(
            (item) => (item.record?.id || item.id) === wave.beadId,
          )
        : null;
      const execution =
        run?.execution ||
        {
          in_progress: "running",
          reserved: "queued",
          blocked: "blocked",
          unmerged: "succeeded",
          integrated: "succeeded",
          needs_attention: "blocked",
          needs_resolution: "blocked",
          dirty_checkout: "blocked",
        }[native] ||
        (wave.beadId ? "unknown" : "not_started");
      const review =
        typeof run?.review === "string"
          ? run.review
          : run?.review?.state || "pending";
      const reviewValidity =
        run?.review &&
        typeof run.review === "object" &&
        run.review.validity === "current"
          ? "current"
          : "unknown";
      const integration = !wave.beadId
        ? "not_integrated"
        : runAmbiguous
          ? "unknown"
          : run?.integration ||
            (["integrated", "workspace_closed", "checkout_removed"].includes(
              native,
            )
              ? "integrated"
              : native === "needs_resolution"
                ? "conflict"
                : observed.available
                  ? "not_integrated"
                  : "unknown");
      return {
        ...wave,
        execution,
        review,
        reviewValidity,
        integration,
        resultRevision: run?.resultRevision || run?.result_revision || null,
        runId: run?.id || null,
        native: run || null,
        closed: task ? (task.record?.status || task.status) === "closed" : null,
        sourceStatus: observed.available ? "available" : "unavailable",
        allowedActions: [],
        blockReasons: runAmbiguous ? ["ambiguous_active_run"] : [],
      };
    });
    const byId = new Map(projected.map((w) => [w.id, w]));
    for (const wave of projected) {
      const deps = wave.dependsOn.map((id) => byId.get(id));
      wave.dependencies = dependencyState(deps);
      if (wave.planning !== "approved")
        wave.blockReasons.push("plan_not_approved");
      if (wave.dependencies !== "satisfied")
        wave.blockReasons.push("dependencies_not_satisfied");
      if (wave.beadId) wave.blockReasons.push("already_admitted");
      if (
        wave.operationId &&
        this.store.operation(wave.operationId)?.state !== "failed"
      )
        wave.blockReasons.push("admission_already_recorded");
      try {
        this.plugins.executor(front.projectId);
      } catch {
        wave.blockReasons.push("executor_unavailable");
      }
      if (!observed.available)
        wave.blockReasons.push("executor_observation_unavailable");
      if (!observed.agents?.some((agent) => agent.available !== false))
        wave.blockReasons.push("no_available_agent");
      const pending = this.store.operation(wave.operationId || "none");
      if (pending && ["dispatched", "unknown"].includes(pending.state))
        wave.blockReasons.push("operation_requires_reconciliation");
      if (!wave.blockReasons.length) wave.allowedActions.push("dispatch");
      if (wave.runId && ["succeeded", "blocked"].includes(wave.execution))
        wave.allowedActions.push("continue");
      if (wave.runId && wave.resultRevision) wave.allowedActions.push("review");
      if (
        ["draft", "approved"].includes(wave.planning) &&
        !wave.beadId &&
        (!pending || pending.state === "failed")
      )
        wave.allowedActions.push("withdraw");
      wave.delivered =
        wave.review === "accepted" &&
        wave.reviewValidity === "current" &&
        wave.integration === "integrated";
      if (
        wave.review === "accepted" &&
        wave.reviewValidity === "current" &&
        wave.execution === "succeeded" &&
        wave.integration === "not_integrated" &&
        wave.runId &&
        wave.resultRevision
      )
        wave.allowedActions.push("integrate");
      if (wave.delivered && wave.closed !== true)
        wave.allowedActions.push("close");
    }
    return projected;
  }
  async detail(id) {
    const front = this.store.front(id),
      needsExecution = this.store
        .list("waves", id)
        .some((wave) => wave.beadId || wave.planning === "approved"),
      observed = needsExecution
        ? await this.observations(front)
        : { available: false, runs: [] },
      waves = this.projectWaves(front, observed);
    return {
      front,
      entries: this.store.list("entries", id),
      plans: this.store
        .list("plans", id)
        .map((p) => ({ ...p, waves: waves.filter((w) => w.planId === p.id) })),
      waves,
      artifacts: this.store.list("artifacts", id),
      sessions: this.store.list("sessions", id),
      operations: this.store.db
        .prepare("SELECT data FROM operations")
        .all()
        .map((row) => JSON.parse(row.data))
        .filter((op) => op.frontId === id),
    };
  }
  context(frontId, selectedArtifacts) {
    if (selectedArtifacts !== undefined)
      check(
        Array.isArray(selectedArtifacts) && selectedArtifacts.length <= 100,
        "invalid_evidence_selection",
      );
    const front = this.store.front(frontId),
      all = this.store.list("artifacts", frontId),
      selected =
        selectedArtifacts === undefined
          ? all
          : all.filter((a) => selectedArtifacts.includes(a.digest));
    if (selectedArtifacts !== undefined) {
      check(
        Array.isArray(selectedArtifacts) && selectedArtifacts.length <= 100,
        "invalid_evidence_selection",
      );
      check(
        selected.length === new Set(selectedArtifacts).size,
        "unknown_evidence",
        404,
      );
    }
    // The wire protocol is bounded to 64 KiB. Reserve room for user input and the envelope.
    const context = {
      front: {
        id: front.id,
        title: front.title,
        objective: front.objective,
        revision: front.revision,
      },
      entries: [],
      artifacts: [],
      plans: this.store
        .list("plans", frontId)
        .slice(-5)
        .map((p) => ({
          id: p.id,
          revision: p.revision,
          status: p.status,
          title: p.title,
        })),
      coverage: { complete: true, omittedEntries: 0, omittedArtifacts: [] },
    };
    const fits = () => Buffer.byteLength(JSON.stringify(context)) <= 32768;
    for (const entry of this.store.list("entries", frontId).slice(-30)) {
      context.entries.push(entry);
      if (!fits()) {
        context.entries.pop();
        context.coverage.omittedEntries++;
        context.coverage.complete = false;
      }
    }
    for (const artifact of selected) {
      const record = this.store.artifact(artifact.digest);
      context.artifacts.push({
        digest: record.digest,
        name: record.name,
        text: record.text,
      });
      if (!fits()) {
        context.artifacts.pop();
        context.coverage.omittedArtifacts.push({
          digest: record.digest,
          name: record.name,
          reason: "context_byte_limit",
        });
        context.coverage.complete = false;
      }
    }
    // All omissions remain explicit, including history outside the selected recent window.
    const older = Math.max(0, this.store.list("entries", frontId).length - 30);
    context.coverage.omittedEntries += older;
    if (older) context.coverage.complete = false;
    check(
      Buffer.byteLength(JSON.stringify(context)) <= 49152,
      "context_metadata_limit",
    );
    return context;
  }
  async dispatch(frontId, waveId, input) {
    object(input, ["agentId", "expectedRevision", "requestId"]);
    identifier(input.agentId);
    check(
      Number.isSafeInteger(input.expectedRevision),
      "expected_revision_required",
    );
    const front = this.store.front(frontId),
      wave = this.store.get("waves", waveId);
    check(wave.frontId === frontId, "wrong_front", 409);
    const previous = this.store.operation(request(input));
    if (previous) {
      check(
        previous.kind === "wave.dispatch" &&
          previous.payloadDigest ===
            digest({
              kind: "wave.dispatch",
              payload: { ...input, waveId },
              frontId,
            }),
        "request_id_conflict",
        409,
      );
      if (previous.state !== "recorded") return previous;
    }
    const detail = await this.detail(frontId),
      projected = detail.waves.find((w) => w.id === waveId);
    check(
      projected.allowedActions.includes("dispatch") ||
        (previous?.state === "recorded" &&
          projected.planning === "approved" &&
          projected.dependencies === "satisfied" &&
          !projected.beadId),
      "wave_not_dispatchable",
      409,
    );
    this.plugins.executor(front.projectId);
    const agents = await this.plugins.agents(front.projectId);
    check(
      agents.some(
        (agent) =>
          (agent.id || agent.kind) === input.agentId &&
          agent.available !== false,
      ),
      "agent_unavailable",
      409,
    );
    const operation = await this.store.effect(
      input.requestId,
      "wave.dispatch",
      { ...input, waveId },
      frontId,
      input.expectedRevision,
      async () => {
        // Link the returned Bead before ready. A lost response remains unknown and is never resent.
        const created = await this.plugins.call(
          front.projectId,
          "task.create",
          {
            title: wave.title,
            description: `Acceptance criteria:\n${wave.criteria.map((c) => `- ${c}`).join("\n")}\n\nFront: ${front.id}\nPlan: ${wave.planId}\nWave: ${wave.id}`,
            harness: input.agentId,
            externalRef: wave.id,
          },
          `${input.requestId}:create`,
          "execution.submit",
        );
        check(typeof created.id === "string", "invalid_runtime_response", 503);
        wave.beadId = created.id;
        wave.operationId = input.requestId;
        this.store.transaction(() => {
          this.store.put("waves", wave);
          this.store.event(frontId, "wave.bound", {
            waveId,
            beadId: created.id,
          });
        });
        await this.plugins.call(
          front.projectId,
          "task.ready",
          { id: created.id, ready: true },
          `${input.requestId}:ready`,
          "execution.submit",
        );
        return { beadId: created.id, waveId };
      },
      () => {
        const current = this.store.get("waves", waveId);
        const priorAdmission = current.operationId
          ? this.store.operation(current.operationId)
          : null;
        check(
          !current.beadId &&
            (!priorAdmission || priorAdmission.state === "failed"),
          "admission_already_recorded",
          409,
        );
        current.operationId = input.requestId;
        this.store.put("waves", current);
      },
    );
    return operation;
  }
  withdrawWave(frontId, waveId, input) {
    object(input, ["requestId", "expectedRevision"]);
    check(
      Number.isSafeInteger(input.expectedRevision),
      "expected_revision_required",
    );
    return this.store.local(
      request(input),
      "wave.withdraw",
      { ...input, waveId },
      frontId,
      input.expectedRevision,
      () => {
        const wave = this.store.get("waves", waveId);
        check(wave.frontId === frontId, "wrong_front", 409);
        const admission = wave.operationId
          ? this.store.operation(wave.operationId)
          : null;
        check(
          !wave.beadId && (!admission || admission.state === "failed"),
          "wave_already_admitted",
          409,
        );
        wave.planning = transition("planning", wave.planning, "withdraw", {
          dispatched: false,
        });
        this.store.put("waves", wave);
        return wave;
      },
    );
  }
  async continueWave(frontId, waveId, input) {
    object(input, ["requestId"]);
    const front = this.store.front(frontId),
      detail = await this.detail(frontId),
      wave = detail.waves.find((w) => w.id === waveId);
    check(wave?.runId, "run_unavailable", 409);
    return this.store.effect(
      request(input),
      "wave.continue",
      { ...input, waveId },
      frontId,
      undefined,
      () =>
        this.plugins.call(
          front.projectId,
          "worktree.continue",
          { id: wave.runId },
          input.requestId,
          "execution.submit",
        ),
    );
  }
  async review(frontId, waveId, input) {
    object(input, [
      "decision",
      "resultRevision",
      "evidence",
      "reason",
      "requestId",
    ]);
    check(
      ["accepted", "changes_requested"].includes(input.decision),
      "invalid_decision",
    );
    text(input.resultRevision, 128);
    check(Array.isArray(input.evidence) && input.evidence.length <= 30);
    input.evidence.forEach((e) => text(e, 2000));
    if (input.decision === "accepted")
      check(input.evidence.length > 0, "evidence_required");
    else text(input.reason, 4000);
    const front = this.store.front(frontId),
      detail = await this.detail(frontId),
      wave = detail.waves.find((w) => w.id === waveId);
    check(wave?.runId, "run_unavailable", 409);
    check(wave.resultRevision === input.resultRevision, "stale_result", 409);
    return this.store.effect(
      request(input),
      "wave.review",
      { ...input, waveId },
      frontId,
      undefined,
      () =>
        this.plugins.call(
          front.projectId,
          input.decision === "accepted"
            ? "review.accept"
            : "review.changes_requested",
          {
            id: wave.runId,
            resultRevision: input.resultRevision,
            ...(input.decision === "accepted"
              ? { evidence: input.evidence }
              : { reason: input.reason }),
          },
          input.requestId,
          "execution.submit",
        ),
    );
  }
  async integrateWave(frontId, waveId, input) {
    object(input, ["resultRevision", "requestId"]);
    text(input.resultRevision, 128);
    const prior = this.store.operation(request(input));
    if (prior) {
      check(
        prior.frontId === frontId &&
          prior.kind === "wave.integrate" &&
          prior.payloadDigest ===
            digest({
              kind: "wave.integrate",
              payload: { ...input, waveId },
              frontId,
            }),
        "request_id_conflict",
        409,
      );
      return prior;
    }
    const front = this.store.front(frontId),
      detail = await this.detail(frontId),
      wave = detail.waves.find((item) => item.id === waveId);
    check(
      wave?.allowedActions.includes("integrate"),
      "result_not_integrable",
      409,
    );
    check(wave.resultRevision === input.resultRevision, "stale_result", 409);
    return this.store.effect(
      input.requestId,
      "wave.integrate",
      { ...input, waveId },
      frontId,
      undefined,
      () =>
        this.plugins.call(
          front.projectId,
          "worktree.integrate",
          { id: wave.runId, resultRevision: input.resultRevision },
          input.requestId,
          "execution.submit",
        ),
    );
  }
  async closeWave(frontId, waveId, input) {
    object(input, ["requestId"]);
    const prior = this.store.operation(request(input));
    if (prior) {
      check(
        prior.frontId === frontId &&
          prior.kind === "wave.close" &&
          prior.payloadDigest ===
            digest({
              kind: "wave.close",
              payload: { ...input, waveId },
              frontId,
            }),
        "request_id_conflict",
        409,
      );
      return prior;
    }
    const front = this.store.front(frontId),
      detail = await this.detail(frontId),
      wave = detail.waves.find((item) => item.id === waveId);
    check(
      wave?.beadId &&
        wave.review === "accepted" &&
        wave.reviewValidity === "current" &&
        wave.integration === "integrated",
      "delivery_not_accepted_and_integrated",
      409,
    );
    return this.store.effect(
      input.requestId,
      "wave.close",
      { ...input, waveId },
      frontId,
      undefined,
      () =>
        this.plugins.call(
          front.projectId,
          "delivery.close",
          { id: wave.beadId },
          input.requestId,
          "execution.submit",
        ),
    );
  }
  sessionConnection(projectId) {
    const instance = this.config
      .get()
      .connections.find(
        (c) =>
          c.projectId === projectId &&
          c.pluginId === "agent-session" &&
          c.enabled,
      );
    check(instance, "session_connection_disabled", 409);
    return { id: instance.id, revision: instance.revision };
  }
  checkSessionConnection(binding) {
    const instance = this.config.connection(binding.id);
    check(
      instance.enabled && instance.revision === binding.revision,
      "session_connection_changed",
      409,
    );
  }
  priorSessionOperation(frontId, kind, input, sessionId) {
    const op = this.store.operation(request(input));
    if (!op) return null;
    check(
      op.frontId === frontId &&
        op.kind === kind &&
        digest(op.payload.submitted) === digest(input) &&
        (!sessionId || op.payload.sessionId === sessionId),
      "request_id_conflict",
      409,
    );
    return op;
  }
  recordSessionStart(frontId, operationId, payload, result) {
    const providerRef = result.sessionId || result.id || result.session?.id;
    check(
      typeof providerRef === "string" && providerRef.length <= 4096,
      "invalid_runtime_response",
      503,
    );
    const existing = this.store
      .list("sessions", frontId)
      .find((session) => session.startOperationId === operationId);
    if (existing) return existing;
    const session = {
      id: randomUUID(),
      frontId,
      agentId: payload.submitted.agentId,
      createdAt: now(),
      providerRef,
      startOperationId: operationId,
    };
    this.store.transaction(() => {
      this.store.put("sessions", session);
      this.store.put("entries", {
        id: randomUUID(),
        frontId,
        kind: "message",
        text: payload.submitted.input,
        at: now(),
        requestId: operationId,
      });
    });
    return session;
  }
  recordSessionMessage(frontId, operationId, payload) {
    if (
      this.store
        .list("entries", frontId)
        .some((entry) => entry.requestId === operationId)
    )
      return;
    this.store.put("entries", {
      id: randomUUID(),
      frontId,
      kind: "message",
      text: payload.submitted.text,
      at: now(),
      requestId: operationId,
    });
  }
  async startSession(frontId, input) {
    object(input, ["agentId", "input", "selectedArtifacts", "requestId"]);
    identifier(input.agentId);
    text(input.input, 16000);
    const prior = this.priorSessionOperation(frontId, "session.start", input);
    if (prior) return prior;
    const front = this.store.front(frontId),
      binding = this.sessionConnection(front.projectId),
      agents = await this.plugins.agents(front.projectId);
    check(
      agents.some(
        (a) => (a.id || a.kind) === input.agentId && a.available !== false,
      ),
      "agent_unavailable",
      409,
    );
    const payload = {
      submitted: structuredClone(input),
      context: this.context(frontId, input.selectedArtifacts),
    };
    return this.store.effect(
      request(input),
      "session.start",
      payload,
      frontId,
      undefined,
      async () => {
        const result = await this.plugins.call(
          front.projectId,
          "session.start",
          {
            agentId: input.agentId,
            frontId,
            context: payload.context,
            input: input.input,
          },
          input.requestId,
          "conversation.start",
        );
        this.checkSessionConnection(binding);
        return this.recordSessionStart(
          frontId,
          input.requestId,
          payload,
          result,
        );
      },
    );
  }
  async sendSession(frontId, sessionId, input) {
    object(input, ["text", "selectedArtifacts", "requestId"]);
    text(input.text, 16000);
    const prior = this.priorSessionOperation(
      frontId,
      "session.send",
      input,
      sessionId,
    );
    if (prior) return prior;
    const front = this.store.front(frontId),
      session = this.store.get("sessions", sessionId);
    check(session.frontId === frontId, "wrong_front", 409);
    const binding = this.sessionConnection(front.projectId);
    const payload = {
      submitted: structuredClone(input),
      sessionId,
      providerRef: session.providerRef,
      context: this.context(frontId, input.selectedArtifacts),
    };
    return this.store.effect(
      request(input),
      "session.send",
      payload,
      frontId,
      undefined,
      async () => {
        const result = await this.plugins.call(
          front.projectId,
          "session.send",
          {
            sessionId: session.providerRef,
            context: payload.context,
            input: input.text,
          },
          input.requestId,
          "conversation.send",
        );
        this.checkSessionConnection(binding);
        this.recordSessionMessage(frontId, input.requestId, payload);
        return result;
      },
    );
  }
  async observeSession(frontId, sessionId, cursor) {
    const front = this.store.front(frontId),
      session = this.store.get("sessions", sessionId);
    check(session.frontId === frontId, "wrong_front", 409);
    const binding = this.sessionConnection(front.projectId);
    const result = await this.plugins.call(
      front.projectId,
      "session.observe",
      { sessionId: session.providerRef, ...(cursor ? { cursor } : {}) },
      "session-observe",
      "conversation.observe",
    );
    this.checkSessionConnection(binding);
    const content = result.text || result.output || result.message?.text;
    if (typeof content === "string" && content.trim()) {
      check(Buffer.byteLength(content) <= 65536, "agent_response_limit", 503);
      const requestId = `observe:${digest({ sessionId, content })}`;
      this.store.local(
        requestId,
        "session.observed",
        { sessionId, content },
        frontId,
        undefined,
        () => {
          const entry = {
            id: randomUUID(),
            frontId,
            kind: "agent",
            text: content,
            at: now(),
            requestId,
          };
          this.store.put("entries", entry);
          return entry;
        },
      );
    }
    return result;
  }
  addArtifact(frontId, input) {
    object(input, ["name", "text", "requestId"]);
    this.store.front(frontId);
    return this.store.local(
      request(input),
      "artifact.append",
      input,
      frontId,
      undefined,
      () => this.store.addArtifact(frontId, input.name, input.text),
    );
  }
  importArtifact(frontId, input) {
    object(input, ["path", "requestId"]);
    const front = this.store.front(frontId),
      source = this.plugins.importFile(front.projectId, input.path);
    return this.addArtifact(frontId, { ...source, requestId: input.requestId });
  }
  saveSettings(input) {
    const clean = structuredClone(input);
    for (const item of clean.connections || []) {
      delete item.health;
      delete item.lastGoodAt;
    }
    this.config.save(clean);
    return this.plugins.settings();
  }
  runtimeStart(projectId) {
    const project = this.config.project(projectId);
    check(project.executorId, "executor_not_configured", 409);
    this.plugins.connection(project.executorId, "execution.submit");
    return this.plugins.ontrack.start(project);
  }
  async reconcile(frontId, operationId) {
    const op = this.store.operation(operationId);
    check(op && op.frontId === frontId, "operation_not_found", 404);
    check(
      ["recorded", "unknown"].includes(op.state),
      "operation_not_reconcilable",
      409,
    );
    const front = this.store.front(frontId);
    if (op.state === "recorded" && op.kind === "wave.dispatch")
      return this.dispatch(
        frontId,
        op.payload.waveId,
        (({ waveId, ...input }) => input)(op.payload),
      );
    if (op.kind !== "wave.dispatch") {
      const remote = await this.plugins.call(
        front.projectId,
        "operation.get",
        { id: operationId },
        "operation-read",
      );
      if (remote.state === "confirmed") {
        if (op.kind === "session.start")
          op.result = this.recordSessionStart(
            frontId,
            operationId,
            op.payload,
            remote.result,
          );
        else {
          if (op.kind === "session.send")
            this.recordSessionMessage(frontId, operationId, op.payload);
          op.result = remote.result;
        }
        op.state = "confirmed";
        delete op.error;
        this.store.transaction(() => {
          this.store.putOperation(op);
          this.store.event(frontId, "operation.reconciled", {
            requestId: operationId,
          });
        });
      }
      return op;
    }
    const created = await this.plugins.call(
      front.projectId,
      "operation.get",
      { id: `${operationId}:create` },
      "operation-create-read",
    );
    if (created.state !== "confirmed" || typeof created.result?.id !== "string")
      return op;
    const wave = this.store.get("waves", op.payload.waveId);
    check(
      !wave.beadId || wave.beadId === created.result.id,
      "bead_binding_conflict",
      409,
    );
    wave.beadId = created.result.id;
    wave.operationId = operationId;
    this.store.transaction(() => {
      this.store.put("waves", wave);
      this.store.event(frontId, "wave.binding_reconciled", {
        waveId: wave.id,
        beadId: wave.beadId,
      });
    });
    let ready;
    try {
      ready = await this.plugins.call(
        front.projectId,
        "operation.get",
        { id: `${operationId}:ready` },
        "operation-ready-read",
      );
    } catch (error) {
      if (error.code !== "operation_not_found") throw error;
      // The owner's durable ledger proves ready was not admitted. Resubmit only this command,
      // on the same Bead and stable ID. Creation is never repeated.
      await this.plugins.call(
        front.projectId,
        "task.ready",
        { id: wave.beadId, ready: true },
        `${operationId}:ready`,
        "execution.submit",
      );
      ready = await this.plugins.call(
        front.projectId,
        "operation.get",
        { id: `${operationId}:ready` },
        "operation-ready-read",
      );
    }
    if (ready.state === "confirmed") {
      op.state = "confirmed";
      op.result = { waveId: wave.id, beadId: wave.beadId };
      delete op.error;
      this.store.transaction(() => {
        this.store.putOperation(op);
        this.store.event(frontId, "operation.reconciled", {
          requestId: operationId,
        });
      });
    }
    return op;
  }
}
export function createApplication(options) {
  return new Application(options);
}
