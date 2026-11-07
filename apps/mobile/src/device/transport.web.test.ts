import { afterEach, describe, expect, it, vi } from "vitest";

const originalFetch = globalThis.fetch;

afterEach(() => {
  globalThis.fetch = originalFetch;
  vi.resetModules();
});

describe("the web transport", () => {
  it("calls fetch with globalThis as its receiver", async () => {
    const calls: string[] = [];

    function browserFetch(this: unknown, input: string) {
      if (this !== globalThis) {
        throw new TypeError(
          "Failed to execute 'fetch' on 'Window': Illegal invocation",
        );
      }
      calls.push(input);
      return Promise.resolve(new Response("{}"));
    }

    globalThis.fetch = browserFetch as unknown as typeof fetch;

    const { deviceFetch } = await import("./transport.web");
    await deviceFetch("http://localhost:8099/api/telemetry");

    expect(calls).toEqual(["http://localhost:8099/api/telemetry"]);
  });

  it("passes the request init through", async () => {
    let seen: RequestInit | undefined;

    function browserFetch(this: unknown, _input: string, init?: RequestInit) {
      if (this !== globalThis) throw new TypeError("Illegal invocation");
      seen = init;
      return Promise.resolve(new Response("{}"));
    }

    globalThis.fetch = browserFetch as unknown as typeof fetch;

    const { deviceFetch } = await import("./transport.web");
    await deviceFetch("http://localhost:8099/api/health", {
      headers: { Authorization: "Bearer t" },
    });

    expect(seen?.headers).toEqual({ Authorization: "Bearer t" });
  });
});
