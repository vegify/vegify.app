import { useQueryClient } from "@tanstack/react-query";
import { createFileRoute, redirect, useRouter } from "@tanstack/react-router";
import { SIGNUPS_ENABLED, SignupView } from "@vegify/ui/auth-form";

import { signupFn } from "../auth";
import { LinkAdapter } from "../link";

export const Route = createFileRoute("/signup")({
  // Signups are disabled (invite-only) — bounce direct URL nav to login so no one lands on a dead form.
  beforeLoad: () => {
    if (!SIGNUPS_ENABLED) throw redirect({ to: "/login" });
  },
  component: SignupPage,
});

function SignupPage() {
  const router = useRouter();
  const queryClient = useQueryClient();
  return (
    <SignupView
      LinkComponent={LinkAdapter}
      onSubmit={async ({ name, email, password }) => {
        const res = await signupFn({ data: { name, email, password } });
        if (!res.ok) return { error: res.error };
        queryClient.clear(); // start the new session with an empty content cache
        await router.invalidate();
        await router.navigate({ to: "/" });
      }}
    />
  );
}
