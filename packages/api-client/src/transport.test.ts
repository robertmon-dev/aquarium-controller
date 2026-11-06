import { describe, expect, it, vi } from "vitest";
import { ApiError, request } from "./transport";

const device = { host: "10.0.0.2", port: 80, token: "tok" };

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

async function rejection(promise: Promise<unknown>): Promise<ApiError> {
  try {
    await promise;
  } catch (error) {
    return error as ApiError;
  }

  throw new Error("expected the request to reject, but it resolved");
}

describe("request", () => {
  it("sends the token as a bearer header", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(jsonResponse({ ok: true }));

    await request({ device, fetchImpl }, "/api/health");

    const [, init] = fetchImpl.mock.calls[0]!;
    expect(init.headers.Authorization).toBe("Bearer tok");
  });

  it("returns the parsed body on success", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(jsonResponse({ uptime_seconds: 7 }));

    await expect(
      request({ device, fetchImpl }, "/api/health"),
    ).resolves.toEqual({
      uptime_seconds: 7,
    });
  });

  it("maps 401 to an unauthorized ApiError", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(new Response("unauthorized\n", { status: 401 }));

    const error = await rejection(
      request({ device, fetchImpl }, "/api/health"),
    );

    expect(error).toBeInstanceOf(ApiError);
    expect(error.kind).toBe("unauthorized");
  });

  it("maps a thrown fetch to a transport ApiError", async () => {
    const fetchImpl = vi
      .fn()
      .mockRejectedValue(new TypeError("Network request failed"));

    const error = await rejection(
      request({ device, fetchImpl }, "/api/health"),
    );

    expect(error).toBeInstanceOf(ApiError);
    expect(error.kind).toBe("transport");
  });

  it("maps a non-json body to a malformed ApiError", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(new Response("<html>nope", { status: 200 }));

    const error = await rejection(
      request({ device, fetchImpl }, "/api/health"),
    );

    expect(error).toBeInstanceOf(ApiError);
    expect(error.kind).toBe("malformed");
  });

  it("maps other error statuses to a status ApiError carrying the body", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(
        new Response("channel level out of range", { status: 400 }),
      );

    const error = await rejection(
      request({ device, fetchImpl }, "/api/lighting/override"),
    );

    expect(error.kind).toBe("status");
    expect(error.status).toBe(400);
    expect(error.detail).toBe("channel level out of range");
  });

  it("treats 204 as an empty body", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 204 }));

    await expect(
      request({ device, fetchImpl }, "/api/lighting/override"),
    ).resolves.toBeUndefined();
  });
});
