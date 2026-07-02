import type { ReactNode } from "react";
import type { NavLink } from "./screens";
import { VegifyLogo } from "./vegify-logo";
import { NutrientByGroupChart, NutrientRangeChart } from "./blog-charts";

/**
 * BLOG — the public, unauthenticated writing surface (vegify.app/blog), the GEO/SEO layer's
 * citable content. Like the landing it renders bare (no AppShell), extends the brand system
 * (serif display + tokens), and is web-only: the native shells never render it.
 *
 * Posts are authored here in code (JSX bodies in BLOG_POSTS), not markdown — same convention
 * as the landing copy: type-checked, bundled, zero pipeline. Each post carries its own
 * citations as plain external links; internal navigation goes through the shell's
 * `LinkComponent` port so the web router client-navigates.
 */

export type BlogPost = {
  slug: string;
  title: string;
  /** Meta/OG description — also the index-card teaser. */
  description: string;
  /** ISO date for meta + JSON-LD. */
  datePublished: string;
  /** Human date for the byline. */
  dateDisplay: string;
  body: ReactNode;
};

// Shared inline styles for post bodies, applied via arbitrary variants on the <article> wrapper
// (keeps the body markup plain p/ul/strong/a). Links are real external citations — underlined,
// brand-dark, same tab.
const ARTICLE_CLASSES = [
  "space-y-5 text-[17px] leading-relaxed text-foreground",
  "[&_a]:font-medium [&_a]:text-primary-dark dark:[&_a]:text-primary-light [&_a]:underline [&_a]:underline-offset-2 hover:[&_a]:text-primary dark:hover:[&_a]:text-primary",
  "[&_strong]:font-semibold",
  "[&_ul]:list-disc [&_ul]:space-y-3 [&_ul]:pl-6",
].join(" ");

