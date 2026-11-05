import React, { useCallback, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  api,
  endpoint,
  operationId,
  label,
  timestamp,
  type Front,
  type FrontDetail,
  type Settings,
  type Agent,
  type Artifact,
  type Wave,
  type IntegrationRequest,
} from "./api";
import { SettingsView } from "./settings";
import "./styles.css";

import {
  DraftContext,
  useDraftValue,
  hasText,
  neverDirty,
  type DraftRecord,
} from "./drafts";

type Page = "fronts" | "settings";
type Tab = "conversation" | "plan" | "documents";
function Icon({
  name,
}: {
  name: "front" | "settings" | "plus" | "arrow" | "close" | "document";
}) {
  const paths = {
    front: "M4 4h16v12H9l-5 4V4Z",
    settings:
      "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM12 3v2m0 14v2M3 12h2m14 0h2M5.6 5.6 7 7m10 10 1.4 1.4M5.6 18.4 7 17M17 7l1.4-1.4",
    plus: "M12 5v14M5 12h14",
    arrow: "M5 12h14m-6-6 6 6-6 6",
    close: "m6 6 12 12M6 18 18 6",
    document: "M7 3h7l4 4v14H7V3Zm7 0v5h4M10 12h5m-5 4h5",
  };
  return (
    <svg
      width="18"
      height="18"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={paths[name]} />
    </svg>
  );
}
function ErrorNotice({
  error,
  onDismiss,
}: {
  error: string;
  onDismiss: () => void;
}) {
  return error ? (
    <div className="notice error" role="alert">
      <span>{error}</span>
      <button
        className="icon-button"
        aria-label="Dismiss error"
        onClick={onDismiss}
      >
        <Icon name="close" />
      </button>
    </div>
  ) : null;
}
function App() {
  const drafts = useRef(new Map<string, DraftRecord>());
  useEffect(() => {
    const protect = (event: BeforeUnloadEvent) => {
      if ([...drafts.current.values()].some((draft) => draft.dirty)) {
        event.preventDefault();
        event.returnValue = "";
      }
    };
    window.addEventListener("beforeunload", protect);
    return () => window.removeEventListener("beforeunload", protect);
  }, []);
  const [fronts, setFronts] = useState<Front[]>([]);
  const [settings, setSettings] = useState<Settings>({
    version: 1,
    revision: 0,
    projects: [],
    connections: [],
  });
  const [selected, setSelected] = useState("");
  const [detail, setDetail] = useState<FrontDetail | null>(null);
  const [page, setPage] = useState<Page>("fronts");
  const [tab, setTab] = useState<Tab>("conversation");
  const [creating, setCreating] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const refresh = useCallback(async () => {
    const [next, config] = await Promise.all([
      api<Front[]>("/api/fronts"),
      api<Settings>("/api/settings"),
    ]);
    setFronts(next);
    setSettings(config);
  }, []);
  const refreshFront = useCallback(async () => {
    if (!selected) {
      setDetail(null);
      return;
    }
    const next = await api<FrontDetail>(endpoint(selected));
    setDetail(next);
  }, [selected]);
  useEffect(() => {
    refresh()
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  }, [refresh]);
  useEffect(() => {
    let stopped = false;
    if (!selected) {
      setDetail(null);
      return;
    }
    setDetail(null);
    let inFlight = false;
    const load = async () => {
      if (stopped || inFlight) return;
      inFlight = true;
      try {
        const data = await api<FrontDetail>(endpoint(selected));
        if (!stopped) setDetail(data);
      } catch (e) {
        if (!stopped)
          setError(
            e instanceof Error ? e.message : "The workfront could not refresh.",
          );
      } finally {
        inFlight = false;
      }
    };
    void load();
    const timer = setInterval(load, 5000);
    return () => {
      stopped = true;
      clearInterval(timer);
    };
  }, [selected]);
  const mutate = async (work: () => Promise<unknown>) => {
    setError("");
    try {
      await work();
    } catch (e) {
      setError(
        e instanceof Error ? e.message : "The request could not be completed.",
      );
      throw e;
    }
    try {
      await Promise.all([refresh(), refreshFront()]);
    } catch {
      setError(
        "The operation was recorded, but the current view could not refresh. Do not repeat the operation.",
      );
    }
  };
  const project = settings.projects.find(
    (p) => p.id === detail?.front.projectId,
  );
  return (
    <DraftContext.Provider value={drafts.current}>
      <div className="app-shell">
        <a href="#main" className="skip-link">
          Skip to workspace
        </a>
        <aside className="rail" aria-label="Main navigation">
          <div className="wordmark">
            <span className="mark" aria-hidden="true">
              m<span>×</span>
            </span>
            <div>
              macross<span className="wordmark-sub">ONTRACK</span>
            </div>
          </div>
          <nav className="primary-nav">
            <button
              className={page === "fronts" ? "nav-item active" : "nav-item"}
              onClick={() => setPage("fronts")}
            >
              <Icon name="front" /> Workfronts{" "}
              <span className="nav-count">{fronts.length}</span>
            </button>
            <button
              className={page === "settings" ? "nav-item active" : "nav-item"}
              onClick={() => setPage("settings")}
            >
              <Icon name="settings" /> Settings
            </button>
          </nav>
          <div className="rail-section">
            <span className="eyebrow">Your workfronts</span>
            <button
              className="icon-button"
              aria-label="Create a workfront"
              onClick={() => {
                setPage("fronts");
                setCreating(true);
              }}
            >
              <Icon name="plus" />
            </button>
          </div>
          <label className="search-label">
            <span className="sr-only">Find a workfront</span>
            <input
              className="rail-search"
              type="search"
              placeholder="Find a workfront…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </label>
          <div className="front-list">
            {fronts
              .filter((f) =>
                `${f.title} ${f.objective}`
                  .toLowerCase()
                  .includes(query.toLowerCase()),
              )
              .map((front) => (
                <button
                  key={front.id}
                  className={`front-link ${selected === front.id && page === "fronts" ? "selected" : ""}`}
                  onClick={() => {
                    setSelected(front.id);
                    setPage("fronts");
                    setCreating(false);
                  }}
                >
                  <span className="front-symbol" aria-hidden="true">
                    ◌
                  </span>
                  <span>
                    {front.title}
                    <small>
                      {settings.projects.find((p) => p.id === front.projectId)
                        ?.name || "Unassigned project"}
                    </small>
                  </span>
                </button>
              ))}
            {!fronts.length && !loading && (
              <p className="rail-empty">
                A place for the work,
                <br />
                from first idea to delivery.
              </p>
            )}
          </div>
          <div className="rail-footer">
            <span className="local-dot" /> Local workspace
            <span className="footer-caption">
              Your context stays with the front.
            </span>
          </div>
        </aside>
        <main id="main" className="workspace" tabIndex={-1}>
          <header className="topbar">
            <span>
              {page === "settings"
                ? "Workspace / Settings"
                : `Workfronts${detail ? ` / ${project?.name || "Project"}` : ""}`}
            </span>
            <span className="version-label">PIVOT / V1</span>
          </header>
          <ErrorNotice error={error} onDismiss={() => setError("")} />
          {page === "settings" ? (
            <SettingsView
              settings={settings}
              onRefresh={refresh}
              onError={setError}
            />
          ) : creating ? (
            <CreateFront
              settings={settings}
              onCancel={() => setCreating(false)}
              onSettings={() => {
                setCreating(false);
                setPage("settings");
              }}
              onCreate={async (input) => {
                const next = await api<Front>("/api/fronts", {
                  ...input,
                  requestId: operationId(),
                });
                await refresh();
                setSelected(next.id);
                setCreating(false);
              }}
            />
          ) : detail ? (
            <>
              <section className="front-header">
                <div>
                  <p className="eyebrow">Workfront</p>
                  <h1>{detail.front.title}</h1>
                  <p className="objective">{detail.front.objective}</p>
                </div>
                <div className="front-meta">
                  <span className="pill">{project?.name || "Project"}</span>
                  <small>Updated {timestamp(detail.front.updatedAt)}</small>
                </div>
              </section>
              <div className="tabs" role="tablist" aria-label="Workfront views">
                {(["conversation", "plan", "documents"] as Tab[]).map(
                  (item) => (
                    <button
                      role="tab"
                      id={`tab-${item}`}
                      aria-controls={`panel-${item}`}
                      aria-selected={tab === item}
                      key={item}
                      onClick={() => setTab(item)}
                    >
                      {item === "conversation"
                        ? "Conversation"
                        : item === "plan"
                          ? "Plan & waves"
                          : "Documents"}
                      <span>
                        {item === "conversation"
                          ? detail.entries.length
                          : item === "plan"
                            ? detail.waves.length
                            : detail.artifacts.length}
                      </span>
                    </button>
                  ),
                )}
              </div>
              <section
                role="tabpanel"
                id={`panel-${tab}`}
                aria-labelledby={`tab-${tab}`}
                className="front-body"
              >
                {tab === "conversation" && (
                  <Conversation
                    key={detail.front.id}
                    detail={detail}
                    mutate={mutate}
                  />
                )}
                {tab === "plan" && (
                  <PlanView
                    key={detail.front.id}
                    detail={detail}
                    mutate={mutate}
                  />
                )}
                {tab === "documents" && (
                  <Documents
                    key={detail.front.id}
                    detail={detail}
                    mutate={mutate}
                  />
                )}
              </section>
            </>
          ) : loading || selected ? (
            <div className="loading-state" aria-busy="true">
              <div className="skeleton wide" />
              <div className="skeleton" />
              <div className="skeleton" />
            </div>
          ) : (
            <section className="welcome">
              <p className="eyebrow">A clear place to continue</p>
              <h1>
                Keep the whole
                <br />
                thread of the work.
              </h1>
              <p>
                Bring the conversation, evidence and next step into one
                workfront. Start with an idea. Add execution when the plan is
                ready.
              </p>
              <button className="primary" onClick={() => setCreating(true)}>
                Create a workfront <Icon name="arrow" />
              </button>
              <div className="welcome-note">
                <span>01</span>
                <p>
                  Choose a project in Settings. Your sources and agents connect
                  explicitly.
                </p>
              </div>
              <div className="welcome-note">
                <span>02</span>
                <p>
                  Keep decisions and full documents together, even when agents
                  change.
                </p>
              </div>
              <div className="welcome-note">
                <span>03</span>
                <p>
                  Approve a plan, dispatch a wave and review the exact result.
                </p>
              </div>
            </section>
          )}
        </main>
      </div>
    </DraftContext.Provider>
  );
}
function CreateFront({
  settings,
  onCancel,
  onCreate,
  onSettings,
}: {
  settings: Settings;
  onCancel: () => void;
  onSettings: () => void;
  onCreate: (input: {
    title: string;
    objective: string;
    projectId: string;
  }) => Promise<void>;
}) {
  const [title, setTitle] = useDraftValue<string>(
    "new-front:title",
    "",
    hasText,
  );
  const [objective, setObjective] = useDraftValue<string>(
    "new-front:objective",
    "",
    hasText,
  );
  const [projectId, setProjectId] = useDraftValue<string>(
    "new-front:project",
    settings.projects[0]?.id || "",
    neverDirty,
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  return (
    <section className="editor-page">
      <p className="eyebrow">New workfront</p>
      <h1>What are we working on?</h1>
      <p className="muted">
        A workfront can start before a task, branch or agent session exists.
      </p>
      {!settings.projects.length ? (
        <div className="empty-inline">
          <h2>Add a project first</h2>
          <p>
            A project defines the repository and the sources this workfront can
            use.
          </p>
          <button className="primary" onClick={onSettings}>
            Open project settings
          </button>
        </div>
      ) : (
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            setBusy(true);
            setError("");
            try {
              await onCreate({
                title: title.trim(),
                objective: objective.trim(),
                projectId,
              });
              setTitle("");
              setObjective("");
            } catch (e) {
              setError((e as Error).message);
            } finally {
              setBusy(false);
            }
          }}
          className="form-stack"
        >
          <label>
            Title
            <input
              autoFocus
              required
              maxLength={200}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="A short, specific name"
            />
          </label>
          <label>
            Objective
            <textarea
              required
              rows={4}
              maxLength={8000}
              value={objective}
              onChange={(e) => setObjective(e.target.value)}
              placeholder="What should be different when this work is done?"
            />
          </label>
          <label>
            Project
            <select
              value={projectId}
              onChange={(e) => setProjectId(e.target.value)}
            >
              {settings.projects.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
          {error && (
            <p className="inline-error" role="alert">
              {error}
            </p>
          )}
          <div className="actions">
            <button className="primary" disabled={busy}>
              {busy ? "Creating…" : "Create workfront"}
            </button>
            <button type="button" onClick={onCancel}>
              Cancel
            </button>
          </div>
        </form>
      )}
    </section>
  );
}
function useAgents(projectId: string) {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [error, setError] = useState("");
  useEffect(() => {
    let stopped = false;
    api<Agent[]>(`/api/agents?projectId=${encodeURIComponent(projectId)}`)
      .then((data) => {
        if (!stopped) setAgents(data);
      })
      .catch((e) => {
        if (!stopped) setError(e.message);
      });
    return () => {
      stopped = true;
    };
  }, [projectId]);
  return { agents, error };
}
function Conversation({
  detail,
  mutate,
}: {
  detail: FrontDetail;
  mutate: (work: () => Promise<unknown>) => Promise<void>;
}) {
  const { agents, error: agentsError } = useAgents(detail.front.projectId);
  const prefix = `front:${detail.front.id}:conversation:`;
  const [agentId, setAgentId] = useDraftValue<string>(
    prefix + "agent",
    "",
    neverDirty,
  );
  const [text, setText] = useDraftValue<string>(prefix + "text", "", hasText);
  const [kind, setKind] = useDraftValue<string>(
    prefix + "kind",
    "note",
    neverDirty,
  );
  const [busy, setBusy] = useState(false);
  const [sendToAgent, setSendToAgent] = useDraftValue(
    prefix + "send",
    false,
    neverDirty,
  );
  const end = useRef<HTMLDivElement>(null);
  const session = [...detail.sessions]
    .reverse()
    .find((s) => s.agentId === agentId);
  const sessionPending =
    detail.operations?.some(
      (operation) =>
        operation.kind.startsWith("session.") &&
        ["recorded", "dispatched", "unknown"].includes(operation.state),
    ) || false;
  const observedSession = session || detail.sessions.at(-1);
  const [observationError, setObservationError] = useState("");
  useEffect(() => {
    if (!observedSession) return;
    let stopped = false;
    let inFlight = false;
    const observe = async () => {
      if (stopped || inFlight) return;
      inFlight = true;
      try {
        await api(
          endpoint(
            detail.front.id,
            `/sessions/${encodeURIComponent(observedSession.id)}`,
          ),
        );
        if (!stopped) setObservationError("");
      } catch (e) {
        if (!stopped)
          setObservationError(
            e instanceof Error
              ? e.message
              : "The agent session could not refresh.",
          );
      } finally {
        inFlight = false;
      }
    };
    void observe();
    const timer = setInterval(observe, 5000);
    return () => {
      stopped = true;
      clearInterval(timer);
    };
  }, [detail.front.id, observedSession?.id]);
  return (
    <div className="conversation-layout">
      <div className="conversation-main">
        <div className="section-heading">
          <div>
            <h2>Conversation</h2>
            <p className="muted">
              Decisions and progress stay with this front.
            </p>
          </div>
          <span className="subtle-label">{detail.entries.length} entries</span>
        </div>
        {!detail.entries.length ? (
          <div className="empty-conversation">
            <span className="orbit" aria-hidden="true">
              ◌
            </span>
            <h3>Start with the question.</h3>
            <p>
              Record a note, capture a decision or ask a connected agent. The
              history stays here.
            </p>
          </div>
        ) : (
          <ol className="timeline">
            {detail.entries.map((entry) => (
              <li key={entry.id} className={`entry entry-${entry.kind}`}>
                <div className="entry-heading">
                  <span className="entry-kind">{label(entry.kind)}</span>
                  <time>{timestamp(entry.at)}</time>
                </div>
                <p className="entry-text">{entry.text}</p>
              </li>
            ))}
          </ol>
        )}
        {observationError && (
          <p className="field-hint" role="status">
            Agent output is not current: {observationError}. The recorded
            conversation remains available.
          </p>
        )}
        <OperationNotices detail={detail} mutate={mutate} />
        <div ref={end} />
        <form
          className="composer"
          onSubmit={async (e) => {
            e.preventDefault();
            setBusy(true);
            try {
              await mutate(async () => {
                if (sendToAgent) {
                  if (!agentId)
                    throw new Error("Choose an available agent first.");
                  if (session)
                    return api(
                      endpoint(
                        detail.front.id,
                        `/sessions/${encodeURIComponent(session.id)}/messages`,
                      ),
                      { text: text.trim(), requestId: operationId() },
                    );
                  return api(endpoint(detail.front.id, "/sessions"), {
                    agentId,
                    input: text.trim(),
                    requestId: operationId(),
                  });
                }
                return api(endpoint(detail.front.id, "/entries"), {
                  text: text.trim(),
                  kind,
                  expectedRevision: detail.front.revision,
                  requestId: operationId(),
                });
              });
              setText("");
            } catch {
              /* The page owns the visible error. Keep the draft. */
            } finally {
              setBusy(false);
            }
          }}
        >
          <label className="sr-only" htmlFor="front-message">
            Message or workfront note
          </label>
          <textarea
            id="front-message"
            rows={4}
            maxLength={16000}
            required
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={
              sendToAgent
                ? "Ask the selected agent about this workfront…"
                : "Record context, a decision or the next step…"
            }
          />
          <div className="composer-tools">
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={sendToAgent}
                onChange={(e) => setSendToAgent(e.target.checked)}
              />
              Send to agent
            </label>
            {sendToAgent ? (
              <label className="sr-select">
                <span className="sr-only">Agent for this message</span>
                <select
                  required
                  value={agentId}
                  onChange={(e) => setAgentId(e.target.value)}
                >
                  <option value="">Choose an agent</option>
                  {agents.map((a) => (
                    <option disabled={!a.available} value={a.id} key={a.id}>
                      {a.name || a.id}
                      {!a.available ? " · unavailable" : ""}
                    </option>
                  ))}
                </select>
              </label>
            ) : (
              <label className="sr-select">
                <span className="sr-only">Entry type</span>
                <select value={kind} onChange={(e) => setKind(e.target.value)}>
                  <option value="note">Note</option>
                  <option value="decision">Decision</option>
                  <option value="summary">Summary</option>
                </select>
              </label>
            )}
            <button
              className="primary"
              disabled={
                busy ||
                !text.trim() ||
                (sendToAgent && (!agentId || sessionPending))
              }
            >
              {busy ? "Sending…" : sendToAgent ? "Send message" : "Add entry"}
              <Icon name="arrow" />
            </button>
          </div>
          {sendToAgent && sessionPending && (
            <p className="field-hint" role="status">
              A prior session command needs confirmation before another message
              can be sent.
            </p>
          )}
          {sendToAgent && (
            <p className="field-hint">
              {session
                ? "Continuing the selected agent’s linked session."
                : "A new session receives the selected workfront context. No task or worktree is created."}
            </p>
          )}
        </form>
      </div>
      <aside className="context-panel">
        <p className="eyebrow">Context</p>
        <h3>
          One front.
          <br />
          Many sessions.
        </h3>
        <p>
          Changing agents preserves the workfront history. Native sessions stay
          with their provider.
        </p>
        <dl className="context-facts">
          <div>
            <dt>Linked sessions</dt>
            <dd>{detail.sessions.length}</dd>
          </div>
          <div>
            <dt>Documents</dt>
            <dd>{detail.artifacts.length}</dd>
          </div>
          <div>
            <dt>Planned waves</dt>
            <dd>{detail.waves.length}</dd>
          </div>
        </dl>
        {agentsError && (
          <p className="field-hint">Agents unavailable: {agentsError}</p>
        )}
        {!agents.length && (
          <p className="field-hint">
            Connect an executor and start its runtime in Settings to discover
            agents. Notes work without an agent.
          </p>
        )}
      </aside>
    </div>
  );
}
function OperationNotices({
  detail,
  mutate,
}: {
  detail: FrontDetail;
  mutate: (work: () => Promise<unknown>) => Promise<void>;
}) {
  const [busy, setBusy] = useState("");
  return (
    <>
      {detail.operations
        ?.filter((operation) =>
          ["recorded", "unknown", "dispatched"].includes(operation.state),
        )
        .map((operation) => {
          const id = operation.id || operation.requestId || "";
          return (
            <div className="notice operation-notice" key={id}>
              <span>
                <strong>
                  {label(operation.kind)} · {label(operation.state)}
                </strong>
                <br />
                The outcome needs confirmation. Do not repeat the original
                action.
              </span>
              {operation.state !== "dispatched" && (
                <button
                  disabled={Boolean(busy)}
                  onClick={async () => {
                    setBusy(id);
                    try {
                      await mutate(() =>
                        api(
                          endpoint(
                            detail.front.id,
                            `/operations/${encodeURIComponent(id)}/reconcile`,
                          ),
                          {},
                        ),
                      );
                    } catch {
                      /* Preserve the original operation. The page reports the failure. */
                    } finally {
                      setBusy("");
                    }
                  }}
                >
                  {busy === id ? "Reconciling…" : "Reconcile outcome"}
                </button>
              )}
            </div>
          );
        })}
    </>
  );
}
interface WaveDraft {
  id: string;
  title: string;
  criteria: string;
  dependsOn: string[];
}
function PlanView({
  detail,
  mutate,
}: {
  detail: FrontDetail;
  mutate: (work: () => Promise<unknown>) => Promise<void>;
}) {
  const prefix = `front:${detail.front.id}:plan:`;
  const [editing, setEditing] = useDraftValue(
    prefix + "editing",
    false,
    neverDirty,
  );
  const [title, setTitle] = useDraftValue<string>(
    prefix + "title",
    "",
    hasText,
  );
  const [drafts, setDrafts] = useDraftValue<WaveDraft[]>(
    prefix + "waves",
    [{ id: "wave-1", title: "", criteria: "", dependsOn: [] }],
    (values) =>
      values.some((w) =>
        Boolean(w.title.trim() || w.criteria.trim() || w.dependsOn.length),
      ),
  );
  const [busy, setBusy] = useState(false);
  const { agents } = useAgents(detail.front.projectId);
  const [agentId, setAgentId] = useState("");
  const updateDraft = (index: number, patch: Partial<WaveDraft>) =>
    setDrafts((current) =>
      current.map((wave, i) => (i === index ? { ...wave, ...patch } : wave)),
    );
  return (
    <div className="plan-layout">
      <div className="section-heading">
        <div>
          <h2>Plan & waves</h2>
          <p className="muted">
            Approve the scope. Dispatch each wave explicitly.
          </p>
        </div>
        <button onClick={() => setEditing(!editing)}>
          {editing ? "Close draft" : "+ New plan revision"}
        </button>
      </div>
      {editing && (
        <form
          className="plan-editor form-stack"
          onSubmit={async (e) => {
            e.preventDefault();
            setBusy(true);
            try {
              await mutate(() =>
                api(endpoint(detail.front.id, "/plans"), {
                  title: title.trim(),
                  expectedRevision: detail.front.revision,
                  requestId: operationId(),
                  waves: drafts.map((w) => ({
                    ...w,
                    title: w.title.trim(),
                    criteria: w.criteria
                      .split("\n")
                      .map((s) => s.trim())
                      .filter(Boolean),
                  })),
                }),
              );
              setEditing(false);
              setTitle("");
              setDrafts([
                { id: "wave-1", title: "", criteria: "", dependsOn: [] },
              ]);
            } catch {
              /* Preserve the draft after rejection. */
            } finally {
              setBusy(false);
            }
          }}
        >
          <label>
            Plan title
            <input
              required
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              maxLength={200}
              placeholder="The bounded outcome for this plan"
            />
          </label>
          {drafts.map((wave, index) => (
            <fieldset className="wave-draft" key={wave.id}>
              <legend>Wave {index + 1}</legend>
              <label>
                Delivery title
                <input
                  required
                  value={wave.title}
                  onChange={(e) =>
                    updateDraft(index, { title: e.target.value })
                  }
                  maxLength={200}
                />
              </label>
              <label>
                Acceptance criteria
                <textarea
                  required
                  rows={3}
                  placeholder="One observable criterion per line"
                  value={wave.criteria}
                  onChange={(e) =>
                    updateDraft(index, { criteria: e.target.value })
                  }
                  maxLength={8000}
                />
              </label>
              {drafts.length > 1 && (
                <div className="dependency-picker">
                  <span className="field-label">Depends on</span>
                  {drafts
                    .filter((other) => other.id !== wave.id)
                    .map((other) => (
                      <label className="checkbox-label" key={other.id}>
                        <input
                          type="checkbox"
                          checked={wave.dependsOn.includes(other.id)}
                          onChange={(e) =>
                            updateDraft(index, {
                              dependsOn: e.target.checked
                                ? [...wave.dependsOn, other.id]
                                : wave.dependsOn.filter(
                                    (id) => id !== other.id,
                                  ),
                            })
                          }
                        />
                        {other.title || other.id}
                      </label>
                    ))}
                </div>
              )}
            </fieldset>
          ))}
          <div className="actions">
            <button
              type="button"
              onClick={() =>
                setDrafts((v) => [
                  ...v,
                  {
                    id: `wave-${v.length + 1}`,
                    title: "",
                    criteria: "",
                    dependsOn: [],
                  },
                ])
              }
            >
              + Add wave
            </button>
            <button className="primary" disabled={busy}>
              {busy ? "Saving…" : "Save draft plan"}
            </button>
          </div>
        </form>
      )}
      {!detail.plans.length && !editing && (
        <div className="empty-inline">
          <h3>A plan gives the work a boundary.</h3>
          <p>
            Define multiple waves, their acceptance criteria and their
            dependencies. Saving or approving a plan does not start execution.
          </p>
          <button className="primary" onClick={() => setEditing(true)}>
            Create a plan
          </button>
        </div>
      )}
      <OperationNotices detail={detail} mutate={mutate} />
      {detail.plans.map((plan) => (
        <section className="plan-section" key={plan.id}>
          <div className="plan-heading">
            <div>
              <span className="eyebrow">Revision {plan.revision}</span>
              <h3>{plan.title}</h3>
            </div>
            <div className="actions">
              <span className={`pill state-${plan.status}`}>
                {label(plan.status)}
              </span>
              {plan.status === "draft" && (
                <button
                  disabled={busy}
                  onClick={async () => {
                    setBusy(true);
                    try {
                      await mutate(() =>
                        api(
                          endpoint(
                            detail.front.id,
                            `/plans/${encodeURIComponent(plan.id)}/approve`,
                          ),
                          {
                            expectedRevision: detail.front.revision,
                            requestId: operationId(),
                          },
                        ),
                      );
                    } catch {
                      /* Page error. */
                    } finally {
                      setBusy(false);
                    }
                  }}
                >
                  Approve plan
                </button>
              )}
            </div>
          </div>
          {(plan.waves || detail.waves.filter((w) => w.planId === plan.id)).map(
            (wave, i) => (
              <WaveRow
                key={wave.id}
                index={i}
                wave={wave}
                detail={detail}
                agents={agents}
                agentId={agentId}
                setAgentId={setAgentId}
                mutate={mutate}
              />
            ),
          )}
        </section>
      ))}
    </div>
  );
}
function WaveRow({
  wave,
  detail,
  index,
  agents,
  agentId,
  setAgentId,
  mutate,
}: {
  wave: Wave;
  detail: FrontDetail;
  index: number;
  agents: Agent[];
  agentId: string;
  setAgentId: (id: string) => void;
  mutate: (work: () => Promise<unknown>) => Promise<void>;
}) {
  const prefix = `front:${detail.front.id}:wave:${wave.id}:review:`;
  const [busy, setBusy] = useState(false);
  const [reviewing, setReviewing] = useDraftValue(
    prefix + "open",
    false,
    neverDirty,
  );
  const [revision, setRevision] = useDraftValue<string>(
    prefix + "revision",
    wave.resultRevision || "",
    (value) => Boolean(value.trim()) && value !== wave.resultRevision,
  );
  const [evidence, setEvidence] = useDraftValue<string>(
    prefix + "evidence",
    "",
    hasText,
  );
  const [reason, setReason] = useDraftValue<string>(
    prefix + "reason",
    "",
    hasText,
  );
  const allows = (action: string) =>
    wave.allowedActions?.some((a) => a === action || a.endsWith(`.${action}`));
  const perform = async (suffix: string, body: unknown) => {
    setBusy(true);
    try {
      await mutate(() =>
        api(
          endpoint(
            detail.front.id,
            `/waves/${encodeURIComponent(wave.id)}/${suffix}`,
          ),
          body,
        ),
      );
      if (suffix === "review") {
        setReviewing(false);
        setEvidence("");
        setReason("");
      }
    } catch {
      /* Page error. */
    } finally {
      setBusy(false);
    }
  };
  return (
    <article className="wave-row">
      <div className="wave-number">{String(index + 1).padStart(2, "0")}</div>
      <div className="wave-content">
        <div className="wave-title">
          <h4>{wave.title}</h4>
          <span className="mono muted">
            {wave.beadId ? `Bead ${wave.beadId}` : "Not admitted"}
          </span>
        </div>
        <div className="state-axes">
          {(
            [
              "planning",
              "dependencies",
              "execution",
              "review",
              "integration",
            ] as const
          ).map((axis) => (
            <div key={axis}>
              <span>{axis}</span>
              <strong className={`state-${wave[axis]}`}>
                {label(wave[axis])}
                {axis === "review" &&
                wave.review === "accepted" &&
                wave.reviewValidity !== "current"
                  ? " · proof unknown"
                  : ""}
              </strong>
            </div>
          ))}
        </div>
        <ul className="criteria-list">
          {wave.criteria.map((criterion, i) => (
            <li key={i}>{criterion}</li>
          ))}
        </ul>
        {wave.dependsOn.length > 0 && (
          <p className="field-hint">
            Depends on{" "}
            {wave.dependsOn
              .map(
                (id) =>
                  detail.waves.find((dependency) => dependency.id === id)
                    ?.title || "Unavailable dependency",
              )
              .join(", ")}
          </p>
        )}
        {wave.blockReasons?.length > 0 && (
          <ul className="block-reasons">
            {wave.blockReasons.map((reason) => (
              <li key={reason}>{label(reason)}</li>
            ))}
          </ul>
        )}
        <div className="wave-actions">
          {allows("dispatch") && (
            <>
              <label>
                <span className="sr-only">Agent for {wave.title}</span>
                <select
                  value={agentId}
                  onChange={(e) => setAgentId(e.target.value)}
                >
                  <option value="">Choose execution agent</option>
                  {agents.map((a) => (
                    <option key={a.id} disabled={!a.available} value={a.id}>
                      {a.name || a.id}
                    </option>
                  ))}
                </select>
              </label>
              <button
                className="primary"
                disabled={busy || !agentId}
                onClick={() =>
                  void perform("dispatch", {
                    agentId,
                    expectedRevision: detail.front.revision,
                    requestId: operationId(),
                  })
                }
              >
                Dispatch wave
              </button>
            </>
          )}
          {allows("continue") && (
            <button
              disabled={busy}
              onClick={() =>
                void perform("continue", { requestId: operationId() })
              }
            >
              Continue execution
            </button>
          )}
          {(allows("review") ||
            allows("accept") ||
            allows("changes_requested")) && (
            <button disabled={busy} onClick={() => setReviewing(!reviewing)}>
              Review result
            </button>
          )}
          {allows("withdraw") && (
            <button
              disabled={busy}
              onClick={() =>
                void perform("withdraw", {
                  requestId: operationId(),
                  expectedRevision: detail.front.revision,
                })
              }
            >
              Withdraw wave
            </button>
          )}
          {allows("integrate") && (
            <button
              disabled={busy || !wave.resultRevision}
              onClick={() =>
                void perform("integrate", {
                  resultRevision: wave.resultRevision!,
                  requestId: operationId(),
                } satisfies IntegrationRequest)
              }
            >
              Request integration
            </button>
          )}
          {allows("close") && (
            <button
              disabled={busy}
              onClick={() =>
                void perform("close", { requestId: operationId() })
              }
            >
              Close delivery
            </button>
          )}
        </div>
        {allows("integrate") && (
          <p className="field-hint">
            This requests integration of the accepted result into its recorded
            target. The engine checks the exact revision again. It does not
            close the delivery.
          </p>
        )}
        {allows("close") && (
          <p className="field-hint">
            Closing requires acceptance of this exact result and verified
            integration. The engine checks both again before closing the Bead.
          </p>
        )}
        {allows("withdraw") && (
          <p className="field-hint">
            Withdrawal prevents this wave from entering execution and preserves
            its history. It does not cancel running work.
          </p>
        )}
        {reviewing && (
          <div className="review-editor form-stack">
            <h5>Review this exact result</h5>
            <label>
              Result revision
              <input
                required
                value={revision}
                onChange={(e) => setRevision(e.target.value)}
                placeholder="Exact commit or result revision"
              />
            </label>
            <label>
              Evidence for the criteria
              <textarea
                rows={3}
                value={evidence}
                onChange={(e) => setEvidence(e.target.value)}
                placeholder="One evidence reference per line"
              />
            </label>
            <label>
              Review note
              <textarea
                rows={2}
                value={reason}
                onChange={(e) => setReason(e.target.value)}
              />
            </label>
            <p className="field-hint">
              Acceptance does not authorize merge, closure or cleanup.
            </p>
            <div className="actions">
              <button
                className="primary"
                disabled={busy || !revision.trim() || !evidence.trim()}
                onClick={() =>
                  void perform("review", {
                    decision: "accepted",
                    resultRevision: revision.trim(),
                    evidence: evidence
                      .split("\n")
                      .map((s) => s.trim())
                      .filter(Boolean),
                    reason,
                    requestId: operationId(),
                  })
                }
              >
                Accept result
              </button>
              <button
                disabled={busy || !revision.trim() || !reason.trim()}
                onClick={() =>
                  void perform("review", {
                    decision: "changes_requested",
                    resultRevision: revision.trim(),
                    evidence: evidence.split("\n").filter(Boolean),
                    reason,
                    requestId: operationId(),
                  })
                }
              >
                Request changes
              </button>
            </div>
          </div>
        )}
      </div>
    </article>
  );
}
function Documents({
  detail,
  mutate,
}: {
  detail: FrontDetail;
  mutate: (work: () => Promise<unknown>) => Promise<void>;
}) {
  const prefix = `front:${detail.front.id}:documents:`;
  const [selected, setSelected] = useState<Artifact | null>(null);
  const [adding, setAdding] = useDraftValue(
    prefix + "adding",
    false,
    neverDirty,
  );
  const [name, setName] = useDraftValue<string>(prefix + "name", "", hasText);
  const [text, setText] = useDraftValue<string>(prefix + "text", "", hasText);
  const [path, setPath] = useDraftValue<string>(prefix + "path", "", hasText);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const open = async (artifact: Artifact) => {
    setError("");
    try {
      setSelected(
        await api<Artifact>(
          `/api/artifacts/${encodeURIComponent(artifact.digest)}`,
        ),
      );
    } catch (e) {
      setError((e as Error).message);
    }
  };
  return (
    <div>
      <div className="section-heading">
        <div>
          <h2>Documents & evidence</h2>
          <p className="muted">
            Full originals, preserved by content revision.
          </p>
        </div>
        <button onClick={() => setAdding(!adding)}>
          {adding ? "Close editor" : "+ Add document"}
        </button>
      </div>
      {adding && (
        <div className="document-editor">
          <form
            className="form-stack"
            onSubmit={async (e) => {
              e.preventDefault();
              setBusy(true);
              try {
                await mutate(() =>
                  api(endpoint(detail.front.id, "/artifacts"), {
                    name: name.trim(),
                    text,
                    requestId: operationId(),
                  }),
                );
                setAdding(false);
                setName("");
                setText("");
              } catch {
                /* Page error. */
              } finally {
                setBusy(false);
              }
            }}
          >
            <label>
              Document name
              <input
                required
                value={name}
                onChange={(e) => setName(e.target.value)}
                maxLength={200}
              />
            </label>
            <label>
              Full content
              <textarea
                required
                rows={8}
                value={text}
                onChange={(e) => setText(e.target.value)}
                maxLength={1000000}
              />
            </label>
            <button className="primary" disabled={busy}>
              Save document
            </button>
          </form>
          <form
            className="form-stack import-form"
            onSubmit={async (e) => {
              e.preventDefault();
              setBusy(true);
              try {
                await mutate(() =>
                  api(endpoint(detail.front.id, "/artifacts/import"), {
                    path,
                    requestId: operationId(),
                  }),
                );
                setAdding(false);
                setPath("");
              } catch {
                /* Page error. */
              } finally {
                setBusy(false);
              }
            }}
          >
            <h3>Import from an allowed folder</h3>
            <label>
              File path
              <input
                required
                value={path}
                onChange={(e) => setPath(e.target.value)}
                placeholder="Absolute path inside this project’s document roots"
              />
            </label>
            <button disabled={busy}>Import document</button>
          </form>
        </div>
      )}
      {error && (
        <p className="inline-error" role="alert">
          {error}
        </p>
      )}
      {!detail.artifacts.length && !adding ? (
        <div className="empty-inline">
          <Icon name="document" />
          <h3>Keep the complete evidence.</h3>
          <p>
            Add a document or import a file from a folder allowed in project
            settings. Summaries do not replace originals.
          </p>
        </div>
      ) : (
        <div className="documents-layout">
          <ul className="document-list">
            {detail.artifacts.map((artifact) => (
              <li key={artifact.digest}>
                <button
                  className={
                    selected?.digest === artifact.digest ? "selected" : ""
                  }
                  onClick={() => void open(artifact)}
                >
                  <Icon name="document" />
                  <span>
                    {artifact.name}
                    <small className="mono">
                      {artifact.digest.slice(0, 12)}
                    </small>
                  </span>
                </button>
              </li>
            ))}
          </ul>
          <div className="document-reader">
            {selected ? (
              <>
                <div className="reader-heading">
                  <h3>{selected.name}</h3>
                  <span className="mono">{selected.digest.slice(0, 12)}</span>
                </div>
                <pre>{selected.text}</pre>
              </>
            ) : (
              <p className="muted">
                Select a document to read the full original.
              </p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
