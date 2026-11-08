import { useNavigate } from "react-router-dom";
import { DeviceForm } from "@/components/organisms/DeviceForm/DeviceForm";
import { useDevice } from "@/providers/AppProviders";

export function Settings() {
  const { device, setDevice } = useDevice();
  const navigate = useNavigate();

  return (
    <DeviceForm
      device={device}
      onConnected={async (next) => {
        await setDevice(next);
        void navigate("/");
      }}
    />
  );
}
