// Client for the hub's auth API. The contract lives in the hub; this file mirrors it exactly.

export type User = { id: string; name: string; email: string };

export type ApiErrorCode =
  | "bad_request"
  | "email_taken"
  | "invalid_credentials"
  | "rate_limited"
  | "network"
  | "unexpected";

export class ApiError extends Error {
  readonly code: ApiErrorCode;
  readonly status: number;
  /** Text from the server, shown to the user as-is when present. */
  readonly serverMessage: string | null;
  /** Request field the error is about ("name", "email", "password"), if the server named one. */
  readonly field: string | null;
  /** The server's raw `error` value, even when it is not one of the known codes. */
  readonly error: string | null;
  constructor(code: ApiErrorCode, status: number, serverMessage: string | null, field: string | null = null, error: string | null = null) {
    super(serverMessage ?? code);
    this.code = code;
    this.status = status;
    this.serverMessage = serverMessage;
    this.field = field;
    this.error = error;
  }
}

const KNOWN: readonly ApiErrorCode[] = ["bad_request", "email_taken", "invalid_credentials", "rate_limited"];

export async function readError(res: Response): Promise<ApiError> {
  let code: ApiErrorCode = "unexpected";
  let message: string | null = null;
  let field: string | null = null;
  let raw: string | null = null;
  try {
    const body: unknown = await res.json();
    if (body && typeof body === "object") {
      const b = body as { error?: unknown; message?: unknown; field?: unknown };
      if (typeof b.field === "string") field = b.field;
      if (typeof b.error === "string") raw = b.error;
      if (typeof b.error === "string" && (KNOWN as readonly string[]).includes(b.error)) {
        code = b.error as ApiErrorCode;
      }
      if (typeof b.message === "string" && b.message.trim()) message = b.message.trim();
    }
  } catch {
    // Not JSON (a proxy page, an old hub without these routes): fall back to a generic message.
  }
  if (code === "unexpected" && res.status === 429) code = "rate_limited";
  if (code === "unexpected" && res.status === 401) code = "invalid_credentials";
  return new ApiError(code, res.status, message, field, raw);
}

export async function request(method: "GET" | "POST", path: string, body?: unknown, signal?: AbortSignal) {
  const headers: Record<string, string> = { Accept: "application/json" };
  if (method === "POST") {
    headers["Content-Type"] = "application/json";
    headers["X-Davra"] = "1";
  }
  let res: Response;
  try {
    res = await fetch(path, {
      method,
      headers,
      credentials: "same-origin",
      body: method === "POST" ? JSON.stringify(body ?? {}) : undefined,
      signal,
    });
  } catch (e) {
    if (e instanceof DOMException && e.name === "AbortError") throw e;
    throw new ApiError("network", 0, null);
  }
  return res;
}

async function userFrom(res: Response): Promise<User> {
  const data = (await res.json()) as { user: User };
  return data.user;
}

export async function register(input: { name: string; email: string; password: string }): Promise<User> {
  const res = await request("POST", "/api/v1/auth/register", input);
  if (res.status === 201 || res.ok) return userFrom(res);
  throw await readError(res);
}

export async function login(input: { email: string; password: string }): Promise<User> {
  const res = await request("POST", "/api/v1/auth/login", input);
  if (res.ok) return userFrom(res);
  throw await readError(res);
}

export async function logout(): Promise<void> {
  const res = await request("POST", "/api/v1/auth/logout");
  if (!res.ok && res.status !== 401) throw await readError(res);
}

/** Current user, or null when there is no valid session. */
export async function session(signal?: AbortSignal): Promise<User | null> {
  const res = await request("GET", "/api/v1/auth/session", undefined, signal);
  if (res.status === 401) return null;
  if (!res.ok) throw await readError(res);
  return userFrom(res);
}

/** What to show for a failed call: the server's own message when it sent one, else our copy. */
export function errorText(
  err: unknown,
  copy: Record<"network" | "unexpected" | "invalid_credentials" | "rate_limited" | "email_taken" | "bad_request", string>,
): string {
  if (!(err instanceof ApiError)) return copy.unexpected.replace("{status}", "?");
  if (err.serverMessage) return err.serverMessage;
  if (err.code === "unexpected") return copy.unexpected.replace("{status}", String(err.status));
  return copy[err.code];
}
