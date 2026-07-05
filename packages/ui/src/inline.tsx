"use client";

import * as React from "react";
import { cn } from "./cn";

/**
 * INLINE EDITING PRIMITIVES — the Linear-like edit-in-place contract (docs/design/inline-edit.md).
 *
 * Each primitive renders as static content (identical typography to the read-only view) until
 * activated by click or keyboard, then becomes its own editor in place. The commit contract is
 * shared: Enter or blur commits, Esc reverts, commits are optimistic (the static view shows the
 * new value immediately) and revert with a brief error flash if the async commit rejects.
 * A primitive with no `onCommit` renders as plain static content — the read-only render is the
 * no-adapter render, byte-identical to the pre-inline-editing screens.
 *
 * Keyboard integration: while any primitive is editing, it marks the document via
 * `data-inline-editing` so page-level single-key shortcuts (use-detail-shortcuts) suspend.
 */

/** Shared optimistic-commit state machine. */
function useInlineCommit<T>(value: T, onCommit?: (next: T) => Promise<void>) {
  const [editing, setEditing] = React.useState(false);
  // While a commit is in flight (or after a failure flash) the static view shows this instead of
  // the prop — optimistic UI with revert-on-error.
  const [optimistic, setOptimistic] = React.useState<T | null>(null);
  const [error, setError] = React.useState(false);
  const errorTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null);

  // The committed prop catching up (refetch landed) clears the optimistic overlay.
  React.useEffect(() => {
    setOptimistic(null);
  }, [value]);

  React.useEffect(() => () => {
    if (errorTimer.current) clearTimeout(errorTimer.current);
  }, []);

  const commit = React.useCallback(
    async (next: T, unchanged: boolean) => {
      setEditing(false);
      if (!onCommit || unchanged) return;
      setOptimistic(next);
      try {
        await onCommit(next);
      } catch {
        // Revert to the committed value and flash the failure (aria-live announces it).
        setOptimistic(null);
        setError(true);
        if (errorTimer.current) clearTimeout(errorTimer.current);
        errorTimer.current = setTimeout(() => setError(false), 2500);
      }
    },
    [onCommit],
  );

  return { editing, setEditing, display: optimistic ?? value, error, commit };
}

/** Marks the document while any inline editor is open (suspends page shortcuts). */
function useEditingMarker(editing: boolean) {
  React.useEffect(() => {
    if (!editing) return;
    const root = document.documentElement;
    const count = Number(root.getAttribute("data-inline-editing") ?? "0") + 1;
    root.setAttribute("data-inline-editing", String(count));
    return () => {
      const next = Number(root.getAttribute("data-inline-editing") ?? "1") - 1;
      if (next <= 0) root.removeAttribute("data-inline-editing");
      else root.setAttribute("data-inline-editing", String(next));
    };
  }, [editing]);
}

export function anyInlineEditing(): boolean {
  return document.documentElement.hasAttribute("data-inline-editing");
}

/** The shared invisible-until-hover affordance for owner-editable fields. */
const AFFORDANCE =
  "rounded-sm transition hover:bg-primary/10 focus-visible:outline-2 focus-visible:outline-primary cursor-text";
const ERROR_FLASH = "outline outline-2 outline-destructive/70";

const errorLive = (error: boolean) =>
  error ? (
    <span aria-live="polite" className="sr-only">
      Couldn't save. Your change was reverted.
    </span>
  ) : null;

// ---------------------------------------------------------------------------
// InlineText — single-line text in place (h1s, subtitles). Enter/blur commit, Esc reverts.
// ---------------------------------------------------------------------------