export const BLOG_POSTS: BlogPost[] = [
  {
    slug: "no-such-thing-as-100-percent",
    title: "There's no such thing as 100%",
    description:
      "The label says 100% Vitamin C. 100% of what, and for whom? A short, honest tour of RDA, UL, and the numbers behind the number.",
    datePublished: "2026-07-02",
    dateDisplay: "July 2, 2026",
    body: (
      <>
        <p>
          Your cereal box says "Vitamin C: 100%." A hundred percent of what? Of a number some very
          careful scientists picked for an average adult who is not you.
        </p>
        <p>That's not a scam. It's a compromise. And once you see how it's built, the label reads differently.</p>
        <p>
          <strong>The alphabet soup, decoded.</strong> All these acronyms live under one umbrella: the{" "}
          <strong>DRI</strong>, the Dietary Reference Intakes the US and Canada publish (Europe has its
          own, called DRVs; they mostly agree). The DRI isn't one number. It's a small family (
          <a href="https://ods.od.nih.gov/HealthInformation/Dietary_Reference_Intakes.aspx">NIH ODS</a>):
        </p>
        <ul>
          <li>
            <strong>EAR</strong> — the amount that covers <em>half</em> of people. Useful to scientists,
            useless to you: aim for the EAR and it's a coin flip whether you came up short.
          </li>
          <li>
            <strong>RDA</strong> — the EAR plus a safety cushion, set to cover about 97–98% of healthy
            people. THIS is the "aim for roughly here" number. Most labels are built on it.
          </li>
          <li>
            <strong>AI</strong> — when the data's too thin to draw an RDA, they set an "Adequate Intake"
            instead: an honest best guess. A lot of nutrients are AIs, not RDAs. The science isn't finished.
          </li>
          <li>
            <strong>UL</strong> — the ceiling. The most you can take every day before the risk of{" "}
            <em>harm</em> starts to climb. Not a target. A guardrail.
          </li>
        </ul>
        <p>
          <strong>So a nutrient isn't a number. It's a range.</strong> Picture a line: a floor on the
          left (go under, you're deficient), a target in the middle, a ceiling on the right. You want to
          live in the wide, comfortable part.
        </p>
        <figure className="my-8 rounded-xl bg-card p-5 ring-1 ring-foreground/10">
          <figcaption className="mb-3 text-sm font-semibold text-foreground">
            Vitamin C — aim ~90&nbsp;mg, ceiling 2,000&nbsp;mg
          </figcaption>
          <NutrientRangeChart name="Vitamin C" target={90} ceiling={2000} unit="mg" />
          <p className="mt-2 text-sm text-muted-foreground">
            Roughly 22× headroom between the target and the ceiling. This is why nobody's ever hurt
            themselves with an orange (
            <a href="https://ods.od.nih.gov/factsheets/VitaminC-HealthProfessional/">NIH ODS, Vitamin C</a>).
          </p>
        </figure>
        <p>Other nutrients give you far less room.</p>
        <figure className="my-8 rounded-xl bg-card p-5 ring-1 ring-foreground/10">
          <figcaption className="mb-3 text-sm font-semibold text-foreground">
            Zinc — aim ~11&nbsp;mg, ceiling 40&nbsp;mg
          </figcaption>
          <NutrientRangeChart name="Zinc" target={11} ceiling={40} unit="mg" />
          <p className="mt-2 text-sm text-muted-foreground">
            Same picture, a much tighter band — under 4× headroom (
            <a href="https://ods.od.nih.gov/factsheets/Zinc-HealthProfessional/">NIH ODS, Zinc</a>). The
            shape of the safe range is different for every nutrient.
          </p>
        </figure>
        <p>
          <strong>Not every nutrient even has a ceiling.</strong> Thiamin, riboflavin, B12, biotin,
          vitamin K — no UL has been set for any of them. That does NOT mean "megadose freely." It means
          the evidence for harm at high intakes is too thin to draw the line. Absence of a ceiling is
          absence of data, not a green light. (That's the kind of honesty a label has no room to print.)
        </p>
        <p>
          <strong>Now, the label's one number.</strong> The %DV — Percent Daily Value — is a single
          figure the FDA picked for everyone, on a 2,000-calorie basis (the current set took effect in
          2020;{" "}
          <a href="https://www.fda.gov/food/nutrition-facts-label/daily-value-nutrition-and-supplement-facts-labels">
            FDA
          </a>
          ). One number. For a pregnant 30-year-old, a sedentary 70-year-old, a teenage athlete, and you.
        </p>
        <p>Take iron.</p>
        <figure className="my-8 rounded-xl bg-card p-5 ring-1 ring-foreground/10">
          <figcaption className="mb-3 text-sm font-semibold text-foreground">
            Iron — the same nutrient, three different targets
          </figcaption>
          <NutrientByGroupChart
            unit="mg"
            ceiling={45}
            groups={[
              { label: "Men 19–50", value: 8 },
              { label: "Women 19–50", value: 18 },
              { label: "Pregnancy", value: 27 },
            ]}
          />
          <p className="mt-2 text-sm text-muted-foreground">
            A man needs about 8&nbsp;mg; a menstruating woman, 18; pregnancy, 27 — all under the same
            45&nbsp;mg ceiling (
            <a href="https://ods.od.nih.gov/factsheets/Iron-HealthProfessional/">NIH ODS, Iron</a>). Same
            label, wildly different "100%." The %DV can't tell you which one you are. It was never trying
            to — it's a shelf-comparison tool, not a personal target.
          </p>
        </figure>
        <p>
          <strong>Here's the part where I talk you off the ledge.</strong> None of this means you should
          start weighing food and running spreadsheets at dinner. Eat a varied diet — actual plants, a
          real mix — and you land in the comfortable middle on almost everything without trying. The
          numbers matter at the <em>edges</em>: a nutrient you're genuinely low on, or a supplement you're
          taking by the fistful. That's it.
        </p>
        <p>
          And I'll say the vegan part plainly, because I always do: the one number a plant-based eater
          should actually put on the calendar is B12. Supplement it (
          <a href="https://ods.od.nih.gov/factsheets/VitaminB12-HealthProfessional/">NIH ODS, B12</a>).
          It's cheap, it's non-negotiable, and — see above — it has no ceiling to fret about. Everything
          else, breathe.
        </p>
        <p>
          <strong>Why Vegify cares.</strong> Because "100% of the label" is the wrong question, and we'd
          rather answer the right one: are you in <em>your</em> range, for the nutrients that matter to{" "}
          <em>you</em>? That means storing nutrients as floors, targets, and ceilings — not single
          numbers — and eventually letting the range fit the actual person doing the cooking. That
          feature's coming. This post is the groundwork.
        </p>
        <p>The label isn't lying to you. It's just answering an easier question than the one you have. :)</p>
        <p className="text-sm text-muted-foreground">
          <em>
            (Reference values from the NIH Office of Dietary Supplements; adult US DRIs. General
            information, not medical advice — targets shift with age, sex, pregnancy, and health.)
          </em>
        </p>
      </>
    ),
  },
  {
    slug: "vegan-honestly",
    title: "Vegan, honestly: what a decade of research actually changed",
    description:
      "A decade of vegan-nutrition research, honestly: what actually changed, the caveats vegan blogs skip, and the one non-negotiable.",
    datePublished: "2026-07-01",
    dateDisplay: "July 1, 2026",
    body: (
      <>
        <p>
          Let's get the boring part out of the way: a well-planned vegan diet is fine for a healthy
          adult. Every major nutrition body agrees. That's been settled since the 2000s, and if you
          came here for me to tell you kale is a miracle, I'm going to disappoint you.
        </p>
        <p>
          The interesting part is what <em>changed</em>.
        </p>
        <p>
          <strong>"Plant-based" stopped meaning "healthy."</strong> This is the big one. Researchers
          split plant diets into two piles: the healthful kind (whole grains, legumes, fruit, veg,
          nuts) and the unhealthful kind (refined grains, sweets, soda, ultra-processed everything).
          The first drops heart-disease and diabetes risk by a quarter or more. The second{" "}
          <em>raises</em> it (
          <a href="https://pmc.ncbi.nlm.nih.gov/articles/PMC5555375/">Satija et al., JACC 2017</a>).
          A vegan living on fries and white bread is technically plant-based and measurably worse
          off than your friend eating a balanced omnivore plate. So "is it vegan" was never the
          right question. "Is it food" is closer.
        </p>
        <p>
          <strong>The evidence got harder to argue with.</strong> Nutrition studies are mostly
          observational — they watch people who already eat a certain way, and healthy-user bias
          haunts all of it. Then in 2023 Stanford ran identical twins. Same genes, same upbringing,
          one twin vegan, one omnivore, eight weeks. The vegan twins came out with lower LDL, lower
          fasting insulin, more weight loss (
          <a href="https://jamanetwork.com/journals/jamanetworkopen/fullarticle/2812392">
            Landry &amp; Gardner, JAMA Network Open 2023
          </a>
          ). That's about as close as nutrition gets to controlling for "well, they were healthier
          to begin with."
        </p>
        <p>
          <strong>The institutions moved — and got more honest, not less.</strong> Germany's DGE
          spent years effectively steering people away from vegan diets. In 2024 they reversed: fine
          for healthy adults with B12 (
          <a href="https://www.dge.de/wissenschaft/stellungnahmen-und-positionspapiere/positionen/update-of-the-dge-position-on-vegan-diet/">
            DGE 2024
          </a>
          ). A big cautious body admitting it was wrong. I respect that. And then the Academy of
          Nutrition and Dietetics did the opposite of cheerleading: in 2025 they <em>narrowed</em>{" "}
          their famous "all life stages" line down to healthy adults, explicitly carving out
          pregnancy and lactation (
          <a href="https://pubmed.ncbi.nlm.nih.gov/39923894/">AND 2025</a>). Not because veganism
          got worse — because the evidence for pregnancy and kids is thin, and they said so. When
          the pro-plant body tightens its own claims, trust it more, not less.
        </p>
        <p>
          <strong>Now the part vegan blogs skip: the caveats.</strong> I'd rather you hear them from
          me.
        </p>
        <ul>
          <li>
            <strong>Bone fractures.</strong> Vegans have shown meaningfully higher fracture risk in
            large cohorts. Most of it vanishes once calcium, vitamin D, B12, and protein are
            adequate (
            <a href="https://pmc.ncbi.nlm.nih.gov/articles/PMC7613518/">
              Tong et al., BMC Medicine 2020
            </a>
            ). The catch: a lot of vegans aren't adequate on those. Don't be that vegan.
          </li>
          <li>
            <strong>Hemorrhagic stroke.</strong> EPIC-Oxford found a higher signal in vegetarians,
            possibly a B12 story (
            <a href="https://www.bmj.com/content/366/bmj.l4897">Tong et al., BMJ 2019</a>).
          </li>
          <li>
            <strong>Colorectal cancer.</strong> A 2026 pooled analysis of 1.8 million people flagged
            a higher signal specifically in vegans. Small subgroup, hotly debated, and the Adventist
            data point the other way — but I won't pretend it isn't there (
            <a href="https://www.nature.com/articles/s41416-025-03327-4">
              Fraser et al., Br J Cancer 2026
            </a>
            ). Watch the space.
          </li>
        </ul>
        <p>None of these are "gotcha, veganism bad." They're "plan it, and get bloodwork."</p>
        <p>
          <strong>The one non-negotiable.</strong> B12. Take it — 250 mcg a day or 2,000 a week. No
          reliable plant source, there never was one, and "but nutritional yeast" is not a plan.
          After that: iodine (not kelp — kelp doses are a lottery), algae-based omega-3, check your
          vitamin D, watch iron if you menstruate, hit calcium and protein with variety. That's the
          whole list. It fits on an index card.
        </p>
        <p>
          <strong>And the protein thing.</strong> "But where do you get your protein?" …plants. I
          get my protein from plants. The myth that you can't build muscle without meat is dead —
          with adequate protein spread through the day, plant eaters gain and perform right
          alongside everyone else (
          <a href="https://r.jordan.im/download/nutrition/hevia-larra%C3%ADn2021.pdf">
            Hevia-Larraín 2021
          </a>
          ). And "combine your proteins at every meal"? The woman who popularized that in 1971
          retracted it in 1981. You've been carrying a 45-year-old correction's worth of guilt for
          nothing.
        </p>
        <p>
          <strong>Where I land, and where Vegify lands:</strong> this isn't a purity contest. Not
          everyone can go vegan tomorrow. Food deserts are real, poverty is real, eating disorders
          are real. If someone's grocery run is a gas station, "just eat more lentils" is a
          privileged thing to say, and I won't say it. The goal was never compliance. It's access —
          more good plant options on more shelves, so the day you get curious, it's an easy yes.
          Maybe that's today. Maybe it's a Tuesday five years from now when your regular spot
          finally stocks a decent oat milk and you switch without thinking about it. That counts
          too. That's the whole game.
        </p>
        <p>
          The science is on solid ground. The caveats are real and manageable. The door's open
          whenever you are. :)
        </p>
        <p className="text-sm text-muted-foreground">
          <em>
            (Research synthesis, not medical advice. Before changing supplements, get baseline
            bloodwork and talk to a dietitian or doctor.)
          </em>
        </p>
      </>
    ),
  },
];

