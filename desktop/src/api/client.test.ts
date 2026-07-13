import { describe, expect, it, vi } from "vitest";
import { request } from "./client";

const mockFetch = vi.fn();
vi.mock("@tauri-apps/plugin-http", () => ({
  fetch: (...args: unknown[]) => mockFetch(...args),
}));

vi.mock("../stores/auth", () => ({
  useAuthStore: () => ({
    isAuthenticated: true,
    session: {
      server_url: "http://localhost:8080",
      token: "test-token",
    },
    clear: vi.fn(),
  }),
}));

describe("request", () => {
  it("injects the Authorization header", async () => {
    mockFetch.mockResolvedValueOnce({
      status: 200,
      ok: true,
      json: async () => ({ id: "1" }),
    });

    const result = await request("/api/v1/images");

    expect(mockFetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/images",
      expect.objectContaining({
        headers: expect.any(Headers),
      }),
    );
    const call = mockFetch.mock.calls[0] as [string, { headers: Headers }];
    expect(call[1].headers.get("Authorization")).toBe("Bearer test-token");
    expect(result).toEqual({ id: "1" });
  });

  it("throws ApiError on non-ok response", async () => {
    mockFetch.mockResolvedValueOnce({
      status: 500,
      ok: false,
      text: async () => "internal error",
    });

    await expect(request("/api/v1/images")).rejects.toThrow("internal error");
  });
});
