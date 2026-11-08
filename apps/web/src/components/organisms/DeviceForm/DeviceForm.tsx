import {
  deviceFormSchema,
  toDeviceConfig,
  type DeviceConfig,
  type DeviceFormValues,
  type ParsedDeviceForm,
} from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { useDeviceProbe } from "@aquarium/view-model";
import { zodResolver } from "@hookform/resolvers/zod";
import { Eye, EyeOff } from "lucide-react";
import { useMemo, useState } from "react";
import { useForm } from "react-hook-form";
import { Alert, Button, Card, Input } from "@/components/atoms";
import { deviceFormMessages } from "@/device/form-messages";
import { deviceFetch } from "@/device/transport";
import "./DeviceForm.scss";

export function DeviceForm({
  device,
  onConnected,
}: {
  device: DeviceConfig | null;
  onConnected: (device: DeviceConfig) => Promise<void>;
}) {
  const t = useT();
  const probe = useDeviceProbe(deviceFetch);
  const [showToken, setShowToken] = useState(false);
  const schema = useMemo(() => deviceFormSchema(deviceFormMessages(t)), [t]);

  const {
    register,
    handleSubmit,
    trigger,
    getValues,
    formState: { errors },
  } = useForm<DeviceFormValues, unknown, ParsedDeviceForm>({
    resolver: zodResolver(schema),
    defaultValues: {
      host: device?.host ?? "",
      port: device ? String(device.port) : "80",
      token: device?.token ?? "",
    },
  });

  const onTest = async () => {
    if (!(await trigger())) return;

    const parsed = schema.safeParse(getValues());
    if (!parsed.success) return;

    await probe.run("test", toDeviceConfig(parsed.data));
  };

  const onSave = handleSubmit(async (values) => {
    const config = toDeviceConfig(values);

    if (await probe.run("save", config)) await onConnected(config);
  });

  return (
    <Card variant="outlined" padding="md" width="full">
      <h3 className="device-form__title">{t("deviceForm.title")}</h3>

      <form
        className="device-form"
        onSubmit={(event) => void onSave(event)}
        noValidate
      >
        <div className="device-form__row">
          <Input
            label={t("deviceForm.host")}
            placeholder="192.168.1.25"
            fullWidth
            error={errors.host?.message ?? false}
            {...register("host")}
          />
          <Input
            label={t("deviceForm.port")}
            helperText={t("deviceForm.portHelper")}
            inputMode="numeric"
            error={errors.port?.message ?? false}
            {...register("port")}
          />
        </div>

        <Input
          label={t("deviceForm.token")}
          type={showToken ? "text" : "password"}
          fullWidth
          error={errors.token?.message ?? false}
          rightIcon={
            <button
              type="button"
              className="device-form__reveal"
              onClick={() => setShowToken((shown) => !shown)}
              aria-label={
                showToken
                  ? t("deviceForm.hideToken")
                  : t("deviceForm.showToken")
              }
            >
              {showToken ? <EyeOff size={16} /> : <Eye size={16} />}
            </button>
          }
          {...register("token")}
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

        {probe.state.status === "reachable" ? (
          <Alert variant="success" title={t("deviceForm.reachable")}>
            {t("deviceForm.host")}: {getValues("host")}
          </Alert>
        ) : null}

        <div className="device-form__actions">
          <Button
            type="button"
            variant="outline"
            isLoading={probe.state.status === "probing" && probe.state.action === "test"}
            disabled={probe.probing}
            onClick={() => void onTest()}
          >
            {t("deviceForm.test")}
          </Button>

          <Button
            type="submit"
            variant="primary"
            isLoading={probe.state.status === "probing" && probe.state.action === "save"}
            disabled={probe.probing}
          >
            {t("deviceForm.submit")}
          </Button>
        </div>
      </form>
    </Card>
  );
}