export function InlineText({
  value,
  onCommit,
  as = "span",
  className,
  inputClassName,
  placeholder,
  required = false,
  autoEdit = false,
  selectAllOnEdit = false,
  ariaLabel,
}: {
  value: string;
  onCommit?: (next: string) => Promise<void>;
  /** Static element to render (keeps the read-only typography). */
  as?: "h1" | "h2" | "h3" | "p" | "span";
  className?: string;
  inputClassName?: string;
  placeholder?: string;
  /** Empty input reverts instead of committing (e.g. recipe name). */
  required?: boolean;
  /** Open the editor on mount (create-blank drafts). */
  autoEdit?: boolean;
  /** Select the whole value when the editor opens (drafts: typing replaces "Untitled recipe"). */
  selectAllOnEdit?: boolean;
  ariaLabel?: string;
}) {
  const { editing, setEditing, display, error, commit } = useInlineCommit(value, onCommit);
  const inputRef = React.useRef<HTMLInputElement>(null);
  const openedAuto = React.useRef(false);
  useEditingMarker(editing);

  React.useEffect(() => {
    if (autoEdit && onCommit && !openedAuto.current) {
      openedAuto.current = true;
      setEditing(true);
    }
  }, [autoEdit, onCommit, setEditing]);

  React.useEffect(() => {
    if (!editing) return;
    const input = inputRef.current;
    if (!input) return;
    input.focus();
    if (selectAllOnEdit) input.select();
  }, [editing, selectAllOnEdit]);

  const Tag = as;
  if (!onCommit) {
    return <Tag className={className}>{display || placeholder}</Tag>;
  }

  if (editing) {
    return (
      <input
        ref={inputRef}
        defaultValue={display}
        placeholder={placeholder}
        aria-label={ariaLabel}
        // The input inherits the static element's typography so nothing shifts.
        className={cn(
          "w-full min-w-0 border-none bg-transparent p-0 outline-none focus:ring-0",
          className,
          inputClassName,
        )}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            const next = e.currentTarget.value.trim();
            if (required && !next) return setEditing(false);
            void commit(next, next === value);
          } else if (e.key === "Escape") {
            e.preventDefault();
            setEditing(false);
          }
        }}
        onBlur={(e) => {
          const next = e.currentTarget.value.trim();
          if (required && !next) return setEditing(false);
          void commit(next, next === value);
        }}
      />
    );
  }

  return (
    <Tag
      role="button"
      tabIndex={0}
      aria-label={ariaLabel ? `Edit ${ariaLabel}` : "Edit"}
      data-inline-field={ariaLabel}
      className={cn(className, AFFORDANCE, error && ERROR_FLASH, !display && "text-muted-foreground")}
      onClick={() => setEditing(true)}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          setEditing(true);
        }
      }}
    >
      {display || placeholder}
      {errorLive(error)}
    </Tag>
  );
}

// ---------------------------------------------------------------------------
// InlineTextarea — multi-line in place (directions). Cmd/Ctrl+Enter or blur commit, Esc reverts.
// ---------------------------------------------------------------------------

export function InlineTextarea({
  value,
  onCommit,
  className,
  placeholder,
  ariaLabel,
}: {
  value: string;
  onCommit?: (next: string) => Promise<void>;
  className?: string;
  placeholder?: string;
  ariaLabel?: string;
}) {
  const { editing, setEditing, display, error, commit } = useInlineCommit(value, onCommit);
  const ref = React.useRef<HTMLTextAreaElement>(null);
  useEditingMarker(editing);

  const autoGrow = (el: HTMLTextAreaElement) => {
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  };

  React.useEffect(() => {
    if (!editing) return;
    const el = ref.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
    autoGrow(el);
  }, [editing]);

  if (!onCommit) {
    return <p className={className}>{display || placeholder}</p>;
  }

  if (editing) {
    return (
      <textarea
        ref={ref}
        defaultValue={display}
        placeholder={placeholder}
        aria-label={ariaLabel}
        rows={1}
        className={cn(
          "w-full resize-none border-none bg-transparent p-0 outline-none focus:ring-0",
          className,
        )}
        onInput={(e) => autoGrow(e.currentTarget)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            const next = e.currentTarget.value.trim();
            void commit(next, next === value);
          } else if (e.key === "Escape") {
            e.preventDefault();
            setEditing(false);
          }
        }}
        onBlur={(e) => {
          const next = e.currentTarget.value.trim();
          void commit(next, next === value);
        }}
      />
    );
  }

  return (
    <p
      role="button"
      tabIndex={0}
      aria-label={ariaLabel ? `Edit ${ariaLabel}` : "Edit"}
      data-inline-field={ariaLabel}
      className={cn(className, AFFORDANCE, error && ERROR_FLASH, !display && "text-muted-foreground")}
      onClick={() => setEditing(true)}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          setEditing(true);
        }
      }}
    >
      {display || placeholder}
      {errorLive(error)}
    </p>
  );
}

// ---------------------------------------------------------------------------
// InlineNumber — the amount chip. Click → digits pre-selected; type-over; ↑/↓ smart steps
// (Shift ×10); Enter/blur commit; Esc reverts; Tab commits AND hops to the next chip in the
// same group (retune a recipe: click · type · Tab · type · Tab …).
// ---------------------------------------------------------------------------

/** Smart step size for grams-scale numbers: 1 below 20, 5 to 100, 25 above. */
function stepFor(n: number): number {
  if (n < 20) return 1;
  if (n < 100) return 5;
  return 25;
}

