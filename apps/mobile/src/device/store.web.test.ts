import { beforeEach, describe, expect, it } from "vitest";
import { clearDevice, loadDevice, saveDevice } from "./store.web";

const device = { host: "192.168.1.50", port: 80, token: "abc123" };

function fakeStorage() {
  const entries = new Map<string, string>();
  return {
    getItem: (key: string) => entries.get(key) ?? null,
    setItem: (key: string, value: string) => {
      entries.set(key, value);
    },
    removeItem: (key: string) => {
      entries.delete(key);
    },
  };
}

describe("the web device store", () => {
  beforeEach(() => {
    Object.defineProperty(globalThis, "localStorage", {
      value: fakeStorage(),
      configurable: true,
    });
  });

  it("returns null with nothing stored", async () => {
    await expect(loadDevice()).resolves.toBeNull();
  });

  it("round-trips a saved device", async () => {
    await saveDevice(device);

    await expect(loadDevice()).resolves.toEqual(device);
  });

  it("returns null when the stored entry is not json", async () => {
    globalThis.localStorage.setItem("aquarium.device", "{nope");

    await expect(loadDevice()).resolves.toBeNull();
  });

  it("forgets the device once cleared", async () => {
    await saveDevice(device);

    await clearDevice();

    await expect(loadDevice()).resolves.toBeNull();
  });
});
