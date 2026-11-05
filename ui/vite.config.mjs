import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
const target = "http://127.0.0.1:4280";
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": {
        target,
        changeOrigin: true,
        configure(proxy) {
          proxy.on("proxyReq", (outgoing, incoming) => {
            const origin = incoming.headers.origin;
            if (origin) {
              try {
                if (new URL(origin).host === incoming.headers.host)
                  outgoing.setHeader("Origin", target);
              } catch {
                /* Keep invalid origins for the API to reject. */
              }
            }
          });
        },
      },
    },
  },
  build: { sourcemap: false },
});
