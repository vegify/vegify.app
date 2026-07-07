"use client";

import {
  type ComponentProps,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from "react";
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
export type AuthSubmitResult = { error?: string } | undefined;

/**
 * Signups are disabled (invite-only). The server is the authority (`POST /api/auth/signup` → 403);
 * this gates the UI entry points so neither shell advertises a dead path. To re-open: set this to
 * `true` AND set `VEGIFY_SIGNUPS_OPEN=1` on the server. Typed `boolean` (not the `false` literal) so
 * the always-false guards below don't read as constant-condition dead code.
 */
export const SIGNUPS_ENABLED: boolean = false;

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
      <Input id={id} name={id} className="h-11" {...props} />
    </div>
  );
}

export function LoginView({
  onSubmit,
  LinkComponent,
}: {
  onSubmit: (values: {
    email: string;
    password: string;
  }) => Promise<AuthSubmitResult>;
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
        <h1 className="text-center font-serif text-3xl font-bold text-primary-dark">
          Welcome back
        </h1>
        {error ? (
          <p
            role="alert"
            className="rounded-lg bg-destructive/10 px-3 py-2 text-sm text-destructive"
          >
            {error}
          </p>
        ) : null}
        <LabeledInput
          id="email"
          label="Email or username"
          type="text"
          autoComplete="username"
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
          <LinkComponent
            href="/forgot"
            className="font-semibold text-primary hover:underline"
          >
            Forgot your password?
          </LinkComponent>
        </p>
        {SIGNUPS_ENABLED ? (
          <p className="text-center text-sm text-muted-foreground">
            Don&apos;t have an account?{" "}
            <LinkComponent
              href="/signup"
              className="font-semibold text-primary hover:underline"
            >
              Sign up
            </LinkComponent>
          </p>
        ) : null}
      </form>
    </AuthLayout>
  );
}

export function SignupView({
  onSubmit,
  LinkComponent,
}: {
  onSubmit: (values: {
    name: string;
    email: string;
    password: string;
  }) => Promise<AuthSubmitResult>;
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
          <p
            role="alert"
            className="rounded-lg bg-destructive/10 px-3 py-2 text-sm text-destructive"
          >
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
          <LinkComponent
            href="/login"
            className="font-semibold text-primary hover:underline"
          >
            Sign in
          </LinkComponent>
        </p>
      </form>
    </AuthLayout>
  );
}

/**
 * Request a password-reset link. Enumeration-safe: on a successful submit it always shows the same
 * "check your email" confirmation, never revealing whether the address has an account (the backend
 * always 200s too). `onSubmit` returns an error only for a transport/validation failure.
 */
export function ForgotPasswordView({
  onSubmit,
  LinkComponent,
}: {
  onSubmit: (values: { email: string }) => Promise<AuthSubmitResult>;
  LinkComponent: NavLink;
}) {
  const [email, setEmail] = useState("");
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  async function handleSubmit() {
    if (pending) return;
    setPending(true);
    setError(null);
    try {
      const res = await onSubmit({ email });
      if (res?.error) setError(res.error);
      else setSent(true);
    } catch {
      setError("Something went wrong. Please try again.");
    } finally {
      setPending(false);
    }
  }

  return (
    <AuthLayout>
      <div className="w-full space-y-4 rounded-2xl bg-card p-6 ring-1 ring-foreground/10">
        <h1 className="text-center font-serif text-3xl font-bold text-primary-dark">
          Reset password
        </h1>
        {sent ? (
          <>
            <p className="text-center text-sm text-muted-foreground">
              If an account exists for{" "}
              <span className="font-medium text-foreground">{email}</span>,
              we&apos;ve sent a link to reset your password. Check your inbox.
            </p>
            <LinkComponent
              href="/login"
              className={cn(buttonClasses({ size: "lg" }), "w-full")}
            >
              Back to sign in
            </LinkComponent>
          </>
        ) : (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void handleSubmit();
            }}
            className="space-y-4"
          >
            <p className="text-center text-sm text-muted-foreground">
              Enter your email and we&apos;ll send you a link to reset your
              password.
            </p>
            {error ? (
              <p
                role="alert"
                className="rounded-lg bg-destructive/10 px-3 py-2 text-sm text-destructive"
              >
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
            <button
              type="submit"
              disabled={pending}
              className={cn(buttonClasses({ size: "lg" }), "w-full")}
            >
              {pending ? "Sending…" : "Send reset link"}
            </button>
            <p className="text-center text-sm text-muted-foreground">
              <LinkComponent
                href="/login"
                className="font-semibold text-primary hover:underline"
              >
                Back to sign in
              </LinkComponent>
            </p>
          </form>
        )}
      </div>
    </AuthLayout>
  );
}

/**
 * Choose a new password from a reset link. `token` comes from the link's `?token=`; an absent token
 * shows a re-request prompt. On success it surfaces a sign-in CTA. The backend enforces token validity
 * + the 8-char minimum; the client mirrors the minimum and the confirm-match for instant feedback.
 */
