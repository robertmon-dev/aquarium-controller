import { endpointUrl, type ApiPath, type DeviceConfig } from "./config";

export type FetchImpl = (
  input: string,
  init?: RequestInit,
) => Promise<Response>;

export type Transport = {
  device: DeviceConfig;
  fetchImpl: FetchImpl;
};

export type ApiErrorKind =
  | "unauthorized"
  | "transport"
  | "malformed"
  | "status";

export class ApiError extends Error {
  readonly kind: ApiErrorKind;
  readonly status?: number | undefined;
  readonly detail?: string | undefined;

  constructor(
    kind: ApiErrorKind,
    message: string,
    status?: number | undefined,
    detail?: string | undefined,
  ) {
    super(message);
    this.name = "ApiError";
    this.kind = kind;
    this.status = status;
    this.detail = detail;
  }
}

export async function request<T>(
  transport: Transport,
  path: ApiPath,
  init: RequestInit = {},
): Promise<T> {
  const url = endpointUrl(transport.device, path);

  let response: Response;
  try {
    response = await transport.fetchImpl(url, {
      ...init,
      headers: {
        Authorization: `Bearer ${transport.device.token}`,
        ...(init.body === undefined
          ? {}
          : { "content-type": "application/json" }),
        ...init.headers,
      },
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

  if (response.status === 204) {
    return undefined as T;
  }

  if (!response.ok) {
    const detail = (await response.text().catch(() => "")).trim();
    throw new ApiError(
      "status",
      `device returned ${response.status}`,
      response.status,
      detail,
    );
  }

  try {
    return (await response.json()) as T;
  } catch (cause) {
    throw new ApiError(
      "malformed",
      "device returned a body that is not json",
      response.status,
      String(cause),
    );
  }
}