/** Trim float noise: 163.60000000000001 → "163.6". */
const fmt = (n: number) => String(Number(n.toFixed(3)));

export function InlineNumber({
  value,
  onCommit,
  onPreview,
  suffix,
  group,
  min = 0,
  className,
  ariaLabel,
}: {
  value: number;
  onCommit?: (next: number) => Promise<void>;
  /** Fires on EVERY intermediate value — each keystroke, each scrub step — WITHOUT persisting, so a
   *  consumer (e.g. the nutrition panel) can update live as you type or drag. `onCommit` still fires
   *  once at the end. A `null` argument means "editing ended, drop the preview" (revert to committed). */
  onPreview?: (next: number | null) => void;
  /** Unit label rendered after the number ("g"). */
  suffix?: string;
  /** Tab-chain group: Tab commits and opens the next chip with the same group name. */
  group?: string;
  min?: number;
  className?: string;
  ariaLabel?: string;
}) {
  const { editing, setEditing, display, error, commit } = useInlineCommit(value, onCommit);
  const inputRef = React.useRef<HTMLInputElement>(null);
  const hostRef = React.useRef<HTMLButtonElement>(null);
  useEditingMarker(editing);
  // preview() is fire-and-forget live feedback; the committed prop landing clears it (null).
  const preview = onPreview ?? (() => {});

  React.useEffect(() => {
    if (!editing) return;
    const input = inputRef.current;
    if (!input) return;
    input.focus();
    input.select(); // digits pre-selected: typing replaces
  }, [editing]);

  const parse = (raw: string): number | null => {
    const n = Number(raw.replace(",", "."));
    return Number.isFinite(n) && n >= min ? n : null;
  };

  const hopToNext = () => {
    if (!group) return;
    const chips = Array.from(
      document.querySelectorAll<HTMLButtonElement>(`[data-inline-group="${group}"]`),
    );
    const i = chips.indexOf(hostRef.current!);
    const next = chips[i + 1];
    if (next) next.click();
  };

  if (!onCommit) {
    return (
      <span className={className}>
        {fmt(display)}
        {suffix ? ` ${suffix}` : null}
      </span>
    );
  }

  if (editing) {
    return (
      <span className={cn("inline-flex items-baseline gap-1", className)}>
        <input
          ref={inputRef}
          defaultValue={fmt(display)}
          inputMode="decimal"
          aria-label={ariaLabel}
          size={Math.max(fmt(display).length, 3)}
          className="w-[5ch] border-none bg-transparent p-0 text-right outline-none focus:ring-0 tabular-nums"
          onInput={(e) => {
            // LIVE: every keystroke feeds the preview (the nutrition panel updates as you type).
            const n = parse(e.currentTarget.value);
            if (n != null) preview(n);
          }}
          onKeyDown={(e) => {
            const input = e.currentTarget;
            if (e.key === "Enter") {
              e.preventDefault();
              const n = parse(input.value);
              if (n == null) return setEditing(false);
              void commit(n, n === value);
            } else if (e.key === "Escape") {
              e.preventDefault();
              preview(null); // drop the live preview, revert to committed
              setEditing(false);
            } else if (e.key === "Tab") {
              e.preventDefault();
              const n = parse(input.value);
              if (n != null) void commit(n, n === value);
              else setEditing(false);
              hopToNext();
            } else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
              e.preventDefault();
              const cur = parse(input.value) ?? value;
              const base = stepFor(cur) * (e.shiftKey ? 10 : 1);
              const next = Math.max(min, cur + (e.key === "ArrowUp" ? base : -base));
              input.value = fmt(next);
              input.select();
              preview(next); // live on ↑/↓ too
            }
          }}
          onBlur={(e) => {
            const n = parse(e.currentTarget.value);
            if (n == null) return setEditing(false);
            void commit(n, n === value);
          }}
        />
        {suffix ? <span className="text-muted-foreground">{suffix}</span> : null}
      </span>
    );
  }

  return (
    <ScrubButton
      hostRef={hostRef}
      group={group}
      ariaLabel={ariaLabel}
      className={cn(className, error && ERROR_FLASH)}
      display={display}
      suffix={suffix}
      min={min}
      base={value}
      error={error}
      onScrub={preview}
      onScrubCommit={(n) => void commit(n, n === value)}
      onOpen={() => setEditing(true)}
    />
  );
}

