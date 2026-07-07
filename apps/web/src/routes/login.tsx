import { useQueryClient } from "@tanstack/react-query";
import { createFileRoute, useRouter } from "@tanstack/react-router";
import { LoginView } from "@vegify/ui/auth-form";

import { loginFn } from "../auth";
import { LinkAdapter } from "../link";

export const Route = createFileRoute("/login")({
  component: LoginPage,
});

function LoginPage() {
  const router = useRouter();
  const queryClient = useQueryClient();
  return (
    <LoginView
      LinkComponent={LinkAdapter}
      onSubmit={async ({ email, password }) => {
        const res = await loginFn({ data: { email, password } });
        if (!res.ok) return { error: res.error };
        queryClient.clear(); // start the new session with an empty content cache
        await router.invalidate();
        await router.navigate({ to: "/" });
      }}
    />
  );
}
