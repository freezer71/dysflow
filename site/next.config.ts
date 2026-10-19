import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  /* config options here */
  cacheComponents: true,
  partialPrefetching: true,
  // Les anciennes adresses françaises vivent désormais sous /fr.
  async redirects() {
    return [
      { source: "/modeles", destination: "/fr/modeles", permanent: true },
      { source: "/telecharger", destination: "/fr/telecharger", permanent: true },
    ];
  },
  turbopack: {
    rules: {
      "*.css": {
        loaders: ["@tailwindcss/turbopack"],
        as: "*.css",
      },
    },
  },
};

export default nextConfig;
