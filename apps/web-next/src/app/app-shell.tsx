"use client";

import type { ReactNode } from "react";
import NextLink from "next/link";
import { usePathname } from "next/navigation";
import { AppShell, type AppShellLinkProps } from "@vegify/ui";

function LinkAdapter({ href, ...props }: AppShellLinkProps) {
  return <NextLink href={href} {...props} />;
}

/** Client wrapper: feeds the shared shell the Next router's pathname + Link. */
export function AppShellNext({ children }: { children: ReactNode }) {
  const pathname = usePathname();
  return (
    <AppShell currentPath={pathname ?? "/"} LinkComponent={LinkAdapter}>
      {children}
    </AppShell>
  );
}
