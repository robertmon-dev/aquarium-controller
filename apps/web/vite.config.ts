import react from "@vitejs/plugin-react";
import path from "node:path";
import { defineConfig, loadEnv } from "vite";

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "");
  const influxToken = env.INFLUX_READ_TOKEN ?? "";

  return {
    plugins: [react()],

    resolve: {
      alias: { "@": path.resolve(import.meta.dirname, "./src") },
    },

    css: {
      preprocessorOptions: {
        scss: {
          additionalData: '@use "@/styles/palette.scss" as *;',
        },
      },
    },

    server: {
      proxy: {
        "/influx": {
          target: env.INFLUX_URL ?? "http://localhost:8086",
          changeOrigin: true,
          rewrite: (p: string) => p.replace(/^\/influx/, ""),
          headers: influxToken ? { Authorization: `Token ${influxToken}` } : {},
        },
      },
    },

    build: { outDir: "dist", emptyOutDir: true },
  };
});
