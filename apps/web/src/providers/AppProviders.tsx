import type { DeviceConfig, Transport } from "@aquarium/api-client";
import { LocaleProvider } from "@aquarium/i18n/use-locale";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { loadDevice, saveDevice } from "@/device/store";
import { deviceFetch } from "@/device/transport";
import { webLocaleAdapter } from "./locale-adapter";

const queryClient = new QueryClient();

interface DeviceContextValue {
  device: DeviceConfig | null;
  transport: Transport | null;
  setDevice: (device: DeviceConfig) => Promise<void>;
  loading: boolean;
}

const DeviceContext = createContext<DeviceContextValue | null>(null);

export function AppProviders({ children }: { children: ReactNode }) {
  const [device, setDeviceState] = useState<DeviceConfig | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadDevice()
      .then(setDeviceState)
      .finally(() => setLoading(false));
  }, []);

  const value = useMemo<DeviceContextValue>(
    () => ({
      device,
      transport: device ? { device, fetchImpl: deviceFetch } : null,
      setDevice: async (next) => {
        await saveDevice(next);
        setDeviceState(next);
      },
      loading,
    }),
    [device, loading],
  );

  return (
    <QueryClientProvider client={queryClient}>
      <LocaleProvider adapter={webLocaleAdapter}>
        <DeviceContext.Provider value={value}>
          {children}
        </DeviceContext.Provider>
      </LocaleProvider>
    </QueryClientProvider>
  );
}

export function useDevice(): DeviceContextValue {
  const value = useContext(DeviceContext);
  if (!value) throw new Error("useDevice must be used inside AppProviders");
  return value;
}
