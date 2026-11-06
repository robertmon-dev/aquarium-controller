import { eventsUrl } from "./config";
import type { TankTelemetryDto } from "./generated";
import { ApiError, type Transport } from "./transport";

export type SseEvent = {
  event: string;
  data: string;
};

export class SseDecoder {
  private buffer = "";

  push(chunk: string): SseEvent[] {
    this.buffer += chunk.replace(/\r\n/g, "\n").replace(/\r/g, "\n");

    const events: SseEvent[] = [];
    let boundary = this.buffer.indexOf("\n\n");

    while (boundary !== -1) {
      const frame = this.buffer.slice(0, boundary);
      this.buffer = this.buffer.slice(boundary + 2);

      const event = decodeFrame(frame);
      if (event) events.push(event);

      boundary = this.buffer.indexOf("\n\n");
    }

    return events;
  }
}

function decodeFrame(frame: string): SseEvent | null {
  let name: string | null = null;
  const data: string[] = [];

  for (const line of frame.split("\n")) {
    if (line === "" || line.startsWith(":")) continue;

    const colon = line.indexOf(":");
    const field = colon === -1 ? line : line.slice(0, colon);
    const rawValue = colon === -1 ? "" : line.slice(colon + 1);
    const value = rawValue.startsWith(" ") ? rawValue.slice(1) : rawValue;

    if (field === "event") name = value;
    else if (field === "data") data.push(value);
  }

  if (data.length === 0) return null;

  return { event: name ?? "message", data: data.join("\n") };
}

export async function* streamTelemetry(
  transport: Transport,
  signal?: AbortSignal,
): AsyncGenerator<TankTelemetryDto> {
  const url = eventsUrl(transport.device);

  let response: Response;
  try {
    response = await transport.fetchImpl(url, {
      headers: { accept: "text/event-stream" },
      ...(signal ? { signal } : {}),
    });
  } catch (cause) {
    throw new ApiError(
      "transport",
      `cannot reach ${transport.device.host}`,
      undefined,
      String(cause),
    );
  }

  if (response.status === 401) {
    throw new ApiError("unauthorized", "device rejected the api token", 401);
  }

  if (!response.ok) {
    throw new ApiError(
      "status",
      `device returned ${response.status}`,
      response.status,
    );
  }

  if (!response.body) {
    throw new ApiError(
      "malformed",
      "event stream response had no body",
      response.status,
    );
  }

  const reader = response.body.getReader();
  const utf8 = new TextDecoder();
  const decoder = new SseDecoder();

  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) return;

      for (const event of decoder.push(utf8.decode(value, { stream: true }))) {
        if (event.event !== "telemetry") continue;

        const snapshot = tryParse(event.data);
        if (snapshot) yield snapshot;
      }
    }
  } finally {
    reader.releaseLock();
  }
}

function tryParse(data: string): TankTelemetryDto | null {
  try {
    return JSON.parse(data) as TankTelemetryDto;
  } catch {
    return null;
  }
}
