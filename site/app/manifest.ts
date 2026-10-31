import type { MetadataRoute } from "next";
import { dico } from "./_i18n";

export default function manifest(): MetadataRoute.Manifest {
  return {
    name: "Dysflow",
    short_name: "Dysflow",
    description: dico("en").meta.description,
    start_url: "/",
    display: "browser",
    background_color: "#f8f3e6",
    theme_color: "#f8f3e6",
    icons: [
      { src: "/icon.svg", sizes: "any", type: "image/svg+xml" },
      { src: "/apple-icon", sizes: "180x180", type: "image/png" },
      { src: "/icon-192.png", sizes: "192x192", type: "image/png" },
      { src: "/icon-512.png", sizes: "512x512", type: "image/png" },
    ],
  };
}
