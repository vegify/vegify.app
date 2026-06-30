import { ArrowRight, Layers, Laptop, Search } from "lucide-react";
import type { CSSProperties, ReactNode } from "react";
import { buttonClasses } from "./button";
import { SIGNUPS_ENABLED } from "./auth-form";
import type { NavLink } from "./screens";
import { VegifyLogo } from "./vegify-logo";

/**
 * LANDING — the public, unauthenticated marketing page rendered at "/" for logged-out
 * visitors (and crawlers). The app itself lives behind /login; this is the only SEO/GEO
 * surface. It extends the existing brand design system (tokens + serif display + the
 * micronutrient palette) rather than inventing a new look, and renders bare (no AppShell).
 *
 * The hero visual is a data-visualization of the product's actual output (%DV per
 * micronutrient for a sample serving) — the central value prop made visible, not a mock
 * screenshot. Values are illustrative and labeled as such.
 *
 * Web-only: the desktop app is a signed-in native shell and never renders this.
 */

type Nutrient = { name: string; dv: number; color: string; low?: boolean };

// Illustrative %DV for one serving of a hearty plant-based bowl. The story is the point:
// plant foods cover iron/folate/magnesium well and run short on B12 and vitamin D — exactly
// the gaps Vegify surfaces. Raw palette vars come from packages/tokens theme.css.
const SAMPLE: Nutrient[] = [
  { name: "Folate", dv: 62, color: "var(--color-green)" },
  { name: "Iron", dv: 45, color: "var(--color-red)" },
  { name: "Magnesium", dv: 38, color: "var(--color-violet)" },
  { name: "Zinc", dv: 30, color: "var(--color-purple)" },
  { name: "Calcium", dv: 26, color: "var(--color-orange)" },
  { name: "Potassium", dv: 22, color: "var(--color-yellow)" },
  { name: "Vitamin B12", dv: 4, color: "var(--color-magenta)", low: true },
  { name: "Vitamin D", dv: 2, color: "var(--color-magenta)", low: true },
];

const MOTION_CSS = `
@media (prefers-reduced-motion: no-preference) {
  .vg-rise { opacity: 0; animation: vg-rise 0.7s cubic-bezier(0.16, 1, 0.3, 1) both; }
}
@keyframes vg-rise { from { opacity: 0; transform: translateY(18px); } to { opacity: 1; transform: none; } }
`;

