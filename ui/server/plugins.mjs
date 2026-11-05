import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { AppError, check } from "./errors.mjs";
import {
  CONNECTOR_PROTOCOL_VERSION,
  CONNECTOR_CONTRACT,
  validateDefinitions,
} from "./plugin-contracts.mjs";
export const DEFINITIONS = Object.freeze([
  {
    id: "ontrack",
    name: "OnTrack",
    kind: "executor",
    protocolVersion: CONNECTOR_PROTOCOL_VERSION,
    contracts: ["source", "executor", "session"],
    operations: ["source", "executor", "session"].flatMap(
      (kind) => CONNECTOR_CONTRACT.requiredOperations[kind],
    ),
    capabilities: [
      "source.snapshot",
      "execution.submit",
      "execution.observe",
      "conversation.start",
      "conversation.send",
      "conversation.observe",
    ],
    fields: [],
  },
  {
    id: "local-documents",
    name: "Local documents",
    kind: "source",
    protocolVersion: CONNECTOR_PROTOCOL_VERSION,
    contracts: ["source"],
    operations: CONNECTOR_CONTRACT.requiredOperations.source,
    capabilities: ["source.snapshot", "source.document"],
    fields: [],
  },
  {
    id: "agent-session",
    name: "Agent sessions",
    kind: "session",
    protocolVersion: CONNECTOR_PROTOCOL_VERSION,
    contracts: ["session"],
    operations: CONNECTOR_CONTRACT.requiredOperations.session,
    capabilities: [
      "conversation.start",
      "conversation.send",
      "conversation.observe",
    ],
    fields: [],
  },
]);
const within = (root, file) => {
  const relative = path.relative(root, file);
  return (
    relative === "" ||
    (!relative.startsWith(".." + path.sep) &&
      relative !== ".." &&
      !path.isAbsolute(relative))
  );
};
export function runtimeEnvironment(source = process.env) {
  const localBin = path.join(os.homedir(), ".local", "bin");
  const entries = (source.PATH || "/usr/bin:/bin:/usr/sbin:/sbin").split(
    path.delimiter,
  );
  if (!entries.includes(localBin)) entries.push(localBin);
  return { ...source, PATH: entries.join(path.delimiter) };
}
export class OntrackAdapter {
  constructor({
    binary = process.env.MACROSS_ONTRACK_BIN || "ontrack",
    timeoutMs = 60000,
    spawnProcess = spawn,
  } = {}) {
    this.binary = binary;
    this.timeoutMs = timeoutMs;
    this.spawn = spawnProcess;
  }
  call(project, method, params = {}, id = "read") {
    const packet = JSON.stringify({ version: 1, id, method, params });
    check(Buffer.byteLength(packet) <= 65536, "invalid_input");
    const timeout = [
      "session.start",
      "session.send",
      "task.ready",
      "worktree.continue",
    ].includes(method)
      ? Math.max(this.timeoutMs, 90000)
      : this.timeoutMs;
    return new Promise((resolve, reject) => {
      let output = "",
        bytes = 0,
        settled = false;
      const child = this.spawn(
        this.binary,
        [
          "api",
          ...(method === "capabilities" ||
          method.endsWith(".list") ||
          method.endsWith(".show") ||
          ["operation.get", "session.observe", "runtime.status"].includes(
            method,
          )
            ? []
            : ["--allow-writes"]),
        ],
        {
          cwd: project.repo,
          stdio: ["pipe", "pipe", "pipe"],
          shell: false,
          env: runtimeEnvironment(),
        },
      );
      const finish = (error, value) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        if (error) reject(error);
        else resolve(value);
      };
      const timer = setTimeout(() => {
        child.kill("SIGKILL");
        finish(new AppError("runtime_timeout", 503));
      }, timeout);
      child.on("error", () => finish(new AppError("runtime_unavailable", 503)));
      child.stderr.on("data", () => {});
      child.stdout.on("data", (chunk) => {
        bytes += chunk.length;
        if (bytes > 4 * 1024 * 1024) {
          child.kill("SIGKILL");
          finish(new AppError("runtime_output_limit", 503));
        } else output += chunk;
      });
      child.on("close", () => {
        if (settled) return;
        try {
          const response = JSON.parse(output);
          check(
            response.version === 1 && response.id === id,
            "invalid_runtime_response",
            503,
          );
          if (response.error)
            throw new AppError(
              /^[a-z_]{1,80}$/.test(response.error.code)
                ? response.error.code
                : "runtime_error",
              503,
            );
          finish(null, response.result);
        } catch (error) {
          finish(
            error instanceof AppError
              ? error
              : new AppError("invalid_runtime_response", 503),
          );
        }
      });
      child.stdin.on("error", () => {});
      child.stdin.end(packet);
    });
  }
  start(project) {
    const child = this.spawn(this.binary, ["serve"], {
      cwd: project.repo,
      stdio: "ignore",
      detached: true,
      shell: false,
      env: runtimeEnvironment(),
    });
    child.on("error", () => {});
    child.unref();
    return { status: "start_requested" };
  }
}
export class Plugins {
  constructor(
    config,
    {
      ontrack = new OntrackAdapter(),
      providers = {},
      definitions = DEFINITIONS,
    } = {},
  ) {
    this.config = config;
    this.ontrack = ontrack;
    this.providers = providers;
    this.registered = validateDefinitions(definitions);
    this.health = new Map();
  }
  definitions() {
    return structuredClone(this.registered);
  }
  provider(instance) {
    const adapter =
      this.providers[instance.id] || this.providers[instance.pluginId];
    if (adapter) return adapter;
    if (["ontrack", "agent-session"].includes(instance.pluginId))
      return this.ontrack;
    throw new AppError("capability_unavailable", 409);
  }
  connection(id, capability) {
    const connection = this.config.connection(id);
    check(connection.enabled, "connection_disabled", 409);
    const def = this.registered.find((d) => d.id === connection.pluginId);
    check(
      def?.capabilities.includes(capability),
      "capability_unavailable",
      409,
    );
    return connection;
  }
  executor(projectId) {
    const project = this.config.project(projectId);
    check(project.executorId, "executor_not_configured", 409);
    const instance = this.connection(project.executorId, "execution.submit");
    return { project, instance };
  }
  async call(projectId, method, params, id, capability = "execution.observe") {
    const project = this.config.project(projectId);
    check(project.executorId, "executor_not_configured", 409);
    const instance = this.connection(project.executorId, capability),
      generation = instance.revision;
    const adapter = this.provider(instance);
    const result = await adapter.call(project, method, params, id);
    const current = this.config.connection(instance.id);
    check(
      current.enabled && current.revision === generation,
      "connection_changed",
      409,
    );
    this.health.set(instance.id, {
      status: "available",
      lastGoodAt: new Date().toISOString(),
    });
    return result;
  }
  async test(id) {
    const instance = this.config.connection(id),
      project = this.config.project(instance.projectId),
      generation = instance.revision;
    try {
      let result;
      if (instance.pluginId === "local-documents") {
        for (const root of project.documentRoots) {
          check(
            fs.statSync(root).isDirectory(),
            "document_root_unavailable",
            503,
          );
        }
        result = { roots: project.documentRoots.length };
      } else {
        const provider = this.provider(instance);
        result =
          typeof provider.readSnapshot === "function"
            ? await provider.readSnapshot(project, instance)
            : await provider.call(
                project,
                "capabilities",
                {},
                "connection-test",
              );
        if (["ontrack", "agent-session"].includes(instance.pluginId))
          await provider.call(
            project,
            "runtime.status",
            {},
            "connection-runtime-test",
          );
      }
      check(
        this.config.connection(id).revision === generation,
        "connection_changed",
        409,
      );
      const health = {
        status: "available",
        lastGoodAt: new Date().toISOString(),
        result,
      };
      this.health.set(id, health);
      return health;
    } catch (error) {
      const prior = this.health.get(id);
      const health = {
        status: prior?.lastGoodAt ? "stale" : "unavailable",
        lastGoodAt: prior?.lastGoodAt || null,
        error:
          error instanceof AppError ? error.code : "connection_unavailable",
      };
      this.health.set(id, health);
      return health;
    }
  }
  async readSource(id) {
    const instance = this.connection(id, "source.snapshot"),
      project = this.config.project(instance.projectId),
      generation = instance.revision;
    let result;
    if (instance.pluginId === "local-documents") {
      result = {
        records: project.documentRoots.flatMap((root) =>
          fs
            .readdirSync(root, { withFileTypes: true })
            .filter(
              (item) =>
                item.isFile() &&
                /\.(md|txt)$/i.test(item.name) &&
                !item.name.startsWith("."),
            )
            .slice(0, 200)
            .map((item) => ({
              sourceInstanceId: id,
              nativeRecordId: path.join(root, item.name),
              label: item.name,
            })),
        ),
        complete: false,
      };
    } else {
      const adapter = this.provider(instance);
      result =
        typeof adapter.readSnapshot === "function"
          ? await adapter.readSnapshot(project, instance)
          : await adapter.call(project, "board.list", {}, "source-snapshot");
    }
    const current = this.config.connection(id);
    check(
      current.enabled && current.revision === generation,
      "connection_changed",
      409,
    );
    return {
      sourceInstanceId: id,
      observedAt: new Date().toISOString(),
      status: "available",
      snapshot: result,
    };
  }
  settings() {
    const value = this.config.get();
    value.connections = value.connections.map((item) => ({
      ...item,
      health: item.enabled
        ? this.health.get(item.id)?.status || "unknown"
        : "disabled",
      lastGoodAt: this.health.get(item.id)?.lastGoodAt || null,
    }));
    return value;
  }
  async agents(projectId) {
    const rows = await this.call(projectId, "agents.list", {}, "agents-list");
    return (Array.isArray(rows) ? rows : rows.agents || []).map((agent) =>
      typeof agent === "string"
        ? { id: agent, name: agent, available: true, capabilities: [] }
        : agent,
    );
  }
  importFile(projectId, filename) {
    const project = this.config.project(projectId);
    const configured = this.config
      .get()
      .connections.some(
        (c) =>
          c.projectId === projectId &&
          c.pluginId === "local-documents" &&
          c.enabled,
      );
    check(configured, "documents_connection_disabled", 409);
    check(
      typeof filename === "string" && path.isAbsolute(filename),
      "absolute_document_path_required",
    );
    const target = fs.realpathSync(filename),
      allowed = project.documentRoots.map((root) => fs.realpathSync(root));
    check(
      allowed.some((root) => within(root, target)),
      "document_outside_roots",
      403,
    );
    const stat = fs.statSync(target);
    check(stat.isFile() && stat.size <= 2 * 1024 * 1024, "document_size_limit");
    check(
      !/(^|\/)(\.env(?:\.|$)|\.npmrc$|credentials(?:\.|$)|id_(rsa|ed25519)$)/i.test(
        target,
      ),
      "private_document",
      403,
    );
    const fd = fs.openSync(
      target,
      fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW,
    );
    try {
      const before = fs.fstatSync(fd);
      check(
        before.isFile() && before.size <= 2 * 1024 * 1024,
        "document_size_limit",
      );
      const content = fs.readFileSync(fd, "utf8"),
        after = fs.fstatSync(fd);
      check(
        before.size === after.size && before.mtimeMs === after.mtimeMs,
        "document_changed",
        409,
      );
      return { name: path.basename(target), text: content };
    } finally {
      fs.closeSync(fd);
    }
  }
}
