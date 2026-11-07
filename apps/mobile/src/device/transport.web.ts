import type { FetchImpl } from "@aquarium/api-client";

export const deviceFetch: FetchImpl = (input, init) =>
  globalThis.fetch(input, init);
