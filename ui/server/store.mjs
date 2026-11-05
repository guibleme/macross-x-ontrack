import { DatabaseSync, backup } from "node:sqlite";
import {
  mkdirSync,
  chmodSync,
  readFileSync,
  writeFileSync,
  renameSync,
  existsSync,
  realpathSync,
  openSync,
  closeSync,
  fsyncSync,
} from "node:fs";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";
import { AppError, check, identifier, text } from "./errors.mjs";

export const digest = (value) =>
  createHash("sha256")
    .update(typeof value === "string" ? value : JSON.stringify(value))
    .digest("hex");
const parse = (row) => (row ? JSON.parse(row.data) : null);
export class Store {
  constructor(root) {
    this.root = path.resolve(root);
    mkdirSync(this.root, { recursive: true, mode: 0o700 });
    this.db = new DatabaseSync(path.join(this.root, "macross.sqlite"));
    chmodSync(path.join(this.root, "macross.sqlite"), 0o600);
    const metadataExists = this.db
      .prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='metadata'",
      )
      .get();
    if (metadataExists) {
      const schema = this.db
        .prepare("SELECT value FROM metadata WHERE key=?")
        .get("schema");
      check(schema?.value === "1", "unsupported_store_version", 409);
    }
    this.db
      .exec(`PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000; PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;
      CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY,value TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS fronts (id TEXT PRIMARY KEY,data TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS entries (id TEXT PRIMARY KEY,front_id TEXT NOT NULL REFERENCES fronts(id),data TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS plans (id TEXT PRIMARY KEY,front_id TEXT NOT NULL REFERENCES fronts(id),data TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS waves (id TEXT PRIMARY KEY,front_id TEXT NOT NULL REFERENCES fronts(id),plan_id TEXT NOT NULL REFERENCES plans(id),data TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY,front_id TEXT NOT NULL REFERENCES fronts(id),data TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS artifacts (digest TEXT NOT NULL,front_id TEXT NOT NULL REFERENCES fronts(id),data TEXT NOT NULL,PRIMARY KEY(digest,front_id));
      CREATE TABLE IF NOT EXISTS operations (id TEXT PRIMARY KEY,data TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS events (seq INTEGER PRIMARY KEY AUTOINCREMENT,front_id TEXT,event TEXT NOT NULL,data TEXT NOT NULL);`);
    const version = this.db
      .prepare("SELECT value FROM metadata WHERE key=?")
      .get("schema");
    check(!version || version.value === "1", "unsupported_store_version", 409);
    this.db
      .prepare("INSERT OR IGNORE INTO metadata VALUES (?,?)")
      .run("schema", "1");
    // A process can die after an effect was accepted. Never replay its intent automatically.
    for (const row of this.db.prepare("SELECT id,data FROM operations").all()) {
      const op = parse(row);
      if (op.state === "dispatched") {
        op.state = "unknown";
        op.error = "restart_requires_reconciliation";
        this.putOperation(op);
      }
    }
  }
  close() {
    this.db.close();
  }
  transaction(fn) {
    this.db.exec("BEGIN IMMEDIATE");
    try {
      const result = fn();
      this.db.exec("COMMIT");
      return result;
    } catch (error) {
      this.db.exec("ROLLBACK");
      throw error;
    }
  }
  front(id) {
    const item = parse(
      this.db.prepare("SELECT data FROM fronts WHERE id=?").get(identifier(id)),
    );
    check(item, "front_not_found", 404);
    return item;
  }
  fronts() {
    return this.db
      .prepare("SELECT data FROM fronts ORDER BY rowid DESC")
      .all()
      .map(parse);
  }
  list(table, frontId) {
    check(
      ["entries", "plans", "waves", "sessions", "artifacts"].includes(table),
    );
    return this.db
      .prepare(`SELECT data FROM ${table} WHERE front_id=? ORDER BY rowid`)
      .all(frontId)
      .map(parse);
  }
  get(table, id) {
    check(["plans", "waves", "sessions"].includes(table));
    const item = parse(
      this.db
        .prepare(`SELECT data FROM ${table} WHERE id=?`)
        .get(identifier(id)),
    );
    check(item, `${table}_not_found`, 404);
    return item;
  }
  put(table, value, frontId = value.frontId) {
    check(["fronts", "entries", "plans", "waves", "sessions"].includes(table));
    if (table === "fronts")
      this.db
        .prepare(
          "INSERT INTO fronts VALUES (?,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
        )
        .run(value.id, JSON.stringify(value));
    else if (table === "waves")
      this.db
        .prepare(
          "INSERT INTO waves VALUES (?,?,?,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
        )
        .run(value.id, frontId, value.planId, JSON.stringify(value));
    else
      this.db
        .prepare(
          `INSERT INTO ${table} VALUES (?,?,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data`,
        )
        .run(value.id, frontId, JSON.stringify(value));
  }
  revise(frontId, expected) {
    const front = this.front(frontId);
    check(
      Number.isSafeInteger(expected) && expected === front.revision,
      "stale_revision",
      409,
    );
    front.revision++;
    front.updatedAt = new Date().toISOString();
    this.put("fronts", front);
    return front;
  }
  event(frontId, event, data) {
    this.db
      .prepare("INSERT INTO events(front_id,event,data) VALUES (?,?,?)")
      .run(frontId, event, JSON.stringify(data));
  }
  operation(id) {
    return parse(
      this.db
        .prepare("SELECT data FROM operations WHERE id=?")
        .get(identifier(id)),
    );
  }
  putOperation(op) {
    this.db
      .prepare(
        "INSERT INTO operations VALUES (?,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
      )
      .run(op.id, JSON.stringify(op));
  }
  prepare(
    id,
    kind,
    payload,
    frontId,
    expectedRevision,
    effect = false,
    onPrepare = () => {},
  ) {
    identifier(id);
    const payloadDigest = digest({ kind, payload, frontId: frontId || null });
    const previous = this.operation(id);
    if (previous) {
      check(
        previous.payloadDigest === payloadDigest,
        "request_id_conflict",
        409,
      );
      return { operation: previous, existing: true };
    }
    return this.transaction(() => {
      if (frontId && expectedRevision !== undefined)
        this.revise(frontId, expectedRevision);
      onPrepare();
      const operation = {
        id,
        kind,
        payloadDigest,
        frontId: frontId || null,
        payload: structuredClone(payload),
        expectedRevision: expectedRevision ?? null,
        state: effect ? "recorded" : "confirmed",
        createdAt: new Date().toISOString(),
        result: null,
      };
      this.putOperation(operation);
      this.event(frontId || null, kind, { requestId: id, payloadDigest });
      return { operation, existing: false };
    });
  }
  local(id, kind, payload, frontId, expectedRevision, apply) {
    identifier(id);
    const payloadDigest = digest({ kind, payload, frontId: frontId || null });
    const prior = this.operation(id);
    if (prior) {
      check(prior.payloadDigest === payloadDigest, "request_id_conflict", 409);
      return prior.result;
    }
    return this.transaction(() => {
      if (frontId && expectedRevision !== undefined)
        this.revise(frontId, expectedRevision);
      const result = apply();
      this.putOperation({
        id,
        kind,
        payloadDigest,
        frontId: frontId || null,
        state: "confirmed",
        createdAt: new Date().toISOString(),
        result,
      });
      this.event(frontId || null, kind, { requestId: id, payloadDigest });
      return result;
    });
  }
  async effect(id, kind, payload, frontId, expectedRevision, apply, onPrepare) {
    const prepared = this.prepare(
        id,
        kind,
        payload,
        frontId,
        expectedRevision,
        true,
        onPrepare,
      ),
      op = prepared.operation;
    if (prepared.existing && op.state !== "recorded") return op;
    op.state = "dispatched";
    this.putOperation(op);
    try {
      const result = await apply();
      op.state = "confirmed";
      op.result = result;
    } catch (error) {
      op.state = [
        "runtime_unavailable",
        "connection_disabled",
        "capability_unavailable",
        "invalid_input",
        "precondition_failed",
      ].includes(error.code)
        ? "failed"
        : "unknown";
      op.error =
        error instanceof AppError ? error.code : "operation_unavailable";
    }
    this.transaction(() => {
      this.putOperation(op);
      this.event(frontId || null, `${kind}.${op.state}`, { requestId: id });
    });
    return op;
  }
  addArtifact(frontId, name, content) {
    name = text(name, 200);
    content = text(content, 2 * 1024 * 1024);
    const hash = digest(content),
      folder = path.join(this.root, "artifacts");
    mkdirSync(folder, { recursive: true, mode: 0o700 });
    const file = path.join(folder, hash);
    if (!existsSync(file)) {
      const tmp = `${file}.${randomUUID()}.tmp`;
      const fd = openSync(tmp, "wx", 0o600);
      try {
        writeFileSync(fd, content);
        fsyncSync(fd);
      } finally {
        closeSync(fd);
      }
      renameSync(tmp, file);
      const dir = openSync(folder, "r");
      try {
        fsyncSync(dir);
      } finally {
        closeSync(dir);
      }
    }
    const artifact = {
      digest: hash,
      frontId,
      name,
      size: Buffer.byteLength(content),
      createdAt: new Date().toISOString(),
    };
    this.db
      .prepare(
        "INSERT INTO artifacts VALUES (?,?,?) ON CONFLICT(digest,front_id) DO UPDATE SET data=excluded.data",
      )
      .run(hash, frontId, JSON.stringify(artifact));
    return artifact;
  }
  artifact(hash) {
    check(/^[a-f0-9]{64}$/.test(hash), "invalid_digest");
    const row = this.db
      .prepare("SELECT data FROM artifacts WHERE digest=? LIMIT 1")
      .get(hash);
    check(row, "artifact_not_found", 404);
    const meta = parse(row),
      content = readFileSync(path.join(this.root, "artifacts", hash), "utf8");
    check(digest(content) === hash, "artifact_corrupt", 409);
    return { ...meta, text: content };
  }
  async backup(name = randomUUID()) {
    identifier(name);
    const directory = path.join(this.root, "backups", name);
    mkdirSync(directory, { recursive: true, mode: 0o700 });
    const dbPath = path.join(directory, "macross.sqlite");
    check(!existsSync(dbPath), "backup_exists", 409);
    await backup(this.db, dbPath);
    chmodSync(dbPath, 0o600);
    const blobs = this.db
      .prepare("SELECT DISTINCT digest FROM artifacts")
      .all()
      .map((row) => row.digest);
    mkdirSync(path.join(directory, "artifacts"), { mode: 0o700 });
    for (const hash of blobs) {
      const item = this.artifact(hash);
      writeFileSync(path.join(directory, "artifacts", hash), item.text, {
        mode: 0o600,
      });
    }
    const config = path.join(this.root, "connections.json");
    if (existsSync(config))
      writeFileSync(
        path.join(directory, "connections.json"),
        readFileSync(config),
        { mode: 0o600 },
      );
    writeFileSync(
      path.join(directory, "manifest.json"),
      JSON.stringify({ version: 1, artifacts: blobs }),
      { mode: 0o600 },
    );
    return { path: directory };
  }
}
