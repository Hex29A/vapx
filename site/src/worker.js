// vapx.29a.se: landing page (static assets) plus short install URLs.
//
//   curl -fsSL https://vapx.29a.se/install | sh
//   irm https://vapx.29a.se/install.ps1 | iex
//
// The scripts are served from the latest GitHub release, not from main, so what
// people pipe into a shell is exactly what was tagged and listed in SHA256SUMS.

const RELEASE = "https://github.com/Hex29A/vapx/releases/latest/download/";
const LATEST = "https://github.com/Hex29A/vapx/releases/latest";
const VERSION_TTL = 600; // seconds
const SCRIPTS = {
  "/install": "install.sh",
  "/install.sh": "install.sh",
  "/install.ps1": "install.ps1",
};

// The latest release tag, from where GitHub's /releases/latest redirects (no API
// token, no API rate limit), cached for VERSION_TTL. Null when GitHub cannot be asked.
async function latestVersion(ctx) {
  const cache = caches.default;
  const key = new Request("https://vapx.29a.se/__latest-version");
  const hit = await cache.match(key);
  if (hit) return (await hit.text()) || null;
  let version = null;
  try {
    const r = await fetch(LATEST, { redirect: "manual" });
    const m = /\/tag\/v?(\d+\.\d+\.\d+[^/?#]*)$/.exec(r.headers.get("location") || "");
    if (m) version = m[1];
  } catch (_) {
    // GitHub unreachable: the page is served without the badge.
  }
  if (version) {
    const stored = new Response(version, { headers: { "cache-control": `max-age=${VERSION_TTL}` } });
    ctx.waitUntil(cache.put(key, stored));
  }
  return version;
}

// Pages get the version filled into their badge (<a class="ver" hidden>) on the way out.
async function page(request, env, ctx) {
  const res = await env.ASSETS.fetch(request);
  if (!(res.headers.get("content-type") || "").includes("text/html")) return res;
  const version = await latestVersion(ctx);
  if (!version) return res;
  return new HTMLRewriter()
    .on("a.ver", {
      element(el) {
        el.removeAttribute("hidden");
        el.setAttribute("href", `${LATEST.replace("/latest", "")}/tag/v${version}`);
        el.setInnerContent(`v${version}`);
      },
    })
    .transform(res);
}

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    if (url.pathname === "/version") {
      const version = await latestVersion(ctx);
      return new Response(JSON.stringify({ version }), {
        status: version ? 200 : 502,
        headers: {
          "content-type": "application/json",
          "cache-control": `public, max-age=${VERSION_TTL}`,
          "access-control-allow-origin": "*",
        },
      });
    }
    const file = SCRIPTS[url.pathname];
    if (!file) return page(request, env, ctx);

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
