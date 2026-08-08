import { createFileRoute } from "@tanstack/react-router"

import { LegalPage } from "../legal"

// The data-sources / attribution page (P2.2). This is a LICENCE OBLIGATION, not a courtesy page:
// Open Food Facts data is ODbL, whose terms — restated in OFF's own terms of use — require anyone
// reproducing or re-using the data to "mention the licence and to attribute the authorship to Open
// Food Facts with a link to https://openfoodfacts.org". Every branded result also carries a per-row
// attribution line in the UI; this page is where the licences themselves are named, and it is linked
// from the branded results list and the footers.
//
// USDA FoodData Central is public domain (CC0) and imposes no such condition — it is credited here
// anyway, because provenance is the product (P2.5's trust layer builds on exactly this).

export const Route = createFileRoute("/sources")({ component: SourcesPage })

function SourcesPage() {
  return (
    <LegalPage title="Data sources & licenses" updated="2026-08-07">
      <p>
        Vegify's nutrition data comes from public food databases. Every food in
        the app carries the source it came from, and this page names those
        sources and the licenses they are used under.
      </p>

      <h2>USDA FoodData Central</h2>
      <p>
        The plant-food catalog and the branded-food lookups draw on{" "}
        <a
          href="https://fdc.nal.usda.gov/"
          rel="noreferrer noopener"
          target="_blank"
        >
          USDA FoodData Central
        </a>
        , published by the U.S. Department of Agriculture, Agricultural Research
        Service. FoodData Central data are in the public domain under{" "}
        <a
          href="https://creativecommons.org/publicdomain/zero/1.0/"
          rel="noreferrer noopener"
          target="_blank"
        >
          CC0 1.0
        </a>{" "}
        — no permission or attribution is required, and USDA does not endorse
        Vegify. Foods sourced this way are marked{" "}
        <strong>USDA FoodData Central</strong> or{" "}
        <strong>USDA FoodData Central (Branded)</strong>.
      </p>

      <h2>Open Food Facts</h2>
      <p>
        Barcode lookups that USDA cannot answer fall through to{" "}
        <a
          href="https://openfoodfacts.org"
          rel="noreferrer noopener"
          target="_blank"
        >
          Open Food Facts
        </a>
        , a collaborative, free and open database of food products from around
        the world.
      </p>
      <p>
        Open Food Facts data is made available under the{" "}
        <a
          href="https://opendatacommons.org/licenses/odbl/1-0/"
          rel="noreferrer noopener"
          target="_blank"
        >
          Open Database License (ODbL) v1.0
        </a>
        . The individual contents of the database are available under the{" "}
        <a
          href="https://opendatacommons.org/licenses/dbcl/1-0/"
          rel="noreferrer noopener"
          target="_blank"
        >
          Database Contents License (DbCL) v1.0
        </a>
        , and product images are available under{" "}
        <a
          href="https://creativecommons.org/licenses/by-sa/3.0/"
          rel="noreferrer noopener"
          target="_blank"
        >
          CC BY-SA 3.0
        </a>
        . Vegify does not use Open Food Facts photos.
      </p>
      <p>
        Foods sourced this way are marked{" "}
        <strong>Open Food Facts (ODbL)</strong> in the app, permanently, so an
        ODbL-derived record stays identifiable and attributed wherever it
        appears. Each Open Food Facts product page is reachable at{" "}
        <code>https://world.openfoodfacts.org/product/&lt;barcode&gt;</code>.
      </p>

      <h2>Dietary reference intakes</h2>
      <p>
        Personalized daily targets are computed from the Dietary Reference
        Intakes published by the National Academies (Institute of Medicine) and
        the{" "}
        <a
          href="https://ods.od.nih.gov/factsheets/list-all/"
          rel="noreferrer noopener"
          target="_blank"
        >
          NIH Office of Dietary Supplements
        </a>
        , with a plant-bioavailability overlay. Every constant is cited to its
        source in the code, and every target and insight in the app shows the
        citation it rests on. Guidance, not medical advice.
      </p>

      <h2>Recipes and ingredients you create</h2>
      <p>
        Content contributed by Vegify members — recipes, ingredients, photos —
        belongs to the people who wrote it and is not part of any of the
        databases above. Branded records are cached separately from the
        community catalog so that a share-alike license can never reach your
        content.
      </p>

      <h2>Corrections</h2>
      <p>
        Food data is imperfect and label data changes. If a food is wrong, email{" "}
        <a href="mailto:hello@vegify.app">hello@vegify.app</a> — and for Open
        Food Facts products, corrections made{" "}
        <a
          href="https://world.openfoodfacts.org"
          rel="noreferrer noopener"
          target="_blank"
        >
          upstream
        </a>{" "}
        help everyone.
      </p>
    </LegalPage>
  )
}
