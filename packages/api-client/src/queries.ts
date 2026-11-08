import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { createClient } from "./client";
import type { DeviceConfig } from "./config";
import type {
  LightingOverride,
  LightingScheduleDto,
  SetOverrideRequest,
  SystemHealthDto,
  TankTelemetryDto,
} from "./generated";
import { queryKeys } from "./query-keys";
import type { Transport } from "./transport";

export function useTelemetry(
  transport: Transport,
): UseQueryResult<TankTelemetryDto> {
  return useQuery({
    queryKey: queryKeys.telemetry(transport.device),
    queryFn: () => createClient(transport).telemetry(),
    staleTime: 5_000,
  });
}

export function useHealth(
  transport: Transport,
): UseQueryResult<SystemHealthDto> {
  return useQuery({
    queryKey: queryKeys.health(transport.device),
    queryFn: () => createClient(transport).health(),
  });
}

export function useLightingOverride(
  transport: Transport,
): UseQueryResult<LightingOverride | null> {
  return useQuery({
    queryKey: queryKeys.lightingOverride(transport.device),
    queryFn: () => createClient(transport).lightingOverride(),
  });
}

// The curve for a month does not change, so this is fetched once a session
// rather than polled.
export function useLightingSchedule(
  transport: Transport,
  month: number,
): UseQueryResult<LightingScheduleDto> {
  return useQuery({
    queryKey: queryKeys.lightingSchedule(transport.device, month),
    queryFn: () => createClient(transport).lightingSchedule(month),
    staleTime: 6 * 60 * 60 * 1000,
  });
}

export function useSetLightingOverride(
  transport: Transport,
): UseMutationResult<LightingOverride, Error, SetOverrideRequest> {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (body: SetOverrideRequest) =>
      createClient(transport).setLightingOverride(body),
    onSuccess: (over) => {
      queryClient.setQueryData(
        queryKeys.lightingOverride(transport.device),
        over,
      );
    },
  });
}

export function useClearLightingOverride(
  transport: Transport,
): UseMutationResult<void, Error, void> {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => createClient(transport).clearLightingOverride(),
    onSuccess: () => {
      queryClient.setQueryData(
        queryKeys.lightingOverride(transport.device),
        null,
      );
    },
  });
}

export function publishTelemetry(
  queryClient: QueryClient,
  device: DeviceConfig,
  snapshot: TankTelemetryDto,
): void {
  queryClient.setQueryData(queryKeys.telemetry(device), snapshot);
}
