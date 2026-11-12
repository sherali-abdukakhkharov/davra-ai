// Client for the hub's signed-in web API (/api/v1/web/*). Mirrors the hub contract.

import type { Trust } from "@/components/landing/room";
import { ApiError, readError, request, type User } from "./api";

export type { Trust };
export type Role = "owner" | "maintainer" | "member" | "viewer" | "agent" | (string & {});

export type ProjectItem = {
  org: string;
  slug: string;
  name: string;
  role: Role;
  handle: string;
  members: number;
  agents: number;
  messages: number;
  open_handoffs: number;
  last_activity: string | null;
  telegram: boolean;
};

export type Overview = { user: User; telegram: { linked: boolean }; projects: ProjectItem[] };

export type TelegramLink = { code: string; bot: string; deep_link: string; expires_in: number };

export type Token = { id: string | number; name: string; created_at: string; last_used_at: string | null };

export type Member = {
  handle: string;
  kind: "human" | "agent";
  /** A person with a Davra web account; people added from the CLI have none yet. */
  linked: boolean;
  display_name: string | null;
  owner: string | null;
  roles: string[];
  role: Role;
  last_seen: string | null;
  is_me: boolean;
  mine: boolean;
  tokens: Token[];
};

export type Message = {
  id: number;
  created_at: string;
  author: string;
  author_kind: "human" | "agent" | null;
  trust: Trust;
  kind: string;
  topic: string | null;
  body: string;
  reply_to: number | null;
  mentions: string[];
};

export type Handoff = {
  id: number;
  ticket: string | null;
  from: string;
  to: string[];
  status: string;
  body: string;
  done_note: string | null;
  source_path: string | null;
  created_at: string;
  done_at: string | null;
  message_id: number | null;
};

export type ProjectRef = { org: string; slug: string; name: string; role: Role; handle: string; telegram: boolean };

export type ProjectDetail = {
  project: ProjectRef;
  stats: { messages: number; messages_7d: number; open_handoffs: number; done_handoffs_7d: number; agents: number };
  members: Member[];
  messages: Message[];
  handoffs: Handoff[];
};

export type SearchHit = { id: number; created_at: string; author: string; trust: Trust; snippet: string; score: number };
export type SearchResult = { project: ProjectRef; query: string; hits: SearchHit[] };

export type Invite = {
  id: string;
  role: Role;
  /** The existing person this link hands over, if any. */
  member: string | null;
  created_by: string | null;
  created_at: string;
  expires_at: string;
};

export type InviteInfo = {
  org: string;
  slug: string;
  name: string | null;
  role: Role;
  invited_by: string | null;
  member: string | null;
  status: "open" | "used" | "expired" | "revoked";
};

export type Accepted = { org: string; slug: string; handle: string; already: boolean };

/** Roles someone with `role` may hand out or change; mirrors the hub's rule. */
export const grantable = (role: Role): Role[] =>
  role === "owner" ? ["maintainer", "member", "viewer"] : role === "maintainer" ? ["member", "viewer"] : [];

export const isAdmin = (role: Role) => role === "owner" || role === "maintainer";

export type NewToken = { id: string | number; token: string; hub: string; commands: string[] };

const enc = encodeURIComponent;
const base = (org: string, slug: string) => `/api/v1/web/projects/${enc(org)}/${enc(slug)}`;

async function get<T>(path: string, signal?: AbortSignal): Promise<T> {
  const res = await request("GET", path, undefined, signal);
  if (!res.ok) throw await readError(res);
  return (await res.json()) as T;
}

async function post<T>(path: string, body?: unknown): Promise<T> {
  const res = await request("POST", path, body);
  if (!res.ok) throw await readError(res);
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

export const web = {
  overview: (signal?: AbortSignal) => get<Overview>("/api/v1/web/overview", signal),
  telegramLink: () => post<TelegramLink>("/api/v1/web/telegram/link"),
  project: (org: string, slug: string, signal?: AbortSignal) => get<ProjectDetail>(base(org, slug), signal),
  search: (org: string, slug: string, q: string, k = 20, signal?: AbortSignal) =>
    get<SearchResult>(`${base(org, slug)}/search?q=${enc(q)}&k=${k}`, signal),
  older: (org: string, slug: string, before: number) =>
    get<{ messages: Message[] }>(`${base(org, slug)}/messages?before=${before}`),
  post: (org: string, slug: string, text: string) =>
    post<{ id: number; relayed: boolean; redacted: boolean }>(`${base(org, slug)}/messages`, { text }),
  addAgent: (org: string, slug: string, handle: string, roles: string[]) =>
    post<{ agent: Member }>(`${base(org, slug)}/agents`, { handle, roles }),
  setRoles: (org: string, slug: string, handle: string, roles: string[]) =>
    post<{ agent: Member }>(`${base(org, slug)}/agents/${enc(handle)}/roles`, { roles }),
  createToken: (org: string, slug: string, handle: string, name: string) =>
    post<NewToken>(`${base(org, slug)}/agents/${enc(handle)}/tokens`, { name }),
  revokeToken: (id: string | number) => post<void>(`/api/v1/web/tokens/${enc(String(id))}/revoke`),
  invites: (org: string, slug: string, signal?: AbortSignal) => get<{ invites: Invite[] }>(`${base(org, slug)}/invites`, signal),
  createInvite: (org: string, slug: string, role: Role, member?: string) =>
    post<{ invite: Invite; url: string }>(`${base(org, slug)}/invites`, member ? { role, member } : { role }),
  revokeInvite: (org: string, slug: string, id: string) => post<void>(`${base(org, slug)}/invites/${enc(id)}/revoke`),
  inviteInfo: (code: string, signal?: AbortSignal) => get<InviteInfo>(`/api/v1/web/invites/${enc(code)}`, signal),
  acceptInvite: (code: string, handle?: string) => post<Accepted>(`/api/v1/web/invites/${enc(code)}/accept`, handle ? { handle } : {}),
  setMemberRole: (org: string, slug: string, handle: string, role: Role) =>
    post<{ members: Member[] }>(`${base(org, slug)}/members/${enc(handle)}/role`, { role }),
  removeMember: (org: string, slug: string, handle: string) => post<void>(`${base(org, slug)}/members/${enc(handle)}/remove`),
  createProject: (input: { org: string; org_name?: string; slug: string; name: string; handle: string }) =>
    post<{ project: ProjectItem }>("/api/v1/web/projects", input),
};

export type WebErrorCopy = {
  network: string;
  unexpected: string;
  forbidden: string;
  notFound: string;
  signedOut: string;
};

/** Text for a failed web call: the server's message when it sent one, else our copy. */
export function webErrorText(err: unknown, copy: WebErrorCopy): string {
  if (!(err instanceof ApiError)) return copy.unexpected.replace("{status}", "?");
  if (err.serverMessage) return err.serverMessage;
  if (err.code === "network") return copy.network;
  if (err.status === 401) return copy.signedOut;
  if (err.status === 403) return copy.forbidden;
  if (err.status === 404) return copy.notFound;
  return copy.unexpected.replace("{status}", String(err.status));
}

export const canPost = (role: Role) => role !== "viewer";
