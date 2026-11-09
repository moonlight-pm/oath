import { defineConfig } from "cf/config";

// The public name is not in the repo: the workflow passes it in. Without one
// the site deploys to its workers.dev address only.
const domain = process.env.SITE_DOMAIN;

export default defineConfig({
  worker: {
    name: "oath-site",
    compatibilityDate: "2026-11-09",
    assets: {
      htmlHandling: "auto-trailing-slash",
      notFoundHandling: "404-page",
    },
    domains: domain ? [domain] : [],
    workersDev: !domain,
  },
});
