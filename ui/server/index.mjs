import http from "node:http";
import path from "node:path";
import fs from "node:fs/promises";
import { fileURLToPath, pathToFileURL } from "node:url";
import { createApplication } from "./application.mjs";
import { AppError, check, errorCode } from "./errors.mjs";
export { createApplication } from "./application.mjs";
const mime = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
};
export function createServer(
  app,
  {
    dist = path.resolve(
      path.dirname(fileURLToPath(import.meta.url)),
      "../dist",
    ),
  } = {},
) {
  return http.createServer(async (req, res) => {
    res.setHeader("Cache-Control", "no-store");
    res.setHeader("X-Content-Type-Options", "nosniff");
    res.setHeader("Referrer-Policy", "no-referrer");
    res.setHeader(
      "Content-Security-Policy",
      "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
    );
    const send = (status, data) => {
      res.writeHead(status, {
        "Content-Type": "application/json; charset=utf-8",
      });
      res.end(JSON.stringify(data));
    };
    try {
      const host = req.headers.host,
        port = req.socket.localPort;
      check(
        [`127.0.0.1:${port}`, `localhost:${port}`].includes(host),
        "invalid_host",
        403,
      );
      check(
        !req.headers.origin || req.headers.origin === `http://${host}`,
        "invalid_origin",
        403,
      );
      const staticNavigation =
        req.method === "GET" &&
        req.headers["sec-fetch-mode"] === "navigate" &&
        !req.url.startsWith("/api/");
      check(
        staticNavigation ||
          !req.headers["sec-fetch-site"] ||
          ["same-origin", "none"].includes(req.headers["sec-fetch-site"]),
        "cross_origin_request",
        403,
      );
      const url = new URL(req.url, `http://${host}`),
        parts = url.pathname.split("/").filter(Boolean).map(decodeURIComponent);
      if (parts[0] !== "api") {
        check(req.method === "GET", "method_not_allowed", 405);
        const rel = parts.length ? parts.join("/") : "index.html",
          target = path.resolve(dist, rel);
        check(
          target.startsWith(path.resolve(dist) + path.sep),
          "invalid_path",
          403,
        );
        let content;
        try {
          content = await fs.readFile(target);
        } catch {
          throw new AppError("asset_not_found", 404);
        }
        res.writeHead(200, {
          "Content-Type":
            mime[path.extname(target)] || "application/octet-stream",
        });
        res.end(content);
        return;
      }
      let body = {};
      if (["POST", "PUT"].includes(req.method)) {
        check(
          (req.headers["content-type"] || "").split(";")[0] ===
            "application/json",
          "json_required",
          415,
        );
        let size = 0,
          chunks = [];
        for await (const chunk of req) {
          size += chunk.length;
          check(size <= 2 * 1024 * 1024, "request_too_large", 413);
          chunks.push(chunk);
        }
        try {
          body = JSON.parse(Buffer.concat(chunks).toString() || "{}");
        } catch {
          throw new AppError("invalid_json");
        }
        check(body && typeof body === "object" && !Array.isArray(body));
      }
      const method = req.method,
        frontId = parts[2];
      let data;
      if (method === "GET" && url.pathname === "/api/health")
        data = app.health();
      else if (method === "GET" && url.pathname === "/api/fronts")
        data = app.fronts();
      else if (method === "POST" && url.pathname === "/api/fronts")
        data = app.createFront(body);
      else if (method === "GET" && url.pathname === "/api/settings")
        data = app.plugins.settings();
      else if (method === "PUT" && url.pathname === "/api/settings")
        data = app.saveSettings(body);
      else if (method === "GET" && url.pathname === "/api/plugins")
        data = app.plugins.definitions();
      else if (method === "GET" && url.pathname === "/api/agents")
        data = await app.plugins.agents(url.searchParams.get("projectId"));
      else if (
        parts[1] === "connections" &&
        parts.length === 4 &&
        method === "GET" &&
        parts[3] === "snapshot"
      )
        data = await app.plugins.readSource(parts[2]);
      else if (
        parts[1] === "connections" &&
        parts.length === 4 &&
        method === "POST" &&
        parts[3] === "test"
      )
        data = await app.plugins.test(parts[2]);
      else if (
        parts[1] === "connections" &&
        parts.length === 4 &&
        method === "POST" &&
        parts[3] === "toggle"
      ) {
        app.config.toggle(parts[2], body.enabled, body.expectedRevision);
        data = app.plugins.settings();
      } else if (
        parts[1] === "projects" &&
        parts.length === 5 &&
        method === "POST" &&
        parts[3] === "runtime" &&
        parts[4] === "start"
      )
        data = app.runtimeStart(parts[2]);
      else if (
        parts[1] === "artifacts" &&
        parts.length === 3 &&
        method === "GET"
      )
        data = app.store.artifact(parts[2]);
      else if (url.pathname === "/api/backup" && method === "POST")
        data = await app.store.backup(body.name);
      else if (parts[1] === "fronts") {
        if (parts.length === 3 && method === "GET")
          data = await app.detail(frontId);
        else if (
          parts.length === 4 &&
          parts[3] === "entries" &&
          method === "POST"
        )
          data = app.entry(frontId, body);
        else if (
          parts.length === 4 &&
          parts[3] === "plans" &&
          method === "POST"
        )
          data = app.plan(frontId, body);
        else if (
          parts.length === 6 &&
          parts[3] === "plans" &&
          parts[5] === "approve" &&
          method === "POST"
        )
          data = app.approve(frontId, parts[4], body);
        else if (
          parts.length === 6 &&
          parts[3] === "waves" &&
          method === "POST"
        ) {
          if (parts[5] === "integrate")
            data = await app.integrateWave(frontId, parts[4], body);
          else if (parts[5] === "withdraw")
            data = app.withdrawWave(frontId, parts[4], body);
          else if (parts[5] === "dispatch")
            data = await app.dispatch(frontId, parts[4], body);
          else if (parts[5] === "close")
            data = await app.closeWave(frontId, parts[4], body);
          else if (parts[5] === "continue")
            data = await app.continueWave(frontId, parts[4], body);
          else if (parts[5] === "review")
            data = await app.review(frontId, parts[4], body);
          else throw new AppError("route_not_found", 404);
        } else if (
          parts.length === 4 &&
          parts[3] === "sessions" &&
          method === "POST"
        )
          data = await app.startSession(frontId, body);
        else if (
          parts.length === 5 &&
          parts[3] === "sessions" &&
          method === "GET"
        )
          data = await app.observeSession(
            frontId,
            parts[4],
            url.searchParams.get("cursor"),
          );
        else if (
          parts.length === 6 &&
          parts[3] === "sessions" &&
          parts[5] === "messages" &&
          method === "POST"
        )
          data = await app.sendSession(frontId, parts[4], body);
        else if (
          parts.length === 4 &&
          parts[3] === "artifacts" &&
          method === "POST"
        )
          data = app.addArtifact(frontId, body);
        else if (
          parts.length === 5 &&
          parts[3] === "artifacts" &&
          parts[4] === "import" &&
          method === "POST"
        )
          data = app.importArtifact(frontId, body);
        else if (
          parts.length === 6 &&
          parts[3] === "operations" &&
          parts[5] === "reconcile" &&
          method === "POST"
        )
          data = await app.reconcile(frontId, parts[4]);
        else throw new AppError("route_not_found", 404);
      } else throw new AppError("route_not_found", 404);
      send(200, { data });
    } catch (error) {
      send(error instanceof AppError ? error.status : 500, {
        error: {
          code: errorCode(error),
          message: errorCode(error).replaceAll("_", " "),
        },
      });
    }
  });
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  const port = Number(process.env.MACROSS_PORT || 4280);
  check(
    Number.isInteger(port) && port >= 1024 && port <= 65535,
    "invalid_port",
  );
  const app = createApplication({
      stateRoot:
        process.env.MACROSS_STATE_ROOT ||
        path.resolve(
          path.dirname(fileURLToPath(import.meta.url)),
          "../../state",
        ),
    }),
    server = createServer(app);
  server.listen(port, "127.0.0.1", () =>
    process.stdout.write(`Macross: http://127.0.0.1:${port}\n`),
  );
  const close = () =>
    server.close(() => {
      app.close();
      process.exit(0);
    });
  process.on("SIGINT", close);
  process.on("SIGTERM", close);
}
