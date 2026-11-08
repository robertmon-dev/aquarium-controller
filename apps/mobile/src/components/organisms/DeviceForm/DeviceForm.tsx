import type { DeviceConfig } from "@aquarium/api-client";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMemo } from "react";
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
import { useDeviceProbe } from "@aquarium/view-model";
import { useT } from "@/i18n/use-locale";

export interface DeviceFormProps {
  device: DeviceConfig | null;
  onConnected: (device: DeviceConfig) => Promise<void>;
}

export function DeviceForm({ device, onConnected }: DeviceFormProps) {
  const t = useT();
  const probe = useDeviceProbe(deviceFetch);
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

    if (await probe.run("save", config)) await onConnected(config);
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

        {probe.state.status === "failed" ? (
          <Alert
            variant="danger"
            title={t("deviceForm.cannotConnect")}
            onClose={probe.dismiss}
          >
            {probe.state.message}
          </Alert>
        ) : null}

        <Button
          variant="primary"
          fullWidth
          isLoading={probe.probing}
          onPress={() => void onSubmit()}
        >
          {t("deviceForm.submit")}
        </Button>
      </View>
    </Card>
  );
}
