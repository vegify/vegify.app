"use client";

import { useState, type ComponentProps, type ReactNode } from "react";
import { buttonClasses } from "./button";
import { cn } from "./cn";
import { Input } from "./input";
import type { NavLink } from "./screens";
import { VegifyLogo } from "./vegify-logo";

/**
 * Shared auth screens — rendered by BOTH shells. Purely presentational: they own only ephemeral
 * form state and delegate the actual sign-in/up to an injected `onSubmit` (web → server fn, desktop
 * → IPC over HTTPS in A2), and navigation to the `LinkComponent` port. Sibling to recipe-form /
 * ingredient-form (the interactive-form pattern), keeping screens.tsx stateless.
 */
export type AuthSubmitResult = { error?: string } | void;

function AuthLayout({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-background px-4 py-12 text-foreground">
      <div className="flex w-full max-w-sm flex-col items-center">
        <div className="mb-8 w-44 text-green-dark">
          <VegifyLogo className="h-auto w-full" />
        </div>
        {children}
      </div>
    </div>
  );
}

function LabeledInput({
  id,
  label,
  ...props
}: { id: string; label: string } & ComponentProps<typeof Input>) {
  return (
    <div className="space-y-1.5">
      <label htmlFor={id} className="text-sm font-medium">
        {label}
      </label>
      <Input id={id} className="h-11" {...props} />
    </div>
  );
}

export function LoginView({
  onSubmit,
  LinkComponent,
}: {
  onSubmit: (values: { email: string; password: string }) => Promise<AuthSubmitResult>;
  LinkComponent: NavLink;
}) {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  async function handleSubmit() {
    if (pending) return;
    setPending(true);
    setError(null);
    try {
      const res = await onSubmit({ email, password });
      if (res?.error) setError(res.error);
    } catch {
      setError("Something went wrong. Please try again.");
    } finally {
      setPending(false);
    }
  }

  return (
    <AuthLayout>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void handleSubmit();
        }}
        className="w-full space-y-4 rounded-2xl bg-card p-6 ring-1 ring-foreground/10"
      >
        <h1 className="text-center font-serif text-3xl font-bold text-primary-dark">Welcome back</h1>
        {error ? (
          <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </p>
        ) : null}
        <LabeledInput
          id="email"
          label="Email"
          type="email"
          autoComplete="email"
          required
          value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
        <LabeledInput
          id="password"
          label="Password"
          type="password"
          autoComplete="current-password"
          required
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        <button
          type="submit"
          disabled={pending}
          className={cn(buttonClasses({ size: "lg" }), "w-full")}
        >
          {pending ? "Signing in…" : "Sign in"}
        </button>
        <p className="text-center text-sm text-muted-foreground">
          Don&apos;t have an account?{" "}
          <LinkComponent href="/signup" className="font-semibold text-primary hover:underline">
            Sign up
          </LinkComponent>
        </p>
      </form>
    </AuthLayout>
  );
}

export function SignupView({
  onSubmit,
  LinkComponent,
}: {
  onSubmit: (values: { name: string; email: string; password: string }) => Promise<AuthSubmitResult>;
  LinkComponent: NavLink;
}) {
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  async function handleSubmit() {
    if (pending) return;
    setPending(true);
    setError(null);
    try {
      const res = await onSubmit({ name, email, password });
      if (res?.error) setError(res.error);
    } catch {
      setError("Something went wrong. Please try again.");
    } finally {
      setPending(false);
    }
  }

  return (
    <AuthLayout>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void handleSubmit();
        }}
        className="w-full space-y-4 rounded-2xl bg-card p-6 ring-1 ring-foreground/10"
      >
        <h1 className="text-center font-serif text-3xl font-bold text-primary-dark">
          Create your account
        </h1>
        {error ? (
          <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </p>
        ) : null}
        <LabeledInput
          id="name"
          label="Name"
          type="text"
          autoComplete="name"
          required
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <LabeledInput
          id="email"
          label="Email"
          type="email"
          autoComplete="email"
          required
          value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
        <LabeledInput
          id="password"
          label="Password"
          type="password"
          autoComplete="new-password"
          required
          minLength={8}
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        <button
          type="submit"
          disabled={pending}
          className={cn(buttonClasses({ size: "lg" }), "w-full")}
        >
          {pending ? "Creating account…" : "Sign up"}
        </button>
        <p className="text-center text-sm text-muted-foreground">
          Already have an account?{" "}
          <LinkComponent href="/login" className="font-semibold text-primary hover:underline">
            Sign in
          </LinkComponent>
        </p>
      </form>
    </AuthLayout>
  );
}
