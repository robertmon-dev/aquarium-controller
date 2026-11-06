import type { FetchImpl } from "@aquarium/api-client";
import { fetch as expoFetch } from "expo/fetch";

export const deviceFetch = expoFetch as unknown as FetchImpl;