export function LandingView({ LinkComponent }: { LinkComponent: NavLink }) {
  // Signups are gated (invite-only) right now, so the primary action is Sign in. When signups
  // open, the primary flips to Get started (/signup) and Sign in becomes the secondary — two
  // distinct intents. The same "Sign in" label is reused in nav + footer (one label per intent).
  const primaryHref = SIGNUPS_ENABLED ? "/signup" : "/login";
  const primaryLabel = SIGNUPS_ENABLED ? "Get started" : "Sign in";

  return (
    <div className="min-h-[100dvh] bg-background text-foreground">
      <style dangerouslySetInnerHTML={{ __html: MOTION_CSS }} />
      <JsonLd />

      {/* ---- Nav ---- */}
      <header className="mx-auto flex h-16 max-w-7xl items-center justify-between px-5 sm:px-8">
        <a
          href="/"
          aria-label="Vegify home"
          className="flex items-center text-primary"
        >
          <VegifyLogo className="h-7 w-auto" />
        </a>
        <nav className="flex items-center gap-2 sm:gap-4">
          <LinkComponent
            href="/login"
            className="rounded-lg px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:text-foreground"
          >
            Sign in
          </LinkComponent>
          {SIGNUPS_ENABLED && (
            <LinkComponent
              href="/signup"
              className={buttonClasses({
                className: "h-9 rounded-lg px-4 text-sm font-semibold",
              })}
            >
              Get started
            </LinkComponent>
          )}
        </nav>
      </header>

      <main>
        {/* ---- Hero: asymmetric split (copy / live %DV readout) ---- */}
        <section className="mx-auto grid max-w-7xl items-center gap-12 px-5 pt-10 pb-20 sm:px-8 md:pt-16 lg:grid-cols-[1.05fr_0.95fr] lg:gap-16">
          <div>
            <p
              className="vg-rise text-sm font-semibold tracking-wide text-primary uppercase"
              style={{ animationDelay: "0ms" }}
            >
              Plant-based micronutrition
            </p>
            <h1
              className="vg-rise mt-4 font-serif text-4xl leading-[1.05] font-bold text-balance text-primary-dark sm:text-5xl lg:text-6xl"
              style={{ animationDelay: "60ms" }}
            >
              Know exactly what your plants are feeding you.
            </h1>
            <p
              className="vg-rise mt-6 max-w-xl text-lg leading-relaxed text-muted-foreground"
              style={{ animationDelay: "120ms" }}
            >
              Vegify tracks the vitamins and minerals in every plant-based
              recipe you cook, not just the calories and macros.
            </p>
            <div
              className="vg-rise mt-9 flex flex-col gap-3 sm:flex-row sm:items-center"
              style={{ animationDelay: "180ms" }}
            >
              <LinkComponent
                href={primaryHref}
                className={buttonClasses({
                  className:
                    "h-12 gap-2 rounded-xl px-6 text-base font-semibold sm:text-lg [&_svg]:size-5",
                })}
              >
                {primaryLabel}
                <ArrowRight aria-hidden />
              </LinkComponent>
              <a
                href="#features"
                className={buttonClasses({
                  variant: "outline",
                  className:
                    "h-12 rounded-xl px-6 text-base font-medium sm:text-lg",
                })}
              >
                See how it works
              </a>
            </div>
          </div>

          <div
            className="vg-rise lg:justify-self-end"
            style={{ animationDelay: "240ms" }}
          >
            <NutrientReadout />
          </div>
        </section>

        {/* ---- Features: asymmetric bento (1 large tinted + 2 supporting) ---- */}
        <section
          id="features"
          aria-labelledby="features-heading"
          className="mx-auto max-w-7xl scroll-mt-20 px-5 py-16 sm:px-8 sm:py-24"
        >
          <h2
            id="features-heading"
            className="max-w-2xl font-serif text-3xl font-bold text-balance text-primary-dark sm:text-4xl"
          >
            More than a calorie counter.
          </h2>
          <div className="mt-10 grid gap-5 lg:grid-cols-3">
            {/* Large tinted cell — the differentiator, with real nutrient chips */}
            <article className="relative overflow-hidden rounded-2xl bg-primary/5 p-7 ring-1 ring-primary/15 lg:col-span-2 lg:p-9">
              <h3 className="font-serif text-2xl font-semibold">
                The micronutrients that matter
              </h3>
              <p className="mt-3 max-w-lg text-muted-foreground">
                Iron, B12, zinc, calcium, omega-3, and vitamin D. The nutrients
                a plant-based diet asks you to watch, tracked for every
                ingredient and rolled into every recipe.
              </p>
              <ul className="mt-7 flex flex-wrap gap-2.5">
                {[
                  "Iron",
                  "Vitamin B12",
                  "Zinc",
                  "Calcium",
                  "Folate",
                  "Magnesium",
                  "Omega-3",
                  "Vitamin D",
                ].map((n, i) => (
                  <li
                    key={n}
                    className="rounded-full px-3.5 py-1.5 text-sm font-medium text-foreground ring-1"
                    style={
                      {
                        backgroundColor: `color-mix(in oklab, ${CHIP_COLORS[i]} 14%, transparent)`,
                        "--tw-ring-color": `color-mix(in oklab, ${CHIP_COLORS[i]} 40%, transparent)`,
                      } as CSSProperties
                    }
                  >
                    {n}
                  </li>
                ))}
              </ul>
            </article>

            {/* Two supporting cells, stacked */}
            <div className="grid gap-5">
              <FeatureCard
                icon={<Search aria-hidden className="size-5 text-primary" />}
                title="One shared catalog"
              >
                Search a communal library of ingredients and recipes. Cook from
                what others share, and publish your own.
              </FeatureCard>
              <FeatureCard
                icon={<Laptop aria-hidden className="size-5 text-primary" />}
                title="Web and a native app"
              >
                Use it in the browser, or in a Mac app that keeps working
                offline and syncs when you reconnect.
              </FeatureCard>
            </div>
          </div>
        </section>

        {/* ---- Nesting spotlight: the unique domain model, made concrete ---- */}
        <section
          aria-labelledby="nesting-heading"
          className="border-y border-border bg-muted/40"
        >
          <div className="mx-auto grid max-w-7xl items-center gap-12 px-5 py-16 sm:px-8 sm:py-24 lg:grid-cols-2 lg:gap-16">
            <div>
              <span className="inline-flex items-center gap-2 text-primary">
                <Layers aria-hidden className="size-5" />
              </span>
              <h2
                id="nesting-heading"
                className="mt-3 font-serif text-3xl font-bold text-balance text-primary-dark sm:text-4xl"
              >
                A recipe is an ingredient.
              </h2>
              <p className="mt-5 max-w-lg text-lg leading-relaxed text-muted-foreground">
                Build a base once, then cook it into the next recipe. A biga
                becomes pizza dough. The dough becomes a Friday night pizza.
                Nutrition rolls up through every layer, so you never re-enter an
                ingredient.
              </p>
            </div>
            <NestFlow />
          </div>
        </section>

        {/* ---- Closing CTA ---- */}
        <section className="mx-auto max-w-7xl px-5 py-20 text-center sm:px-8 sm:py-28">
          <h2 className="mx-auto max-w-2xl font-serif text-3xl font-bold text-balance text-primary-dark sm:text-4xl">
            Cook with the full picture.
          </h2>
          <p className="mx-auto mt-4 max-w-xl text-lg text-muted-foreground">
            See the micronutrition behind everything you make.
          </p>
          <div className="mt-8 flex justify-center">
            <LinkComponent
              href={primaryHref}
              className={buttonClasses({
                className:
                  "h-12 gap-2 rounded-xl px-7 text-base font-semibold sm:text-lg [&_svg]:size-5",
              })}
            >
              {primaryLabel}
              <ArrowRight aria-hidden />
            </LinkComponent>
          </div>
        </section>
      </main>

      {/* ---- Footer ---- */}
      <footer className="border-t border-border">
        <div className="mx-auto flex max-w-7xl flex-col items-start justify-between gap-6 px-5 py-10 sm:flex-row sm:items-center sm:px-8">
          <div>
            <VegifyLogo className="h-6 w-auto text-primary" />
            <p className="mt-2 text-sm text-muted-foreground">
              Micronutrition tracking for plant-based cooking.
            </p>
          </div>
          <div className="flex items-center gap-6 text-sm">
            <LinkComponent
              href="/login"
              className="font-medium text-muted-foreground transition-colors hover:text-foreground"
            >
              Sign in
            </LinkComponent>
            <span className="text-muted-foreground">© 2026 Vegify</span>
          </div>
        </div>
      </footer>
    </div>
  );
}

