import type { ComponentType } from "react";
import { ImageIcon, PencilIcon, SaveIcon } from "lucide-react";
import type { AppShellLinkProps } from "./app-shell";
import { cn } from "./cn";

/**
 * The detail-page hero: a placeholder image box. When the viewer can edit — i.e. an editHref or onEdit
 * is supplied — save + edit FABs overhang the bottom-right; otherwise no FABs render, so read-only and
 * non-owner viewers see just the image. Keeps all lucide usage inside @vegify/ui so the app pages stay
 * framework-only.
 */
export function DetailHero({
  label,
  editHref,
  onEdit,
  LinkComponent,
  className,
}: {
  label: string;
  /** When set, the edit FAB links to this route. */
  editHref?: string;
  /** With editHref, routes the edit FAB through the shell's nav port (preferred — works on Tauri too). */
  LinkComponent?: ComponentType<AppShellLinkProps>;
  /** When set (and no LinkComponent), the edit FAB becomes a button calling this. Wins over a plain editHref. */
  onEdit?: () => void;
  className?: string;
}) {
  const editFabClass =
    "flex size-11 items-center justify-center rounded-full bg-primary text-primary-foreground";
  return (
    <div className={cn("relative", className)}>
      <div className="flex aspect-video w-full items-center justify-center rounded-xl bg-muted">
        <div className="flex flex-col items-center gap-2 text-muted-foreground">
          <ImageIcon className="size-10" />
          <span className="text-sm">{label}</span>
        </div>
      </div>
      {editHref || onEdit ? (
        <div className="absolute right-4 -bottom-5 flex gap-2">
          <button
            type="button"
            aria-label="Save"
            className="flex size-11 items-center justify-center rounded-full bg-card text-muted-foreground ring-1 ring-foreground/10"
          >
            <SaveIcon className="size-5" />
          </button>
          {LinkComponent && editHref ? (
            <LinkComponent href={editHref} aria-label="Edit" className={editFabClass}>
              <PencilIcon className="size-5" />
            </LinkComponent>
          ) : onEdit ? (
            <button type="button" onClick={onEdit} aria-label="Edit" className={editFabClass}>
              <PencilIcon className="size-5" />
            </button>
          ) : (
            <a href={editHref} aria-label="Edit" className={editFabClass}>
              <PencilIcon className="size-5" />
            </a>
          )}
        </div>
      ) : null}
    </div>
  );
}
