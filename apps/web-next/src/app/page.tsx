import Link from "next/link";
import { buttonClasses } from "@vegify/ui";

export default function Home() {
  return (
    <div className="mx-auto flex min-h-[70vh] max-w-3xl flex-col items-center justify-center gap-6 p-8 text-center">
      <h1 className="text-5xl font-bold text-primary-dark">Vegify</h1>
      <p className="w-full max-w-md text-lg text-gray-500">
        Micronutrition tracking for plant-based cooking — Next.js shell
      </p>
      <Link href="/recipes" className={buttonClasses({ size: "lg" })}>
        Browse recipes
      </Link>
    </div>
  );
}
