import { describe, expect, it, vi } from "vitest";
import { HttpClient } from "./http.js";

describe("HttpClient browser-safe headers", () => {
  it("keeps bodyless GET requests simple", async () => {
    const fetch = vi.fn(async () => new Response(JSON.stringify({ ok: true })));
    const client = new HttpClient({ baseUrl: "https://example.test", fetch });

    await client.get("/public");

    expect(fetch).toHaveBeenCalledWith("https://example.test/public", {
      method: "GET",
      token: undefined,
      headers: {},
    });
  });

  it("sets JSON content type when sending a body", async () => {
    let observedInit: RequestInit | undefined;
    const fetch = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      observedInit = init;
      return new Response(JSON.stringify({ ok: true }));
    });
    const client = new HttpClient({ baseUrl: "https://example.test", fetch });

    await client.post("/items", { value: 1 });

    expect(observedInit?.headers).toEqual({
      "Content-Type": "application/json",
    });
  });
});
