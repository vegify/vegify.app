"use client";

import { useCallback, useState } from "react";

/**
 * Per-page undo/redo for inline editing (docs/design/inline-edit.md). Every inline commit is a
 * whole-object save, so history is just a stack of prior edit states: undo re-saves the previous
 * state, redo re-saves the one undone. The hook is shell-agnostic — it's handed the same async
 * `commit(state)` the per-field edits use, so web (server-fn) and desktop (IPC) get identical
 * behavior. It lives in the detail component, so navigating away resets history (per-page, like the
 * mental model of "editing this recipe"); a rejected save leaves the stacks untouched.
 *
 * Wiring per shell (~3 lines): `record(current)` before each user edit's commit, and expose
 * `undo`/`redo`/`canUndo`/`canRedo` on the edit adapter. Terminal actions (delete) don't go through
 * `record`, so they're naturally excluded.
 */
export function useEditHistory<S>(commit: (state: S) => Promise<void>) {
  const [past, setPast] = useState<S[]>([]);
  const [future, setFuture] = useState<S[]>([]);

  // Called with the pre-edit state right before committing a user-initiated change. A fresh edit
  // clears the redo stack (you can't redo past a new branch), matching every editor's convention.
  const record = useCallback((prev: S) => {
    setPast((p) => [...p, prev]);
    setFuture([]);
  }, []);

  const undo = useCallback(
    async (current: S) => {
      if (past.length === 0) return;
      const prev = past[past.length - 1];
      setPast((p) => p.slice(0, -1));
      setFuture((f) => [...f, current]);
      await commit(prev);
    },
    [past, commit],
  );

  const redo = useCallback(
    async (current: S) => {
      if (future.length === 0) return;
      const next = future[future.length - 1];
      setFuture((f) => f.slice(0, -1));
      setPast((p) => [...p, current]);
      await commit(next);
    },
    [future, commit],
  );

  return { record, undo, redo, canUndo: past.length > 0, canRedo: future.length > 0 };
}