// Chip accent colors, raw palette vars from packages/tokens (fixed brand hues, same in both modes).
const CHIP_COLORS = [
  "var(--color-red)",
  "var(--color-magenta)",
  "var(--color-purple)",
  "var(--color-orange)",
  "var(--color-green)",
  "var(--color-violet)",
  "var(--color-green-light)",
  "var(--color-yellow)",
];

function FeatureCard({
  icon,
  title,
  children,
}: {
  icon: ReactNode;
  title: string;
  children: ReactNode;
}) {
  return (
    <article className="rounded-2xl bg-card p-7 ring-1 ring-foreground/10 transition duration-150 hover:-translate-y-0.5 hover:shadow-lg hover:ring-primary/40">
      <div className="flex size-10 items-center justify-center rounded-xl bg-primary/10">
        {icon}
      </div>
      <h3 className="mt-4 font-serif text-xl font-semibold">{title}</h3>
      <p className="mt-2 text-sm leading-relaxed text-muted-foreground">
        {children}
      </p>
    </article>
  );
}

/**
 * Hero data-viz: the product's actual output (per-serving %DV per micronutrient) for one
 * illustrative plant-based serving. Bars are decorative; every value is also plain text.
 */
function NutrientReadout() {
  return (
    <div className="w-full max-w-md rounded-2xl bg-card p-6 shadow-xl ring-1 ring-foreground/10 sm:p-7">
      <div className="flex items-baseline justify-between border-b border-border pb-3">
        <div>
          <p className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
            Sample serving
          </p>
          <p className="mt-1 font-serif text-xl font-semibold">
            Lentil &amp; tahini bowl
          </p>
        </div>
        <p className="text-sm font-medium text-muted-foreground">480 kcal</p>
      </div>

      <ul className="mt-4 space-y-3">
        {SAMPLE.map((n) => (
          <li
            key={n.name}
            className="grid grid-cols-[7.5rem_1fr_auto] items-center gap-3"
          >
            <span className="text-sm font-medium">{n.name}</span>
            <span
              className="h-2 overflow-hidden rounded-full bg-muted"
              aria-hidden
            >
              <span
                className="block h-full rounded-full"
                style={{ width: `${n.dv}%`, backgroundColor: n.color }}
              />
            </span>
            {n.low ? (
              <span className="text-xs font-semibold text-magenta">Low</span>
            ) : (
              <span className="text-sm font-semibold tabular-nums">
                {n.dv}%
              </span>
            )}
          </li>
        ))}
      </ul>

      <p className="mt-5 border-t border-border pt-3 text-xs text-muted-foreground">
        Percent of daily value. Illustrative values.
      </p>
    </div>
  );
}