export const postBySlug = (slug: string): BlogPost | undefined =>
  BLOG_POSTS.find((p) => p.slug === slug);

/** Slim bare-page chrome shared by the index and post pages (the blog renders without the AppShell). */
function BlogChrome({ LinkComponent, children }: { LinkComponent: NavLink; children: ReactNode }) {
  return (
    <div className="min-h-screen bg-background text-foreground">
      <header className="bg-green-dark">
        <div className="mx-auto flex max-w-2xl items-center justify-between px-6 py-4">
          {/* text-white: the mark inherits currentColor; without it light mode paints it near-black on the green band. */}
          <LinkComponent href="/" className="block w-28 text-white" aria-label="Vegify home">
            <VegifyLogo className="h-auto w-full" />
          </LinkComponent>
          <nav className="flex items-center gap-5 text-sm font-semibold text-white/90">
            <LinkComponent href="/blog" className="transition hover:text-white">
              Blog
            </LinkComponent>
            <LinkComponent href="/recipes" className="transition hover:text-white">
              Browse recipes
            </LinkComponent>
          </nav>
        </div>
      </header>
      <main className="mx-auto max-w-2xl px-6 py-12">{children}</main>
      <footer className="mx-auto max-w-2xl px-6 pb-12 text-sm text-muted-foreground">
        <LinkComponent href="/" className="font-medium text-primary-dark dark:text-primary-light underline underline-offset-2">
          Vegify
        </LinkComponent>
        <span className="mx-2" aria-hidden>·</span>
        micronutrition tracking for plant-based cooking.
      </footer>
    </div>
  );
}

