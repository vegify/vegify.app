"use client"

import { useEffect } from "react"

import { anyInlineEditing } from "./inline"

/**
 * Linear-grade single-key shortcuts for an owner-editable detail page (docs/design/inline-edit.md).
 * One shared hook so web + desktop can't drift on the key map. Handlers are optional — a page
 * wires only what it has. Shortcuts suspend whenever an inline field is open (anyInlineEditing)
 * or focus is in any other input/textarea/select/contenteditable, so typing never triggers them;
 * the chrome search keeps "/" (this hook never binds it).
 *
 * Map: e = edit name · a = add ingredient · v = visibility · ? = shortcut sheet · Cmd/Ctrl+⌫ = delete.
 */
export type DetailShortcuts = {
  onEditName?: () => void
  onAddIngredient?: () => void
  onVisibility?: () => void
  onDelete?: () => void
  onHelp?: () => void
  onUndo?: () => void
  onRedo?: () => void
}

function inTypingContext(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null
  if (!el) return false
  const tag = el.tagName
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    el.isContentEditable
  )
}

export function useDetailShortcuts(shortcuts: DetailShortcuts, enabled = true) {
  useEffect(() => {
    if (!enabled) return
    const onKey = (e: KeyboardEvent) => {
      // Cmd/Ctrl+Backspace = delete — allowed to fire even from a typing context is wrong; but
      // inline fields consume Backspace themselves, so gate the same as the rest.
      if (anyInlineEditing() || inTypingContext(e.target)) return

      if ((e.metaKey || e.ctrlKey) && e.key === "Backspace") {
        if (shortcuts.onDelete) {
          e.preventDefault()
          shortcuts.onDelete()
        }
        return
      }
      // Undo / redo of committed edits. Gated the same as the rest (never while a field is open, so
      // the input's own native undo handles in-progress typing). Cmd/Ctrl+Z, Cmd/Ctrl+Shift+Z, Cmd+Y.
      if ((e.metaKey || e.ctrlKey) && (e.key === "z" || e.key === "Z")) {
        const handler = e.shiftKey ? shortcuts.onRedo : shortcuts.onUndo
        if (handler) {
          e.preventDefault()
          handler()
        }
        return
      }
      if ((e.metaKey || e.ctrlKey) && (e.key === "y" || e.key === "Y")) {
        if (shortcuts.onRedo) {
          e.preventDefault()
          shortcuts.onRedo()
        }
        return
      }
      // Bare single keys only — never hijack browser/OS combos.
      if (e.metaKey || e.ctrlKey || e.altKey) return

      switch (e.key) {
        case "e":
          if (shortcuts.onEditName) {
            e.preventDefault()
            shortcuts.onEditName()
          }
          break
        case "a":
          if (shortcuts.onAddIngredient) {
            e.preventDefault()
            shortcuts.onAddIngredient()
          }
          break
        case "v":
          if (shortcuts.onVisibility) {
            e.preventDefault()
            shortcuts.onVisibility()
          }
          break
        case "?":
          if (shortcuts.onHelp) {
            e.preventDefault()
            shortcuts.onHelp()
          }
          break
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [shortcuts, enabled])
}

/** The canonical shortcut lists for the `?` help sheet (one per detail page kind). */
export const DETAIL_SHORTCUTS: readonly { keys: string; label: string }[] = [
  { keys: "e", label: "Edit name" },
  { keys: "a", label: "Add ingredient" },
  { keys: "v", label: "Change visibility" },
  { keys: "⌘Z", label: "Undo" },
  { keys: "⌘⇧Z", label: "Redo" },
  { keys: "⌘⌫", label: "Delete recipe" },
  { keys: "?", label: "Show shortcuts" }
]

export const INGREDIENT_SHORTCUTS: readonly { keys: string; label: string }[] =
  [
    { keys: "e", label: "Edit name" },
    { keys: "v", label: "Change visibility" },
    { keys: "⌘Z", label: "Undo" },
    { keys: "⌘⇧Z", label: "Redo" },
    { keys: "⌘⌫", label: "Delete ingredient" },
    { keys: "?", label: "Show shortcuts" }
  ]
