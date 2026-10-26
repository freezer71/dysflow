import type { MetadataRoute } from "next";
import { absolu } from "./_data/schema";
import { site } from "./_i18n";

export default function robots(): MetadataRoute.Robots {
  return {
    rules: { userAgent: "*", allow: "/" },
    sitemap: absolu("/sitemap.xml"),
    host: site,
  };
}
