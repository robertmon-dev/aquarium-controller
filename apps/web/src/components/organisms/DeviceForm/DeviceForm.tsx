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
import { Eye, EyeOff } from "lucide-react";
import { useMemo, useReducer, useState } from "react";
import { useForm } from "react-hook-form";
import { Alert, Button, Card, Input } from "@/components/atoms";
import { deviceFormMessages } from "@/device/form-messages";
import { deviceFetch } from "@/device/transport";
import { initialProbe, probeReducer } from "./probe-reducer";
import "./DeviceForm.scss";

export function DeviceForm({
  device,
  onConnected,
}: {
  device: DeviceConfig | null;
  onConnected: (device: DeviceConfig) => Promise<void>;
}) {
  const t = useT();
  const [probe, dispatch] = useReducer(probeReducer, initialProbe);
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

  async function reach(config: DeviceConfig): Promise<string | null> {
    try {
      await createClient({ device: config, fetchImpl: deviceFetch }).health();
      return null;
    } catch (error) {
      return error instanceof ApiError && error.kind === "unauthorized"
        ? t("deviceForm.tokenRejected")
        : t("deviceForm.unreachable", {
            host: config.host,
            port: config.port,
          });
    }
  }

  const onTest = async () => {
    if (!(await trigger())) return;

    const parsed = schema.safeParse(getValues());
    if (!parsed.success) return;

    dispatch({ type: "start", action: "test" });
    const failure = await reach(toDeviceConfig(parsed.data));
    dispatch(
      failure ? { type: "failed", message: failure } : { type: "reachable" },
    );
  };

  const onSave = handleSubmit(async (values) => {
    const config = toDeviceConfig(values);

    dispatch({ type: "start", action: "save" });
    const failure = await reach(config);

    if (failure) {
      dispatch({ type: "failed", message: failure });
      return;
    }

    dispatch({ type: "reachable" });
    await onConnected(config);
  });

  const probing = probe.status === "probing";

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

        {probe.status === "failed" ? (
          <Alert
            variant="danger"
            title={t("deviceForm.cannotConnect")}
            onClose={() => dispatch({ type: "dismiss" })}
          >
            {probe.message}
          </Alert>
        ) : null}

        {probe.status === "reachable" ? (
          <Alert variant="success" title={t("deviceForm.reachable")}>
            {t("deviceForm.host")}: {getValues("host")}
          </Alert>
        ) : null}

        <div className="device-form__actions">
          <Button
            type="button"
            variant="outline"
            isLoading={probing && probe.action === "test"}
            disabled={probing}
            onClick={() => void onTest()}
          >
            {t("deviceForm.test")}
          </Button>

          <Button
            type="submit"
            variant="primary"
            isLoading={probing && probe.action === "save"}
            disabled={probing}
          >
            {t("deviceForm.submit")}
          </Button>
        </div>
      </form>
    </Card>
  );
}