export function ResetPasswordView({
  token,
  onSubmit,
  LinkComponent,
}: {
  token: string;
  onSubmit: (values: {
    token: string;
    password: string;
  }) => Promise<AuthSubmitResult>;
  LinkComponent: NavLink;
}) {
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  async function handleSubmit() {
    if (pending) return;
    setError(null);
    if (password.length < 8) {
      setError("Password must be at least 8 characters.");
      return;
    }
    if (password !== confirm) {
      setError("Passwords don't match.");
      return;
    }
    setPending(true);
    try {
      const res = await onSubmit({ token, password });
      if (res?.error) setError(res.error);
      else setDone(true);
    } catch {
      setError("Something went wrong. Please try again.");
    } finally {
      setPending(false);
    }
  }

  return (
    <AuthLayout>
      <div className="w-full space-y-4 rounded-2xl bg-card p-6 ring-1 ring-foreground/10">
        <h1 className="text-center font-serif text-3xl font-bold text-primary-dark">
          Choose a new password
        </h1>
        {!token ? (
          <p className="text-center text-sm text-muted-foreground">
            This reset link is missing its token. Request a new one from{" "}
            <LinkComponent
              href="/forgot"
              className="font-semibold text-primary hover:underline"
            >
              Reset password
            </LinkComponent>
            .
          </p>
        ) : done ? (
          <>
            <p className="text-center text-sm text-muted-foreground">
              Your password has been reset. You can now sign in with your new
              password.
            </p>
            <LinkComponent
              href="/login"
              className={cn(buttonClasses({ size: "lg" }), "w-full")}
            >
              Sign in
            </LinkComponent>
          </>
        ) : (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void handleSubmit();
            }}
            className="space-y-4"
          >
            {error ? (
              <p
                role="alert"
                className="rounded-lg bg-destructive/10 px-3 py-2 text-sm text-destructive"
              >
                {error}
              </p>
            ) : null}
            <LabeledInput
              id="password"
              label="New password"
              type="password"
              autoComplete="new-password"
              required
              minLength={8}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
            <LabeledInput
              id="confirm"
              label="Confirm new password"
              type="password"
              autoComplete="new-password"
              required
              minLength={8}
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
            />
            <button
              type="submit"
              disabled={pending}
              className={cn(buttonClasses({ size: "lg" }), "w-full")}
            >
              {pending ? "Resetting…" : "Reset password"}
            </button>
          </form>
        )}
      </div>
    </AuthLayout>
  );
}

/**
 * Confirm an email address from a verification link. `token` comes from the link's `?token=`; the view
 * submits it once on mount (a verification link is a single click, not a form) and shows the outcome.
 * The backend stamps the account verified and burns the token; an absent/expired/used token shows a
 * re-request prompt. Rendered by both shells.
 */
export function VerifyEmailView({
  token,
  onSubmit,
  LinkComponent,
}: {
  token: string;
  onSubmit: (values: { token: string }) => Promise<AuthSubmitResult>;
  LinkComponent: NavLink;
}) {
  const [status, setStatus] = useState<"verifying" | "done" | "error">(
    token ? "verifying" : "error",
  );
  const [error, setError] = useState<string | null>(
    token ? null : "This verification link is missing its token.",
  );
  const ran = useRef(false);

  useEffect(() => {
    if (!token || ran.current) return;
    ran.current = true; // the link is single-use — submit exactly once
    void (async () => {
      try {
        const res = await onSubmit({ token });
        if (res?.error) {
          setError(res.error);
          setStatus("error");
        } else {
          setStatus("done");
        }
      } catch {
        setError("Something went wrong. Please try again.");
        setStatus("error");
      }
    })();
  }, [token, onSubmit]);

  return (
    <AuthLayout>
      <div className="w-full space-y-4 rounded-2xl bg-card p-6 ring-1 ring-foreground/10">
        <h1 className="text-center font-serif text-3xl font-bold text-primary-dark">
          {status === "done" ? "Email confirmed" : "Confirm your email"}
        </h1>
        {status === "verifying" ? (
          <p className="text-center text-sm text-muted-foreground">
            Confirming your email address…
          </p>
        ) : status === "done" ? (
          <>
            <p className="text-center text-sm text-muted-foreground">
              Your email address is verified — you&apos;re all set.
            </p>
            <LinkComponent
              href="/"
              className={cn(buttonClasses({ size: "lg" }), "w-full")}
            >
              Continue to Vegify
            </LinkComponent>
          </>
        ) : (
          <>
            <p
              role="alert"
              className="rounded-lg bg-destructive/10 px-3 py-2 text-sm text-destructive"
            >
              {error}
            </p>
            <p className="text-center text-sm text-muted-foreground">
              The link may have expired or already been used. Sign in and we can
              send you a fresh one.
            </p>
            <LinkComponent
              href="/login"
              className={cn(buttonClasses({ size: "lg" }), "w-full")}
            >
              Go to sign in
            </LinkComponent>
          </>
        )}
      </div>
    </AuthLayout>
  );
}

/**
 * A slim banner shown in the app chrome when the signed-in user hasn't verified their email yet.
 * `onResend` re-requests the verification email (enumeration-safe on the backend). Rendered by both
 * shells above the page content — styled to sit inside the app, not the centered AuthLayout.
 */
export function EmailVerificationBanner({
  email,
  onResend,
}: {
  email: string;
  onResend: () => Promise<AuthSubmitResult>;
}) {
  const [state, setState] = useState<"idle" | "sending" | "sent">("idle");

  async function handleResend() {
    if (state === "sending") return;
    setState("sending");
    try {
      await onResend();
    } finally {
      setState("sent");
    }
  }

  return (
    <div className="flex flex-wrap items-center justify-center gap-x-2 gap-y-1 border-b border-primary/20 bg-primary/5 px-4 py-2 text-center text-sm text-foreground">
      {state === "sent" ? (
        <span>
          Verification link sent to <span className="font-medium">{email}</span>
          . Check your inbox.
        </span>
      ) : (
        <>
          <span>
            Please verify your email{" "}
            <span className="font-medium">({email})</span> to secure your
            account.
          </span>
          <button
            type="button"
            onClick={() => void handleResend()}
            disabled={state === "sending"}
            className="font-semibold text-primary hover:underline disabled:opacity-60"
          >
            {state === "sending" ? "Sending…" : "Resend email"}
          </button>
        </>
      )}
    </div>
  );
}
