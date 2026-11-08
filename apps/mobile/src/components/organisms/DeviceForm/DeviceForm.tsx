import {
  ApiError,
  createClient,
  type DeviceConfig,
} from "@aquarium/api-client";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMemo, useState } from "react";
import { useForm } from "react-hook-form";
import { View } from "react-native";
import { Alert, Button, Card } from "@/components/atoms";
import { FormField } from "@/components/molecules";
import { spacing } from "@aquarium/theme";
import {
  deviceFormMessages,
  deviceFormSchema,
  toDeviceConfig,
  type DeviceFormValues,
  type ParsedDeviceForm,
} from "@/device/config";
import { deviceFetch } from "@/device/transport";
import { useT } from "@/i18n/use-locale";

export interface DeviceFormProps {
  device: DeviceConfig | null;
  onConnected: (device: DeviceConfig) => Promise<void>;
}

export function DeviceForm({ device, onConnected }: DeviceFormProps) {
  const t = useT();
  const [probeError, setProbeError] = useState<string | null>(null);
  const [probing, setProbing] = useState(false);
  const schema = useMemo(() => deviceFormSchema(deviceFormMessages(t)), [t]);

  const { control, handleSubmit } = useForm<
    DeviceFormValues,
    unknown,
    ParsedDeviceForm
  >({
    resolver: zodResolver(schema),
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
          ? t("deviceForm.tokenRejected")
          : t("deviceForm.unreachable", {
              host: config.host,
              port: config.port,
            }),
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
          label={t("deviceForm.host")}
          placeholder="192.168.1.50"
        />
        <FormField
          control={control}
          name="port"
          label={t("deviceForm.port")}
          keyboardType="number-pad"
          helperText={t("deviceForm.portHelper")}
        />
        <FormField
          control={control}
          name="token"
          label={t("deviceForm.token")}
          secureTextEntry
        />

        {probeError ? (
          <Alert
            variant="danger"
            title={t("deviceForm.cannotConnect")}
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
          {t("deviceForm.submit")}
        </Button>
      </View>
    </Card>
  );
}
