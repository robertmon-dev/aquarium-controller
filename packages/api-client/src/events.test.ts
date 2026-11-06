import { describe, expect, it } from "vitest";
import { SseDecoder, streamTelemetry } from "./events";

const device = { host: "10.0.0.2", port: 80, token: "tok" };

function streamOf(...chunks: string[]): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
      controller.close();
    },
  });
}

describe("SseDecoder", () => {
  it("decodes a complete event", () => {
    const decoder = new SseDecoder();

    expect(decoder.push('event: telemetry\ndata: {"a":1}\n\n')).toEqual([
      { event: "telemetry", data: '{"a":1}' },
    ]);
  });

  it("buffers a frame split across chunks", () => {
    const decoder = new SseDecoder();

    expect(decoder.push('event: telemetry\ndata: {"a"')).toEqual([]);
    expect(decoder.push(":1}\n\n")).toEqual([
      { event: "telemetry", data: '{"a":1}' },
    ]);
  });

  it("decodes two events arriving in one chunk", () => {
    const decoder = new SseDecoder();

    expect(
      decoder.push(
        "event: telemetry\ndata: 1\n\nevent: telemetry\ndata: 2\n\n",
      ),
    ).toEqual([
      { event: "telemetry", data: "1" },
      { event: "telemetry", data: "2" },
    ]);
  });

  it("joins multi-line data with newlines", () => {
    const decoder = new SseDecoder();

    expect(
      decoder.push("event: telemetry\ndata: line1\ndata: line2\n\n"),
    ).toEqual([{ event: "telemetry", data: "line1\nline2" }]);
  });

  it("ignores keepalive comment lines", () => {
    const decoder = new SseDecoder();

    expect(decoder.push(":keepalive\n\n")).toEqual([]);
  });

  it("tolerates crlf line endings", () => {
    const decoder = new SseDecoder();

    expect(decoder.push("event: telemetry\r\ndata: 1\r\n\r\n")).toEqual([
      { event: "telemetry", data: "1" },
    ]);
  });

  it("defaults the event name to message when absent", () => {
    const decoder = new SseDecoder();

    expect(decoder.push("data: 1\n\n")).toEqual([
      { event: "message", data: "1" },
    ]);
  });
});

describe("streamTelemetry", () => {
  it("yields parsed snapshots from telemetry events", async () => {
    const body = streamOf(
      'event: telemetry\ndata: {"timestamp_epoch_s":1}\n\n',
      ":keepalive\n\n",
      'event: telemetry\ndata: {"timestamp_epoch_s":2}\n\n',
    );
    const fetchImpl = async () => new Response(body, { status: 200 });

    const seen: number[] = [];
    for await (const snapshot of streamTelemetry({ device, fetchImpl })) {
      seen.push(snapshot.timestamp_epoch_s);
    }

    expect(seen).toEqual([1, 2]);
  });

  it("requests the events url with the token in the query", async () => {
    const urls: string[] = [];
    const fetchImpl = async (url: string) => {
      urls.push(url);
      return new Response(streamOf(), { status: 200 });
    };

    for await (const snapshot of streamTelemetry({ device, fetchImpl })) {
      void snapshot;
    }

    expect(urls[0]).toBe("http://10.0.0.2:80/api/events?token=tok");
  });

  it("throws an unauthorized ApiError on 401", async () => {
    const fetchImpl = async () =>
      new Response("unauthorized\n", { status: 401 });

    const iterate = async () => {
      for await (const snapshot of streamTelemetry({ device, fetchImpl })) {
        void snapshot;
      }
    };

    await expect(iterate()).rejects.toMatchObject({ kind: "unauthorized" });
  });

  it("skips a telemetry event whose data is not json", async () => {
    const body = streamOf(
      "event: telemetry\ndata: not-json\n\n",
      'event: telemetry\ndata: {"timestamp_epoch_s":5}\n\n',
    );
    const fetchImpl = async () => new Response(body, { status: 200 });

    const seen: number[] = [];
    for await (const snapshot of streamTelemetry({ device, fetchImpl })) {
      seen.push(snapshot.timestamp_epoch_s);
    }

    expect(seen).toEqual([5]);
  });
});
