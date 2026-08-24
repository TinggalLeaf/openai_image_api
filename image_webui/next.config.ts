import type { NextConfig } from "next";

const BACKEND = process.env.BACKEND_URL ?? "http://127.0.0.1:8000";

const nextConfig: NextConfig = {
  // The dashboard is shipped as a static bundle (`out/`) that the Rust
  // backend embeds and serves from the same origin in production. We
  // skip Next's image optimizer (it requires a Node server) and
  // disable powered-by header for a smaller attack surface.
  output: "export",
  images: { unoptimized: true },

  // In dev, Next.js 16 blocks cross-origin requests for dev resources
  // (`/_next/static/...`, `/_next/hmr`, ...) unless the host is on this
  // list. Without this, opening the page via `127.0.0.1` (vs `localhost`)
  // returns 403 for the React client chunks, hydration never happens,
  // and the page is stuck on whatever the server rendered.
  allowedDevOrigins: ["127.0.0.1", "localhost", "0.0.0.0"],

  // Dev-only proxy. `lib/api.ts` switches `BACKEND_BASE` between
  // `/backend` (dev) and `""` (prod) based on `NODE_ENV`, so this
  // rewrite is never reached in the static export.
  rewrites() {
    return [
      {
        source: "/backend/:path*",
        destination: `${BACKEND}/:path*`,
      },
    ];
  },

  // Make sure the static export does not try to pre-render dynamic
  // routes we now serve as query-string routes.
  trailingSlash: false,
};

export default nextConfig;
