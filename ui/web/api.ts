export interface Front {
  id: string;
  title: string;
  objective: string;
  projectId: string;
  revision: number;
  createdAt: string;
  updatedAt: string;
}
export interface Entry {
  id: string;
  frontId: string;
  kind: string;
  text: string;
  at: string;
  requestId: string;
}
export interface Wave {
  id: string;
  frontId: string;
  planId: string;
  title: string;
  criteria: string[];
  dependsOn: string[];
  beadId: string | null;
  operationId: string | null;
  planning: string;
  dependencies: string;
  execution: string;
  review: string;
  reviewValidity?: string;
  integration: string;
  resultRevision?: string;
  allowedActions: string[];
  blockReasons: string[];
}
export interface IntegrationRequest {
  resultRevision: string;
  requestId: string;
}
export interface Plan {
  id: string;
  frontId: string;
  title: string;
  revision: number;
  status: string;
  waves: Wave[];
  createdAt: string;
}
export interface Artifact {
  digest: string;
  name: string;
  text?: string;
  createdAt?: string;
}
export interface Session {
  id: string;
  agentId: string;
  status?: string;
  providerId?: string;
}
export interface Operation {
  id?: string;
  requestId?: string;
  kind: string;
  state: string;
  error?: string;
}
export interface FrontDetail {
  operations?: Operation[];
  front: Front;
  entries: Entry[];
  plans: Plan[];
  waves: Wave[];
  artifacts: Artifact[];
  sessions: Session[];
}
export interface Project {
  id: string;
  name: string;
  repo: string;
  documentRoots: string[];
  executorId: string;
}
export interface Connection {
  id: string;
  pluginId: string;
  name: string;
  projectId: string;
  enabled: boolean;
  config: Record<string, unknown>;
  revision?: number;
  health?: string | { status?: string; detail?: string };
  lastGoodAt?: string;
}
export interface Settings {
  version: number;
  revision: number;
  projects: Project[];
  connections: Connection[];
}
export interface Agent {
  id: string;
  name: string;
  available: boolean;
  capabilities: string[];
  verification?: string;
}
export interface Plugin {
  kind?: string;
  id: string;
  name: string;
  capabilities?: string[];
  fields?: { key: string; label: string; type?: string; required?: boolean }[];
  configFields?: {
    key: string;
    label: string;
    type?: string;
    required?: boolean;
  }[];
}
export const operationId = () => crypto.randomUUID();
export class ApiError extends Error {
  constructor(
    public code: string,
    message: string,
  ) {
    super(message);
  }
}
export async function api<T>(
  path: string,
  body?: unknown,
  method?: string,
): Promise<T> {
  const response = await fetch(path, {
    method: method || (body === undefined ? "GET" : "POST"),
    cache: "no-store",
    headers:
      body === undefined
        ? undefined
        : { "Content-Type": "application/json", "X-Macross-Client": "desktop" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const payload = await response.json();
  if (!response.ok)
    throw new ApiError(
      payload.error?.code || "request_failed",
      payload.error?.message || "The request could not be completed.",
    );
  return payload.data as T;
}
export const endpoint = (front: string, suffix = "") =>
  `/api/fronts/${encodeURIComponent(front)}${suffix}`;
export function label(value: string | undefined) {
  return (value || "unknown").replaceAll("_", " ");
}
export function timestamp(value?: string) {
  if (!value || !Number.isFinite(Date.parse(value))) return "Time unavailable";
  return new Date(value).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
