import type { Metadata } from "next";
import "./globals.css";
import { AppShellNext } from "./app-shell";

export const metadata: Metadata = {
  title: "Vegify",
  description: "Micronutrition tracking for plant-based cooking",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" className="h-full antialiased">
      <body className="min-h-full">
        <AppShellNext>{children}</AppShellNext>
      </body>
    </html>
  );
}
