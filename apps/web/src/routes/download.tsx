import { queryOptions, useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { createServerFn } from "@tanstack/react-start";
import { buttonClasses } from "@vegify/ui/button";

import { LinkAdapter } from "../link";

// The public GitHub repo (same value the deploy config resolves); the releases API is unauthenticated
// for public repos. The macOS build is the universal .dmg tauri-action attaches to each release.
const REPO = "vegify/vegify.app";
const RELEASES_PAGE = `https://github.com/${REPO}/releases/latest`;

type DownloadInfo = { version: string | null; dmgUrl: string | null };

// Resolve the latest release's macOS .dmg once (cached): the asset name carries the version, so
// there's no stable direct link — we fetch the asset URL. Degrades to the releases page on any error.
const getLatest = createServerFn({ method: "GET" }).handler(
  async (): Promise<DownloadInfo> => {
    try {
      const res = await fetch(
        `https://api.github.com/repos/${REPO}/releases/latest`,
        {
          headers: {
            accept: "application/vnd.github+json",
            "user-agent": "vegify.app",
          },
        },
      );
      if (!res.ok) return { version: null, dmgUrl: null };
      const rel = (await res.json()) as {
        tag_name?: string;
        assets?: { name: string; browser_download_url: string }[];
      };
      const dmg = rel.assets?.find((a) =>
        a.name.toLowerCase().endsWith(".dmg"),
      );
      return {
        version: rel.tag_name ?? null,
        dmgUrl: dmg?.browser_download_url ?? null,
      };
    } catch {
      return { version: null, dmgUrl: null };
    }
  },
);

const latestQuery = queryOptions({
  queryKey: ["desktop-latest"],
  queryFn: () => getLatest(),
  staleTime: 60 * 60 * 1000, // an hour — releases are infrequent
});

export const Route = createFileRoute("/download")({
  loader: ({ context }) => context.queryClient.ensureQueryData(latestQuery),
  component: DownloadPage,
});

function DownloadPage() {
  const { data } = useSuspenseQuery(latestQuery);
  // The direct .dmg when we resolved it; otherwise the releases page (always works).
  const href = data.dmgUrl ?? RELEASES_PAGE;
  return (
    <div className="mx-auto max-w-2xl p-8 text-center">
      <h1 className="mb-2 font-serif text-4xl font-bold text-primary-dark dark:text-primary-light">
        Vegify for desktop
      </h1>
      <p className="mb-8 text-muted-foreground">
        The native macOS app — local-first, offline-capable, with realtime sync.
        {data.version ? (
          <span className="ml-1 tabular-nums">({data.version})</span>
        ) : null}
      </p>
      <a href={href} className={buttonClasses({ size: "lg" })} download>
        Download for macOS
      </a>
      <p className="mt-4 text-sm text-muted-foreground">
        Universal build (Apple Silicon + Intel).{" "}
        <a href={RELEASES_PAGE} className="underline hover:text-foreground">
          All releases
        </a>
        .
      </p>
      <p className="mt-10 text-sm text-muted-foreground">
        Prefer the browser?{" "}
        <LinkAdapter href="/recipes" className="text-primary underline">
          Keep browsing on the web
        </LinkAdapter>
        .
      </p>
    </div>
  );
}