/** Concept diagram for recipe nesting — labeled steps, not a mock screenshot. */
function NestFlow() {
  const steps = ["Biga", "Pizza dough", "Margherita pizza"];
  return (
    <div className="rounded-2xl bg-card p-6 ring-1 ring-foreground/10 sm:p-8">
      <ol className="space-y-3">
        {steps.map((s, i) => (
          <li key={s}>
            <div
              className="flex items-center gap-3 rounded-xl bg-primary/5 px-4 py-3 ring-1 ring-foreground/10"
              style={{ marginLeft: `${i * 1.5}rem` }}
            >
              <span className="flex size-7 shrink-0 items-center justify-center rounded-lg bg-primary/10 font-serif text-sm font-bold text-primary-dark">
                {i + 1}
              </span>
              <span className="font-medium">{s}</span>
            </div>
            {i < steps.length - 1 && (
              <ArrowRight
                aria-hidden
                className="my-1 size-4 rotate-90 text-muted-foreground"
                style={{ marginLeft: `${i * 1.5 + 0.6}rem` }}
              />
            )}
          </li>
        ))}
      </ol>
      <p className="mt-5 border-t border-border pt-4 text-sm text-muted-foreground">
        Iron, calcium, and folate add up automatically.
      </p>
    </div>
  );
}

function JsonLd() {
  const data = {
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "WebSite",
        "@id": "https://vegify.app/#website",
        url: "https://vegify.app/",
        name: "Vegify",
        description: "Micronutrition tracking for plant-based cooking.",
        publisher: { "@id": "https://vegify.app/#org" },
      },
      {
        "@type": "Organization",
        "@id": "https://vegify.app/#org",
        name: "Vegify",
        url: "https://vegify.app/",
        logo: "https://vegify.app/logo512.png",
      },
      {
        "@type": "SoftwareApplication",
        name: "Vegify",
        applicationCategory: "HealthApplication",
        operatingSystem: "Web, macOS",
        url: "https://vegify.app/",
        description:
          "Vegify tracks the vitamins and minerals in plant-based recipes, not just calories and macros. Recipes nest as ingredients so nutrition rolls up automatically.",
        featureList: [
          "Per-ingredient micronutrient tracking",
          "Recipes that nest as ingredients with rolled-up nutrition",
          "Shared catalog of ingredients and recipes",
          "Web app and offline-capable native desktop app",
        ],
        offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
        publisher: { "@id": "https://vegify.app/#org" },
      },
    ],
  };
  return (
    <script
      type="application/ld+json"
      dangerouslySetInnerHTML={{ __html: JSON.stringify(data) }}
    />
  );
}
