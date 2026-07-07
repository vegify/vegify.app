import { createFileRoute } from "@tanstack/react-router";
import { LandingView } from "@vegify/ui/landing";
import { HomeView } from "@vegify/ui/screens";

import { LinkAdapter } from "../link";

// "/" is dual-purpose: the public marketing landing for logged-out visitors (the app's only
// SEO/GEO surface) and the app home for signed-in users. The auth gate in __root lets "/"
// through unauthenticated; here we branch on the user resolved by that gate.

const TITLE = "Micronutrition tracking for plant-based cooking | Vegify";
const DESCRIPTION =
  "Vegify tracks the vitamins and minerals in every plant-based recipe you cook, not just calories and macros. Recipes nest as ingredients, so nutrition rolls up automatically.";
const URL = "https://vegify.app/";
const IMAGE = "https://vegify.app/logo512.png";

export const Route = createFileRoute("/")({
  head: () => ({
    meta: [
      { title: TITLE },
      { name: "description", content: DESCRIPTION },
      { property: "og:type", content: "website" },
      { property: "og:site_name", content: "Vegify" },
      { property: "og:title", content: TITLE },
      { property: "og:description", content: DESCRIPTION },
      { property: "og:url", content: URL },
      { property: "og:image", content: IMAGE },
      { name: "twitter:card", content: "summary" },
      { name: "twitter:title", content: TITLE },
      { name: "twitter:description", content: DESCRIPTION },
      { name: "twitter:image", content: IMAGE },
    ],
    links: [{ rel: "canonical", href: URL }],
  }),
  component: Home,
});

function Home() {
  const { user } = Route.useRouteContext();
  return user ? (
    <HomeView LinkComponent={LinkAdapter} />
  ) : (
    <LandingView LinkComponent={LinkAdapter} />
  );
}