export function BlogIndexView({ LinkComponent }: { LinkComponent: NavLink }) {
  return (
    <BlogChrome LinkComponent={LinkComponent}>
      <h1 className="mb-2 font-serif text-4xl font-bold text-primary-dark dark:text-primary-light">Blog</h1>
      <p className="mb-10 text-muted-foreground">
        Notes on plant-based nutrition: research-led, citations included, honest about the caveats.
      </p>
      <ul className="space-y-8">
        {BLOG_POSTS.map((post) => (
          <li key={post.slug}>
            <LinkComponent href={`/blog/${post.slug}`} className="group block">
              <h2 className="font-serif text-2xl font-semibold group-hover:text-primary-dark dark:group-hover:text-primary-light">
                {post.title}
              </h2>
              <p className="mt-1 text-sm text-muted-foreground">{post.dateDisplay}</p>
              <p className="mt-2 text-muted-foreground">{post.description}</p>
            </LinkComponent>
          </li>
        ))}
      </ul>
    </BlogChrome>
  );
}

/** Article JSON-LD, mirroring the landing's inline-script pattern — the GEO-citable node. */
function BlogPostJsonLd({ post }: { post: BlogPost }) {
  const data = {
    "@context": "https://schema.org",
    "@type": "Article",
    headline: post.title,
    description: post.description,
    datePublished: post.datePublished,
    url: `https://vegify.app/blog/${post.slug}`,
    mainEntityOfPage: `https://vegify.app/blog/${post.slug}`,
    author: { "@type": "Person", name: "John M. Carmack", url: "https://vegify.app" },
    publisher: { "@type": "Organization", name: "Vegify", url: "https://vegify.app/" },
  };
  return (
    <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(data) }} />
  );
}

export function BlogPostView({ post, LinkComponent }: { post: BlogPost; LinkComponent: NavLink }) {
  return (
    <BlogChrome LinkComponent={LinkComponent}>
      <BlogPostJsonLd post={post} />
      <p className="mb-3 text-sm text-muted-foreground">
        <time dateTime={post.datePublished}>{post.dateDisplay}</time>
        <span className="mx-2" aria-hidden>·</span>
        John M. Carmack
      </p>
      <h1 className="mb-8 font-serif text-4xl font-bold leading-tight text-primary-dark dark:text-primary-light">
        {post.title}
      </h1>
      <article className={ARTICLE_CLASSES}>{post.body}</article>
      <p className="mt-12">
        <LinkComponent
          href="/blog"
          className="font-medium text-primary-dark dark:text-primary-light underline underline-offset-2"
        >
          ← All posts
        </LinkComponent>
      </p>
    </BlogChrome>
  );
}
