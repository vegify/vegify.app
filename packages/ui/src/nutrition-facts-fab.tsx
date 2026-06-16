"use client";

import { FileTextIcon } from "lucide-react";
import { cn } from "./cn";
import { Dialog, DialogContent, DialogTitle, DialogTrigger } from "./dialog";
import { NutritionFacts, type NutritionFactsData } from "./nutrition-facts";

/**
 * Mobile-only: an orange FAB that opens the Nutrition Facts panel in a modal
 * (the desktop layout shows the same panel as a persistent rail instead).
 */
export function NutritionFactsFab({
  data,
  className,
}: {
  data: NutritionFactsData;
  className?: string;
}) {
  return (
    <Dialog>
      <DialogTrigger
        aria-label="Nutrition facts"
        className={cn(
          "fixed right-4 bottom-20 z-30 flex size-14 items-center justify-center rounded-full bg-orange text-white shadow-lg transition hover:brightness-95 lg:hidden",
          className,
        )}
      >
        <FileTextIcon className="size-6" />
      </DialogTrigger>
      <DialogContent className="max-h-[85vh] overflow-y-auto">
        <DialogTitle className="sr-only">Nutrition Facts</DialogTitle>
        <NutritionFacts data={data} />
      </DialogContent>
    </Dialog>
  );
}
