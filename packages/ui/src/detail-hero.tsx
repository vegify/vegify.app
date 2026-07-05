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
  photoUrl,
  onUploadPhoto,
  editHref,
  onEdit,
  LinkComponent,
  className,
}: {
  label: string;
  /** The hero photo; absent = the labeled placeholder. */
  photoUrl?: string | null;
  /** Owner affordance: pick a file → the shell uploads + attaches it (camera FAB appears). */
  onUploadPhoto?: (file: File) => void | Promise<void>;
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
      {photoUrl ? (
        <img src={photoUrl} alt={label} className="aspect-video w-full rounded-xl object-cover" />
      ) : (
        <div className="flex aspect-video w-full items-center justify-center rounded-xl bg-muted">
          <div className="flex flex-col items-center gap-2 text-muted-foreground">
            <ImageIcon className="size-10" />
            <span className="text-sm">{label}</span>
          </div>
        </div>
      )}
      {onUploadPhoto ? (
        <label
          aria-label="Upload photo"
          className="absolute left-4 -bottom-5 flex size-11 cursor-pointer items-center justify-center rounded-full bg-card text-muted-foreground ring-1 ring-foreground/10 transition-colors hover:text-foreground"
        >
          <ImageIcon className="size-5" />
          <input
            type="file"
            accept="image/jpeg,image/png,image/webp"
            className="sr-only"
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) void onUploadPhoto(f);
              e.target.value = ""; // allow re-picking the same file
            }}
          />
        </label>
      ) : null}
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
