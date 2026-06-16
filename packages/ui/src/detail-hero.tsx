import { ImageIcon, PencilIcon, SaveIcon } from "lucide-react";
import { cn } from "./cn";

/**
 * The detail-page hero: a placeholder image box with save + edit FABs overhanging
 * the bottom-right (View Ingredient / View Recipe). FABs are presentational until
 * the Create/Edit flow lands. Keeps all lucide usage inside @vegify/ui so the app
 * pages stay framework-only (web-next has no direct lucide dep).
 */
export function DetailHero({
  label,
  editHref,
  className,
}: {
  label: string;
  /** When set, the edit FAB becomes a link to this route (plain <a>, framework-agnostic). */
  editHref?: string;
  className?: string;
}) {
  return (
    <div className={cn("relative", className)}>
      <div className="flex aspect-video w-full items-center justify-center rounded-xl bg-muted">
        <div className="flex flex-col items-center gap-2 text-muted-foreground">
          <ImageIcon className="size-10" />
          <span className="text-sm">{label}</span>
        </div>
      </div>
      <div className="absolute right-4 -bottom-5 flex gap-2">
        <button
          type="button"
          aria-label="Save"
          className="flex size-11 items-center justify-center rounded-full bg-card text-muted-foreground ring-1 ring-foreground/10"
        >
          <SaveIcon className="size-5" />
        </button>
        {editHref ? (
          <a
            href={editHref}
            aria-label="Edit"
            className="flex size-11 items-center justify-center rounded-full bg-primary text-primary-foreground"
          >
            <PencilIcon className="size-5" />
          </a>
        ) : (
          <button
            type="button"
            aria-label="Edit"
            className="flex size-11 items-center justify-center rounded-full bg-primary text-primary-foreground"
          >
            <PencilIcon className="size-5" />
          </button>
        )}
      </div>
    </div>
  );
}
