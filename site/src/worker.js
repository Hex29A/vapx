// vapx.29a.se: landing page (static assets) plus short install URLs.
//
//   curl -fsSL https://vapx.29a.se/install | sh
//   irm https://vapx.29a.se/install.ps1 | iex
//
// The scripts are served from the latest GitHub release, not from main, so what
// people pipe into a shell is exactly what was tagged and listed in SHA256SUMS.

const RELEASE = "https://github.com/Hex29A/vapx/releases/latest/download/";
const SCRIPTS = {
  "/install": "install.sh",
  "/install.sh": "install.sh",
  "/install.ps1": "install.ps1",
};

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const file = SCRIPTS[url.pathname];
    if (!file) return env.ASSETS.fetch(request);

    const upstream = await fetch(RELEASE + file, {
      redirect: "follow",
      cf: { cacheTtl: 300, cacheEverything: true },
    });
    if (!upstream.ok) {
      return new Response(`# vapx: could not fetch ${file} from the latest release (${upstream.status})\nexit 1\n`, {
        status: 502,
        headers: { "content-type": "text/plain; charset=utf-8" },
      });
    }
    return new Response(upstream.body, {
      headers: {
        "content-type": "text/plain; charset=utf-8",
        "cache-control": "public, max-age=300",
        "x-content-type-options": "nosniff",
      },
    });
  },
};
