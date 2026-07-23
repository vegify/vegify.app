import { useCallback, useEffect, useState } from "react"

/**
 * Weight unit preference — metric (kg) vs imperial (lb). A pure DISPLAY choice: body weight is always
 * stored canonically in KILOGRAMS on the nutrition profile (the protein g/kg target reads kg), and this
 * only chooses how the weight field is shown and entered. Persisted client-side in localStorage, the
 * same model as `use-theme` — no backend/wire churn for a display toggle, and it works in both the
 * browser and the Tauri webview. Per-device (SSR renders kg, then reconciles after hydration).
 */
export type WeightUnit = "kg" | "lb"

const KEY = "vegify-weight-unit"

/** Pounds per kilogram (exact-enough for a display conversion; canonical storage stays kg). */
export const LB_PER_KG = 2.2046226218

/** Convert a canonical-kg value into the display unit. */
export function kgToDisplay(kg: number, unit: WeightUnit): number {
  return unit === "lb" ? kg * LB_PER_KG : kg
}

/** Convert a display-unit value back to canonical kg. */
export function displayToKg(value: number, unit: WeightUnit): number {
  return unit === "lb" ? value / LB_PER_KG : value
}

export function useWeightUnit() {
  const [unit, setUnitState] = useState<WeightUnit>("kg")

  // Hydrate from storage on mount (server has no localStorage).
  useEffect(() => {
    const stored = localStorage.getItem(KEY)
    if (stored === "kg" || stored === "lb") setUnitState(stored)
  }, [])

  const setUnit = useCallback((u: WeightUnit) => {
    localStorage.setItem(KEY, u)
    setUnitState(u)
  }, [])

  return { unit, setUnit }
}