/** The closed-state number: a SCRUBBABLE handle (Figma/Blender-style). Hover shows the ↔ resize
 *  cursor. Press-drag horizontally changes the value by smart steps (right = up, left = down),
 *  previewing live and committing on release. A press WITHOUT a drag (< 4px) is a click → opens the
 *  text input with digits pre-selected. Keyboard: Enter/Space opens the input (full a11y kept). */
function ScrubButton({
  hostRef,
  group,
  ariaLabel,
  className,
  display,
  suffix,
  min,
  base,
  error,
  onScrub,
  onScrubCommit,
  onOpen,
}: {
  hostRef: React.RefObject<HTMLButtonElement | null>;
  group?: string;
  ariaLabel?: string;
  className?: string;
  display: number;
  suffix?: string;
  min: number;
  base: number;
  error: boolean;
  onScrub: (next: number | null) => void;
  onScrubCommit: (next: number) => void;
  onOpen: () => void;
}) {
  // Drag bookkeeping in a ref so the pointer handlers don't re-render mid-scrub.
  const drag = React.useRef<{ startX: number; startVal: number; last: number; moved: boolean } | null>(null);
  const [scrubbing, setScrubbing] = React.useState(false);

  // ~6 horizontal px per step, stepped smart to the value's scale — so a small number nudges by 1s
  // and a large one by 25s, matching the keyboard ↑/↓ feel. Sub-min is clamped.
  const valueAt = (dx: number, from: number) => {
    const steps = Math.round(dx / 6);
    return Math.max(min, from + steps * stepFor(from));
  };

  return (
    <button
      ref={hostRef}
      type="button"
      data-inline-group={group}
      data-inline-field={ariaLabel}
      aria-label={ariaLabel ? `Edit ${ariaLabel} (drag to adjust, click to type)` : "Edit amount"}
      style={{ cursor: "ew-resize", touchAction: "none" }}
      className={cn(
        className,
        "select-none rounded-sm px-0.5 -mx-0.5 tabular-nums transition hover:bg-primary/10 focus-visible:outline-2 focus-visible:outline-primary",
        scrubbing && "bg-primary/15",
      )}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { startX: e.clientX, startVal: base, last: base, moved: false };
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d) return;
        const dx = e.clientX - d.startX;
        if (!d.moved && Math.abs(dx) < 4) return; // dead zone: distinguishes click from drag
        d.moved = true;
        if (!scrubbing) setScrubbing(true);
        const next = valueAt(dx, d.startVal);
        if (next !== d.last) {
          d.last = next;
          onScrub(next); // LIVE preview as you drag
        }
      }}
      onPointerUp={(e) => {
        const d = drag.current;
        drag.current = null;
        if (e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.releasePointerCapture(e.pointerId);
        if (!d) return;
        if (d.moved) {
          setScrubbing(false);
          onScrubCommit(d.last); // persist the scrubbed value
        } else {
          onOpen(); // a click, not a drag → type mode
        }
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpen();
        }
      }}
    >
      {fmt(display)}
      {suffix ? ` ${suffix}` : null}
      {errorLive(error)}
    </button>
  );
}

// ---------------------------------------------------------------------------
// InlinePillSelect — the visibility pill: a small popover select in Linear's status-pill idiom.
// Rendered with a native <select> overlay for reliability (Base UI Select is available for the
// richer form controls; the pill wants zero layout shift and full keyboard support for free).
// ---------------------------------------------------------------------------

export function InlinePillSelect<T extends string>({
  value,
  options,
  onCommit,
  className,
  ariaLabel,
}: {
  value: T;
  options: readonly { value: T; label: string }[];
  onCommit?: (next: T) => Promise<void>;
  className?: string;
  ariaLabel?: string;
}) {
  const { display, error, commit } = useInlineCommit(value, onCommit);
  const label = options.find((o) => o.value === display)?.label ?? display;

  if (!onCommit) return null; // visibility is owner-only metadata; readers never see it

  return (
    <span
      className={cn(
        "relative inline-flex items-center rounded-full border border-border px-2.5 py-0.5 text-xs font-semibold text-muted-foreground transition hover:border-primary hover:text-primary-dark dark:hover:text-primary-light",
        error && ERROR_FLASH,
        className,
      )}
    >
      {label}
      <select
        value={display}
        aria-label={ariaLabel}
        className="absolute inset-0 cursor-pointer opacity-0"
        onChange={(e) => {
          const next = e.currentTarget.value as T;
          void commit(next, next === value);
        }}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      {errorLive(error)}
    </span>
  );
}
