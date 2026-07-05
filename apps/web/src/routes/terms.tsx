import { createFileRoute } from '@tanstack/react-router'
import { LegalPage } from '../legal'

export const Route = createFileRoute('/terms')({ component: TermsPage })

function TermsPage() {
  return (
    <LegalPage title="Terms of Service" updated="2026-07-05">
      <p>
        Welcome to Vegify. By creating an account or using the app you agree to these terms. Vegify is
        a community for sharing plant-based recipes and the micronutrition behind them.
      </p>

      <h2>Your content</h2>
      <p>
        You own what you post. By posting a recipe, ingredient, photo, or message you grant Vegify a
        licence to store and display it so the app works — for example, showing a public recipe to
        other people, or an ingredient you create to anyone who cooks with it. Recipes and ingredients
        are public by default; you can set a recipe to unlisted or private. You can delete your content
        at any time, and you can delete your account (Settings → Delete account), which removes your
        account, messages, and recipes.
      </p>

      <h2>Community rules — zero tolerance for objectionable content and abuse</h2>
      <p>
        Vegify has <strong>no tolerance for objectionable content or abusive behaviour.</strong> You
        may not post content that is unlawful, hateful, harassing, sexually explicit, violent, or that
        infringes others' rights, and you may not harass, threaten, impersonate, or abuse other people
        — including in direct messages.
      </p>
      <p>
        Every piece of content and every profile can be <strong>reported</strong> (the “Report”
        control on recipes, ingredients, and profiles), and you can <strong>block</strong> any user to
        stop them from messaging you and remove them from your view. We <strong>review reports and act
        within 24 hours</strong>, removing violating content and ejecting abusive users. Accounts that
        break these rules may be suspended or deleted without notice.
      </p>

      <h2>No warranty</h2>
      <p>
        Nutrition data (including USDA FoodData Central and community-entered values) is provided for
        general information only and is not medical or dietary advice. Vegify is provided “as is”
        without warranties, and we aren't liable for how you use the information here.
      </p>

      <h2>Contact</h2>
      <p>
        Questions, reports, or legal notices: <a href="mailto:hello@vegify.app">hello@vegify.app</a>.
      </p>
    </LegalPage>
  )
}
