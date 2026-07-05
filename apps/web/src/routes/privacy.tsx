import { createFileRoute } from '@tanstack/react-router'
import { LegalPage } from '../legal'

export const Route = createFileRoute('/privacy')({ component: PrivacyPage })

function PrivacyPage() {
  return (
    <LegalPage title="Privacy Policy" updated="2026-07-05">
      <p>
        This policy explains what Vegify collects and why. We collect the minimum needed to run the
        app, and we don't sell your data.
      </p>

      <h2>What we collect</h2>
      <ul>
        <li><strong>Account:</strong> your name, email, and a securely hashed password.</li>
        <li><strong>Content you create:</strong> recipes, ingredients, photos, and direct messages.</li>
        <li><strong>Usage &amp; diagnostics:</strong> basic logs (errors, request metadata) to keep the service reliable and secure.</li>
      </ul>

      <h2>How we use it</h2>
      <p>
        To operate the app: authenticate you, store and display your content, deliver messages, send
        transactional email (password reset, verification), and moderate reported content. We do not
        use your data for advertising and do not sell it.
      </p>

      <h2>Sharing</h2>
      <p>
        Public recipes and ingredients are visible to anyone by design. We use infrastructure
        providers (hosting, email delivery) that process data on our behalf under contract. We disclose
        data only when legally required.
      </p>

      <h2>Your choices</h2>
      <p>
        You can edit or delete your content anytime, and delete your account (Settings → Delete
        account), which removes your account, messages, and recipes; ingredients others depend on
        become part of the shared, anonymous catalog. To request a copy of your data, email us.
      </p>

      <h2>Contact</h2>
      <p>Privacy questions or requests: <a href="mailto:hello@vegify.app">hello@vegify.app</a>.</p>
    </LegalPage>
  )
}
