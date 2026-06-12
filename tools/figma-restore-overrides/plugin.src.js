// Restores symbol-instance text overrides dropped by Figma's .sketch import.
// Data is inlined at build time from design/figma-import/text-overrides.json (see build.py).
//
// Safety model: a node is only written if its current text equals the recorded master
// default (exactly, or after trimming) — i.e. only nodes still showing import-damaged
// text are touched. Nodes already showing the target value count as already-correct.
// Everything else (missing, ambiguous, hand-edited) is reported, never written.
// Idempotent: safe to run repeatedly.

const DATA = /*__DATA__*/;

async function run() {
  const stats = {
    restored: 0,
    alreadyCorrect: 0,
    pageMissing: 0,
    containerMissing: 0,
    instanceMissing: 0,
    noMatchingNode: 0,
    ambiguous: 0,
    errors: 0,
  };
  const problems = [];

  for (const rec of DATA) {
    const targets = (rec.textOverrides || []).filter((o) => o.value !== o.default);
    if (!targets.length) continue;
    const where = `${rec.page} » ${rec.container} » ${rec.instance}`;

    const page = figma.root.children.find((p) => p.name === rec.page);
    if (!page) {
      stats.pageMissing += targets.length;
      problems.push(`page missing (ok if pruned): ${rec.page}`);
      continue;
    }

    let scope = page;
    if (rec.container && rec.container !== "(page root)") {
      const cname = rec.container.replace(/^Symbol master: /, "");
      const c = page.children.find((n) => n.name === cname);
      if (!c) {
        stats.containerMissing += targets.length;
        problems.push(`container missing (ok if pruned): ${rec.page} » ${cname}`);
        continue;
      }
      scope = c;
    }

    let instances =
      scope.type === "INSTANCE" || scope.type === "COMPONENT"
        ? [scope]
        : scope.findAll((n) => n.type === "INSTANCE" && n.name === rec.instance);
    if (rec.master && instances.length > 1) {
      const byMaster = instances.filter((i) => i.mainComponent && i.mainComponent.name === rec.master);
      if (byMaster.length) instances = byMaster;
    }
    if (!instances.length) {
      stats.instanceMissing += targets.length;
      problems.push(`instance missing: ${where}`);
      continue;
    }

    for (const t of targets) {
      const leafName = t.layerPath.split(" > ").pop();
      const cands = [];
      for (const inst of instances) {
        for (const n of inst.findAll((n) => n.type === "TEXT" && n.name === leafName)) cands.push(n);
      }
      const label = `${where} » ${leafName}`;

      if (cands.some((n) => n.characters === t.value)) {
        stats.alreadyCorrect++;
        continue;
      }
      let pick = cands.filter((n) => n.characters === t.default);
      if (!pick.length && t.default != null) {
        pick = cands.filter((n) => n.characters.trim() === t.default.trim());
      }
      if (!pick.length) {
        stats.noMatchingNode++;
        problems.push(`no node showing the expected default (${cands.length} candidates, left untouched): ${label}`);
        continue;
      }
      if (pick.length > 1) {
        stats.ambiguous++;
        problems.push(`ambiguous — ${pick.length} nodes match the default, none written: ${label}`);
        continue;
      }
      try {
        const node = pick[0];
        const fonts = node.characters.length
          ? node.getRangeAllFontNames(0, node.characters.length)
          : [node.fontName];
        for (const f of fonts) await figma.loadFontAsync(f);
        node.characters = t.value;
        stats.restored++;
      } catch (e) {
        stats.errors++;
        problems.push(`error writing ${label}: ${e}`);
      }
    }
  }

  console.log("=== Vegify override restore ===");
  console.log(stats);
  for (const p of problems) console.log("  - " + p);
  const attention =
    stats.noMatchingNode + stats.ambiguous + stats.instanceMissing +
    stats.containerMissing + stats.pageMissing + stats.errors;
  figma.notify(
    `Restored ${stats.restored} · already correct ${stats.alreadyCorrect} · needs attention ${attention} (console has details)`,
    { timeout: 12000 }
  );
  figma.closePlugin();
}

run();
