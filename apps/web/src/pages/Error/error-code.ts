import { ApiError } from "@aquarium/api-client";
import type { ErrorCode } from "./types";

export function errorCodeFor(error: unknown): ErrorCode {
  if (!(error instanceof ApiError)) return "device-error";

  switch (error.kind) {
    case "unauthorized":
      return "unauthorized";
    case "transport":
      return "offline";
    default:
      return "device-error";
  }
}
