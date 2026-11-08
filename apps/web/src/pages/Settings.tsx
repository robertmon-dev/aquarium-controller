import { useNavigate } from "react-router-dom";
import { ScreenMessage } from "@/components/molecules";
import { DeviceForm } from "@/components/organisms/DeviceForm/DeviceForm";
import { DeviceInfo } from "@/components/organisms/DeviceInfo/DeviceInfo";
import { useDevice } from "@/providers/AppProviders";
import { useT } from "@aquarium/i18n/use-locale";

export function Settings() {
  const t = useT();
  const { device, transport, setDevice, loading } = useDevice();
  const navigate = useNavigate();

  if (loading) return <ScreenMessage loading>{t("deviceInfo.checking")}</ScreenMessage>;

  return (
    <>
      <DeviceForm
        device={device}
        onConnected={async (next) => {
          await setDevice(next);
          void navigate("/");
        }}
      />

      {device && transport ? (
        <DeviceInfo device={device} transport={transport} />
      ) : null}
    </>
  );
}
