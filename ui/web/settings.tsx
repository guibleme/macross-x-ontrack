import { useEffect, useState } from "react";
import {
  api,
  label,
  timestamp,
  type Settings,
  type Project,
  type Connection,
  type Plugin,
  type Agent,
} from "./api";
import { useDraftValue, neverDirty } from "./drafts";
type Tab = "projects" | "connections" | "agents";
const id = () => crypto.randomUUID();
const health = (connection: Connection) =>
  typeof connection.health === "string"
    ? connection.health
    : connection.health?.status ||
      (connection.enabled ? "unknown" : "disabled");
export function SettingsView({
  settings,
  onRefresh,
  onError,
}: {
  settings: Settings;
  onRefresh: () => Promise<void>;
  onError: (message: string) => void;
}) {
  const [tab, setTab] = useState<Tab>("projects");
  const [draft, setDraft] = useDraftValue<Settings>(
    "settings:configuration",
    settings,
    neverDirty,
  );
  const [plugins, setPlugins] = useState<Plugin[]>([]);
  const [busy, setBusy] = useState(false);
  const [dirty, setDirty] = useDraftValue(
    "settings:dirty",
    false,
    (value) => value,
  );
  const [notice, setNotice] = useState("");
  const [testing, setTesting] = useState("");
  const [agentProject, setAgentProject] = useState(
    settings.projects[0]?.id || "",
  );
  const [agents, setAgents] = useState<Agent[]>([]);
  useEffect(() => {
    if (!dirty) setDraft(settings);
  }, [settings, dirty]);
  useEffect(() => {
    api<Plugin[]>("/api/plugins")
      .then(setPlugins)
      .catch((e) => onError(e.message));
  }, [onError]);
  useEffect(() => {
    if (!agentProject && settings.projects.length)
      setAgentProject(settings.projects[0].id);
  }, [agentProject, settings.projects]);
  const refreshAgents = async () => {
    if (!agentProject) return;
    setBusy(true);
    setNotice("");
    try {
      setAgents(
        await api<Agent[]>(
          `/api/agents?projectId=${encodeURIComponent(agentProject)}`,
        ),
      );
    } catch (e) {
      onError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };
  useEffect(() => {
    if (tab === "agents" && agentProject) void refreshAgents();
  }, [tab, agentProject]);
  const change = (value: Settings) => {
    setDraft(value);
    setDirty(true);
    setNotice("");
  };
  const changeProject = (projectId: string, patch: Partial<Project>) =>
    change({
      ...draft,
      projects: draft.projects.map((p) =>
        p.id === projectId ? { ...p, ...patch } : p,
      ),
    });
  const changeConnection = (connectionId: string, patch: Partial<Connection>) =>
    change({
      ...draft,
      connections: draft.connections.map((c) =>
        c.id === connectionId ? { ...c, ...patch } : c,
      ),
    });
  const save = async () => {
    setBusy(true);
    setNotice("");
    try {
      if (
        draft.projects.some(
          (project) => !project.name.trim() || !project.repo.trim(),
        )
      )
        throw new Error(
          "Give each project a name and an absolute repository path.",
        );
      if (draft.connections.some((connection) => !connection.name.trim()))
        throw new Error("Give each connection a name.");
      const payload = {
        ...draft,
        projects: draft.projects.map((project) => ({
          ...project,
          name: project.name.trim(),
          repo: project.repo.trim(),
          documentRoots: project.documentRoots
            .map((root) => root.trim())
            .filter(Boolean),
        })),
        connections: draft.connections.map(
          ({ health: _health, lastGoodAt: _lastGoodAt, ...connection }) =>
            connection,
        ),
      };
      const saved = await api<Settings>("/api/settings", payload, "PUT");
      setDraft(saved);
      setDirty(false);
      await onRefresh();
      setNotice(
        "Settings saved. Enabling a connection does not dispatch work.",
      );
    } catch (e) {
      onError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };
  const toggle = async (connection: Connection) => {
    if (dirty) {
      setNotice("Save or discard your changes before changing a connection.");
      return;
    }
    setTesting(connection.id);
    setNotice("");
    try {
      await api(
        `/api/connections/${encodeURIComponent(connection.id)}/toggle`,
        { enabled: !connection.enabled, expectedRevision: connection.revision },
      );
      await onRefresh();
      setNotice(
        connection.enabled
          ? "Connection disabled. Existing execution continues in its engine."
          : "Connection enabled for observation. Execution still requires an explicit command.",
      );
    } catch (e) {
      onError((e as Error).message);
    } finally {
      setTesting("");
    }
  };
  const testConnection = async (connection: Connection) => {
    if (dirty) {
      setNotice("Save your changes before testing the connection.");
      return;
    }
    setTesting(connection.id);
    setNotice("");
    try {
      const result = await api<{
        status?: string;
        health?: string;
        detail?: string;
      }>(`/api/connections/${encodeURIComponent(connection.id)}/test`, {});
      await onRefresh();
      setNotice(
        result.detail ||
          `Connection check: ${label(result.status || result.health || "completed")}.`,
      );
    } catch (e) {
      onError((e as Error).message);
    } finally {
      setTesting("");
    }
  };
  const startRuntime = async (project: Project) => {
    setTesting(project.id);
    setNotice("");
    try {
      await api(
        `/api/projects/${encodeURIComponent(project.id)}/runtime/start`,
        {},
      );
      setNotice(
        `Runtime started for ${project.name}. Discover agents in the Agents tab.`,
      );
      await onRefresh();
    } catch (e) {
      onError((e as Error).message);
    } finally {
      setTesting("");
    }
  };
  return (
    <section className="settings-page">
      <div className="section-heading settings-heading">
        <div>
          <p className="eyebrow">Workspace configuration</p>
          <h1>Connect deliberately.</h1>
          <p className="muted">
            Projects set the boundary. Connections provide capabilities. You
            choose when work starts.
          </p>
        </div>
        <div className="actions">
          {dirty && (
            <button
              disabled={busy}
              onClick={() => {
                setDraft(settings);
                setDirty(false);
                setNotice("Changes discarded.");
              }}
            >
              Discard changes
            </button>
          )}
          <button
            className="primary"
            disabled={!dirty || busy}
            onClick={() => void save()}
          >
            {busy ? "Saving…" : "Save settings"}
          </button>
        </div>
      </div>
      <div className="tabs" role="tablist" aria-label="Configuration areas">
        {(["projects", "connections", "agents"] as Tab[]).map((value) => (
          <button
            role="tab"
            key={value}
            aria-selected={tab === value}
            aria-controls={`settings-${value}`}
            id={`settings-tab-${value}`}
            onClick={() => setTab(value)}
          >
            {value[0].toUpperCase() + value.slice(1)}
          </button>
        ))}
      </div>
      {notice && (
        <p className="notice" role="status">
          {notice}
        </p>
      )}
      <div
        role="tabpanel"
        id={`settings-${tab}`}
        aria-labelledby={`settings-tab-${tab}`}
      >
        {tab === "projects" && (
          <>
            <div className="section-heading">
              <div>
                <h2>Projects</h2>
                <p className="muted">
                  Each project has its own repository, allowed documents and
                  execution connection.
                </p>
              </div>
              <button
                onClick={() =>
                  change({
                    ...draft,
                    projects: [
                      ...draft.projects,
                      {
                        id: id(),
                        name: "",
                        repo: "",
                        documentRoots: [],
                        executorId: "",
                      },
                    ],
                  })
                }
              >
                + Add project
              </button>
            </div>
            {!draft.projects.length && (
              <div className="empty-inline">
                <h3>Start with one project.</h3>
                <p>
                  Add a repository you control. No source or credential is
                  selected automatically.
                </p>
                <button
                  className="primary"
                  onClick={() =>
                    change({
                      ...draft,
                      projects: [
                        {
                          id: id(),
                          name: "",
                          repo: "",
                          documentRoots: [],
                          executorId: "",
                        },
                      ],
                    })
                  }
                >
                  Add project
                </button>
              </div>
            )}
            {draft.projects.map((project, index) => (
              <fieldset className="settings-editor" key={project.id}>
                <legend>{project.name || `New project ${index + 1}`}</legend>
                <div className="field-grid">
                  <label>
                    Project name
                    <input
                      required
                      value={project.name}
                      maxLength={160}
                      onChange={(e) =>
                        changeProject(project.id, { name: e.target.value })
                      }
                      placeholder="A name you recognize"
                    />
                  </label>
                  <label>
                    Repository path
                    <input
                      required
                      value={project.repo}
                      onChange={(e) =>
                        changeProject(project.id, { repo: e.target.value })
                      }
                      placeholder="Absolute path to the repository"
                    />
                  </label>
                  <label className="full-width">
                    Allowed document folders
                    <textarea
                      rows={3}
                      value={project.documentRoots.join("\n")}
                      onChange={(e) =>
                        changeProject(project.id, {
                          documentRoots: e.target.value.split("\n"),
                        })
                      }
                      placeholder="One absolute folder path per line"
                    />
                    <span className="field-hint">
                      Local document imports are limited to these folders.
                    </span>
                  </label>
                  <label>
                    Execution connection
                    <select
                      value={project.executorId || ""}
                      onChange={(e) =>
                        changeProject(project.id, {
                          executorId: e.target.value,
                        })
                      }
                    >
                      <option value="">No executor selected</option>
                      {draft.connections
                        .filter(
                          (c) =>
                            c.projectId === project.id &&
                            plugins.find((plugin) => plugin.id === c.pluginId)
                              ?.kind === "executor",
                        )
                        .map((c) => (
                          <option key={c.id} value={c.id}>
                            {c.name || c.id}
                          </option>
                        ))}
                    </select>
                  </label>
                </div>
                <div className="actions">
                  <button
                    disabled={
                      dirty ||
                      testing === project.id ||
                      !draft.connections.some(
                        (connection) =>
                          connection.id === project.executorId &&
                          connection.enabled,
                      )
                    }
                    onClick={() => void startRuntime(project)}
                  >
                    {testing === project.id ? "Starting…" : "Start runtime"}
                  </button>
                  <span className="field-hint">
                    {!project.executorId
                      ? "Choose an execution connection first."
                      : !draft.connections.some(
                            (connection) =>
                              connection.id === project.executorId &&
                              connection.enabled,
                          )
                        ? "Enable the selected execution connection first."
                        : "Starts the shared engine. It does not dispatch a wave."}
                  </span>
                </div>
              </fieldset>
            ))}
          </>
        )}
        {tab === "connections" && (
          <>
            <div className="section-heading">
              <div>
                <h2>Connections</h2>
                <p className="muted">
                  Optional and project scoped. A disconnected source is not a
                  stopped execution.
                </p>
              </div>
              <button
                disabled={!draft.projects.length || !plugins.length}
                onClick={() =>
                  change({
                    ...draft,
                    connections: [
                      ...draft.connections,
                      {
                        id: id(),
                        pluginId: plugins[0]?.id || "",
                        name: "",
                        projectId: draft.projects[0]?.id || "",
                        enabled: false,
                        config: {},
                        revision: 0,
                      },
                    ],
                  })
                }
              >
                + Add connection
              </button>
            </div>
            {!draft.connections.length && (
              <div className="empty-inline">
                <h3>Only connect what this project needs.</h3>
                <p>
                  Add a project first, then choose an available connector.
                  External services can be added in later connector releases.
                </p>
              </div>
            )}
            {draft.connections.map((connection, index) => {
              const plugin = plugins.find((p) => p.id === connection.pluginId);
              return (
                <fieldset className="settings-editor" key={connection.id}>
                  <legend>
                    {connection.name || `New connection ${index + 1}`}
                  </legend>
                  <div className="connection-top">
                    <span className={`pill state-${health(connection)}`}>
                      {label(health(connection))}
                    </span>
                    <span className="field-hint">
                      {connection.lastGoodAt
                        ? `Last successful read ${timestamp(connection.lastGoodAt)}`
                        : "No successful read recorded"}
                    </span>
                  </div>
                  <div className="field-grid">
                    <label>
                      Connection name
                      <input
                        value={connection.name}
                        maxLength={160}
                        onChange={(e) =>
                          changeConnection(connection.id, {
                            name: e.target.value,
                          })
                        }
                        placeholder="A name for this connection"
                      />
                    </label>
                    <label>
                      Connector
                      <select
                        value={connection.pluginId}
                        onChange={(e) =>
                          changeConnection(connection.id, {
                            pluginId: e.target.value,
                            config: {},
                          })
                        }
                      >
                        {plugins.map((p) => (
                          <option key={p.id} value={p.id}>
                            {p.name}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label>
                      Project
                      <select
                        value={connection.projectId}
                        onChange={(e) =>
                          changeConnection(connection.id, {
                            projectId: e.target.value,
                          })
                        }
                      >
                        {draft.projects.map((p) => (
                          <option key={p.id} value={p.id}>
                            {p.name || "Unnamed project"}
                          </option>
                        ))}
                      </select>
                    </label>
                    {(plugin?.fields || plugin?.configFields || []).map(
                      (field) => (
                        <label key={field.key}>
                          {field.label}
                          <input
                            required={field.required}
                            value={String(connection.config[field.key] || "")}
                            onChange={(e) =>
                              changeConnection(connection.id, {
                                config: {
                                  ...connection.config,
                                  [field.key]: e.target.value,
                                },
                              })
                            }
                          />
                        </label>
                      ),
                    )}
                  </div>
                  {plugin?.capabilities && (
                    <p className="capabilities">
                      {plugin.capabilities.map((capability) => (
                        <span key={capability}>{label(capability)}</span>
                      ))}
                    </p>
                  )}
                  <div className="actions">
                    <button
                      disabled={dirty || testing === connection.id}
                      onClick={() => void testConnection(connection)}
                    >
                      {testing === connection.id
                        ? "Checking…"
                        : "Test connection"}
                    </button>
                    <button
                      className={connection.enabled ? "" : "primary"}
                      disabled={dirty || testing === connection.id}
                      onClick={() => void toggle(connection)}
                    >
                      {connection.enabled
                        ? "Disable connection"
                        : "Enable connection"}
                    </button>
                  </div>
                  <p className="field-hint">
                    Testing reads connection health. Disabling preserves
                    evidence and does not stop accepted work.
                  </p>
                </fieldset>
              );
            })}
          </>
        )}
        {tab === "agents" && (
          <>
            <div className="section-heading">
              <div>
                <h2>Available agents</h2>
                <p className="muted">
                  Reported by the connected runtime. Selection happens
                  explicitly in a conversation or wave.
                </p>
              </div>
              <div className="actions">
                <label>
                  <span className="sr-only">Project for agent discovery</span>
                  <select
                    value={agentProject}
                    onChange={(e) => setAgentProject(e.target.value)}
                  >
                    <option value="">Choose project</option>
                    {settings.projects.map((project) => (
                      <option key={project.id} value={project.id}>
                        {project.name}
                      </option>
                    ))}
                  </select>
                </label>
                <button
                  disabled={!agentProject || busy}
                  onClick={() => void refreshAgents()}
                >
                  {busy ? "Discovering…" : "Refresh agents"}
                </button>
              </div>
            </div>
            {!agents.length ? (
              <div className="empty-inline">
                <h3>No agents reported yet.</h3>
                <p>
                  Save a project and its execution connection. Enable the
                  connection, then start the runtime from Projects.
                </p>
              </div>
            ) : (
              <div
                className="agent-table"
                role="table"
                aria-label="Runtime agents"
              >
                <div className="agent-table-heading" role="row">
                  <span role="columnheader">Agent</span>
                  <span role="columnheader">Availability</span>
                  <span role="columnheader">Capabilities</span>
                </div>
                {agents.map((agent) => (
                  <div className="agent-table-row" role="row" key={agent.id}>
                    <span role="cell">
                      <strong>{agent.name || agent.id}</strong>
                      <small className="mono">{agent.id}</small>
                      {agent.verification ===
                        "discovered-not-live-verified" && (
                        <small className="muted">
                          Detected · live compatibility unverified
                        </small>
                      )}
                    </span>
                    <span
                      role="cell"
                      className={`state-${agent.available ? "available" : "unavailable"}`}
                    >
                      {agent.available ? "Available" : "Unavailable"}
                    </span>
                    <span role="cell">
                      {agent.capabilities?.map(label).join(", ") ||
                        "No capabilities advertised"}
                    </span>
                  </div>
                ))}
              </div>
            )}
            <p className="field-hint">
              A detected agent is not a claim that every operation has been
              validated. Unsupported operations stay unavailable.
            </p>
          </>
        )}
      </div>
      <div className="settings-bottom">
        <span className="mono">Configuration revision {settings.revision}</span>
        <button
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            try {
              const result = await api<{ path: string }>("/api/backup", {});
              setNotice(`Backup created: ${result.path}`);
            } catch (e) {
              onError((e as Error).message);
            } finally {
              setBusy(false);
            }
          }}
        >
          Create local backup
        </button>
      </div>
    </section>
  );
}
