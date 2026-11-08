import {
  ApiError,
  createClient,
  deviceFormSchema,
  toDeviceConfig,
  type DeviceConfig,
  type DeviceFormValues,
  type ParsedDeviceForm,
} from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMemo, useState } from "react";
import { useForm } from "react-hook-form";
import { Alert, Button, Card, Input } from "@/components/atoms";
import { deviceFormMessages } from "@/device/form-messages";
import { deviceFetch } from "@/device/transport";

export function DeviceForm({
  device,
  onConnected,
}: {
  device: DeviceConfig | null;
  onConnected: (device: DeviceConfig) => Promise<void>;
}) {
  const t = useT();
  const [probeError, setProbeError] = useState<string | null>(null);
  const [probing, setProbing] = useState(false);
  const schema = useMemo(() => deviceFormSchema(deviceFormMessages(t)), [t]);

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<DeviceFormValues, unknown, ParsedDeviceForm>({
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
      <form onSubmit={(event) => void onSubmit(event)} noValidate>
        <Input
          label={t("deviceForm.host")}
          placeholder="192.168.1.50"
          fullWidth
          error={errors.host?.message ?? false}
          {...register("host")}
        />
        <Input
          label={t("deviceForm.port")}
          helperText={t("deviceForm.portHelper")}
          fullWidth
          error={errors.port?.message ?? false}
          {...register("port")}
        />
        <Input
          label={t("deviceForm.token")}
          type="password"
          fullWidth
          error={errors.token?.message ?? false}
          {...register("token")}
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

        <Button type="submit" variant="primary" fullWidth isLoading={probing}>
          {t("deviceForm.submit")}
        </Button>
      </form>
    </Card>
  );
}
