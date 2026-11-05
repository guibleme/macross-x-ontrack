import fs from "node:fs";
import path from "node:path";
import { randomUUID } from "node:crypto";
import { check, identifier, object, text } from "./errors.mjs";
import { validateDefinitions } from "./plugin-contracts.mjs";

export class Configuration {
  constructor(root, definitions) {
    this.root = root;
    this.file = path.join(root, "connections.json");
    this.definitions = validateDefinitions(definitions);
    this.value = fs.existsSync(this.file)
      ? JSON.parse(fs.readFileSync(this.file, "utf8"))
      : { version: 1, revision: 0, projects: [], connections: [] };
    this.validate(this.value);
  }
  validate(value) {
    object(value, ["version", "revision", "projects", "connections"]);
    check(value.version === 1, "unsupported_configuration_version", 409);
    check(Number.isSafeInteger(value.revision) && value.revision >= 0);
    check(Array.isArray(value.projects) && value.projects.length <= 100);
    check(Array.isArray(value.connections) && value.connections.length <= 300);
    const projects = new Set(),
      connections = new Set();
    for (const project of value.projects) {
      object(project, ["id", "name", "repo", "documentRoots", "executorId"]);
      identifier(project.id);
      check(!projects.has(project.id), "duplicate_project");
      projects.add(project.id);
      text(project.name, 200);
      check(path.isAbsolute(project.repo), "absolute_repository_required");
      check(
        Array.isArray(project.documentRoots) &&
          project.documentRoots.length <= 20,
      );
      for (const root of project.documentRoots)
        check(
          typeof root === "string" && path.isAbsolute(root),
          "absolute_document_root_required",
        );
      check(
        project.executorId === null ||
          project.executorId === undefined ||
          typeof project.executorId === "string",
      );
    }
    for (const item of value.connections) {
      object(item, [
        "id",
        "pluginId",
        "name",
        "projectId",
        "enabled",
        "config",
        "revision",
      ]);
      identifier(item.id);
      check(!connections.has(item.id), "duplicate_connection");
      connections.add(item.id);
      text(item.name, 200);
      check(projects.has(item.projectId), "unknown_project");
      check(typeof item.enabled === "boolean");
      const def = this.definitions.find((d) => d.id === item.pluginId);
      check(def, "unknown_plugin");
      object(
        item.config || {},
        def.fields.map((f) => f.key),
      );
      for (const [key, v] of Object.entries(item.config || {})) {
        check(typeof v === "string" && v.length <= 500, "invalid_config");
        check(
          !/(token|password|secret|apiKey)/i.test(key),
          "credentials_reference_only",
        );
      }
      check(item.revision === undefined || Number.isSafeInteger(item.revision));
    }
    for (const project of value.projects)
      if (project.executorId)
        check(
          value.connections.some(
            (c) => c.id === project.executorId && c.projectId === project.id,
          ),
          "invalid_executor_binding",
        );
  }
  get() {
    return structuredClone(this.value);
  }
  save(input) {
    this.validate(input);
    check(input.revision === this.value.revision, "stale_configuration", 409);
    const previous = this.value;
    const next = structuredClone(input);
    next.revision++;
    for (const item of next.connections) {
      const old = previous.connections.find((c) => c.id === item.id);
      item.revision =
        old &&
        JSON.stringify({ ...old, revision: 0 }) ===
          JSON.stringify({ ...item, revision: 0 })
          ? old.revision
          : (old?.revision || 0) + 1;
    }
    fs.mkdirSync(this.root, { recursive: true, mode: 0o700 });
    const temporary = `${this.file}.${randomUUID()}.tmp`;
    const fd = fs.openSync(temporary, "wx", 0o600);
    try {
      fs.writeFileSync(fd, JSON.stringify(next, null, 2));
      fs.fsyncSync(fd);
    } finally {
      fs.closeSync(fd);
    }
    fs.renameSync(temporary, this.file);
    const dir = fs.openSync(this.root, "r");
    try {
      fs.fsyncSync(dir);
    } finally {
      fs.closeSync(dir);
    }
    this.value = next;
    return this.get();
  }
  project(id) {
    const value = this.value.projects.find((p) => p.id === id);
    check(value, "project_not_found", 404);
    return structuredClone(value);
  }
  connection(id) {
    const value = this.value.connections.find((c) => c.id === id);
    check(value, "connection_not_found", 404);
    return structuredClone(value);
  }
  toggle(id, enabled, revision) {
    check(typeof enabled === "boolean");
    const next = this.get(),
      item = next.connections.find((c) => c.id === id);
    check(item, "connection_not_found", 404);
    if (revision !== undefined)
      check(item.revision === revision, "stale_configuration", 409);
    item.enabled = enabled;
    return this.save(next);
  }
}
