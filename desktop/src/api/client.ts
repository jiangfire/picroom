import { fetch } from "@tauri-apps/plugin-http";
import router from "../router";
import { useAuthStore } from "../stores/auth";

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

export interface RequestOptions extends RequestInit {
  query?: Record<string, string | number | boolean | undefined>;
}

export async function request<T = unknown>(
  path: string,
  options: RequestOptions = {},
): Promise<T> {
  const auth = useAuthStore();
  if (!auth.isAuthenticated || !auth.session) {
    await router.replace({ name: "login" });
    throw new ApiError(401, "unauthenticated", "no active session");
  }

  const url = buildUrl(auth.session.server_url, path, options.query);
  const headers = buildHeaders(options, auth.session.token);

  const response = await fetch(url, {
    ...options,
    headers,
  });

  if (response.status === 401) {
    await auth.logout();
    await router.replace({ name: "login" });
    throw new ApiError(401, "unauthorized", "session expired");
  }

  if (!response.ok) {
    const text = await response.text();
    throw new ApiError(response.status, "error", text);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

function buildUrl(
  base: string,
  path: string,
  query?: RequestOptions["query"],
): string {
  const trimmed = base.replace(/\/$/, "");
  const normalized = path.startsWith("/") ? path : `/${path}`;
  const url = new URL(`${trimmed}${normalized}`);
  if (query) {
    for (const [key, value] of Object.entries(query)) {
      if (value !== undefined && value !== null) {
        url.searchParams.set(key, String(value));
      }
    }
  }
  return url.toString();
}

function buildHeaders(options: RequestOptions, token: string): Headers {
  const headers = new Headers(options.headers);
  if (!headers.has("Authorization")) {
    headers.set("Authorization", `Bearer ${token}`);
  }
  if (
    options.body &&
    typeof options.body === "string" &&
    !headers.has("Content-Type")
  ) {
    headers.set("Content-Type", "application/json");
  }
  return headers;
}
