import {
  ApiError,
  createClient,
  type DeviceConfig,
} from "@aquarium/api-client";
import { zodResolver } from "@hookform/resolvers/zod";
import { useState } from "react";
import { useForm } from "react-hook-form";
import { View } from "react-native";
import { Alert, Button, Card } from "@/components/atoms";
import { FormField } from "@/components/molecules";
import { spacing } from "@/components/utility/theme";
import {
  deviceFormSchema,
  toDeviceConfig,
  type DeviceFormValues,
  type ParsedDeviceForm,
} from "@/device/config";
import { deviceFetch } from "@/device/transport";

export interface DeviceFormProps {
  device: DeviceConfig | null;
  onConnected: (device: DeviceConfig) => Promise<void>;
}

export function DeviceForm({ device, onConnected }: DeviceFormProps) {
  const [probeError, setProbeError] = useState<string | null>(null);
  const [probing, setProbing] = useState(false);

  const { control, handleSubmit } = useForm<
    DeviceFormValues,
    unknown,
    ParsedDeviceForm
  >({
    resolver: zodResolver(deviceFormSchema),
    defaultValues: {
      host: device?.host ?? "",
      port: device ? String(device.port) : "80",
      token: device?.token ?? "",
    },
  });

  const onSubmit = handleSubmit(async (values) => {
    const config = toDeviceConfig(values);
    setProbing(true);
    setProbeError(null);

    try {
      await createClient({ device: config, fetchImpl: deviceFetch }).health();
    } catch (error) {
      setProbeError(
        error instanceof ApiError && error.kind === "unauthorized"
          ? "the device rejected that token"
          : `could not reach ${config.host}:${config.port}`,
      );
      setProbing(false);
      return;
    }

    await onConnected(config);
    setProbing(false);
  });

  return (
    <Card variant="outlined" padding="md" width="full">
      <View style={{ gap: spacing.sm }}>
        <FormField
          control={control}
          name="host"
          label="host"
          placeholder="192.168.1.50"
        />
        <FormField
          control={control}
          name="port"
          label="port"
          keyboardType="number-pad"
          helperText="defaults to 80"
        />
        <FormField
          control={control}
          name="token"
          label="token"
          secureTextEntry
        />

        {probeError ? (
          <Alert
            variant="danger"
            title="cannot connect"
            onClose={() => setProbeError(null)}
          >
            {probeError}
          </Alert>
        ) : null}

        <Button
          variant="primary"
          fullWidth
          isLoading={probing}
          onPress={() => void onSubmit()}
        >
          save and connect
        </Button>
      </View>
    </Card>
  );
}
