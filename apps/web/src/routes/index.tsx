import { createFileRoute } from "@tanstack/react-router"
import { LandingView } from "@vegify/ui/landing"
import { HomeView } from "@vegify/ui/screens"

import { LinkAdapter } from "../link"
import { pageHead, siteOrigin } from "../seo"

// "/" is dual-purpose: the public marketing landing for logged-out visitors and the app home for
// signed-in users. The auth gate in __root lets "/" through unauthenticated; here we branch on the
// user resolved by that gate.

export const Route = createFileRoute("/")({
  head: async () =>
    pageHead({
      origin: await siteOrigin(),
      path: "/",
      title: "Micronutrition tracking for plant-based cooking",
      description:
        "Vegify tracks the vitamins and minerals in every plant-based recipe you cook, not just calories and macros. Recipes nest as ingredients, so nutrition rolls up automatically."
    }),
  component: Home
})

function Home() {
  const { user } = Route.useRouteContext()
  return user ? (
    <HomeView LinkComponent={LinkAdapter} />
  ) : (
    <LandingView LinkComponent={LinkAdapter} />
  )
}
