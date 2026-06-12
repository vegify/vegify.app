// Audits & restores symbol-instance text-override content after Figma's .sketch import.
// Data is inlined at build time from design/figma-import/text-overrides.json (see build.py).
//
// What actually happens on import (learned empirically):
//   1. Text imports with Sketch PostScript family tokens ("AvenirNext") that don't match real
//      family names ("Avenir Next") — the text exists but renders invisible until Figma's
//      missing-font replacement is run across the whole file. DO THAT FIRST.
//   2. Custom instance layer names ("field Copy 3") are renamed to the component's name, and
//      some scaled instances are detached to frames/groups — so matching here is by component
//      name OR layer name, disambiguated by the instance's recorded canvas position.
//
// Audit (read-only) classifies every expected override: ok / showsDefault / unknown / notFound.
// Restore writes ONLY nodes currently showing the recorded master default. Idempotent.

const DATA = /*__DATA__*/;

const TYPES = ["INSTANCE", "FRAME", "GROUP", "COMPONENT"];
const collapse = (s) => (s == null ? "" : String(s)).replace(/\s+/g, " ").trim();

function candidatesIn(scope, rec) {
  if (scope.type === "INSTANCE" || scope.type === "COMPONENT") return [scope];
  const out = [];
  const seen = new Set();
  for (const n of scope.findAll((n) => TYPES.includes(n.type))) {
    let hit = n.name === rec.instance || (rec.master && n.name === rec.master);
    if (!hit && rec.master && n.type === "INSTANCE" && n.mainComponent) {
      const mc = n.mainComponent;
      hit = mc.name === rec.master ||
        (mc.parent && mc.parent.type === "COMPONENT_SET" && mc.parent.name === rec.master);
    }
    if (hit && !seen.has(n.id)) {
      seen.add(n.id);
      out.push(n);
    }
  }
  // Disambiguate same-component siblings (e.g. six form fields) by canvas position.
  if (out.length > 1 && typeof rec.x === "number") {
    const scored = out
      .map((n) => {
        const bb = n.absoluteBoundingBox;
        return { d: bb ? Math.max(Math.abs(bb.x - rec.x), Math.abs(bb.y - rec.y)) : Infinity, n };
      })
      .sort((a, b) => a.d - b.d);
    if (scored[0].d <= 30) {
      const cut = Math.max(scored[0].d + 2, 4);
      return scored.filter((s) => s.d <= cut).map((s) => s.n);
    }
  }
  return out;
}

async function run(write) {
  const stats = {
    ok: 0, restored: 0, showsDefault: 0, unknown: 0, notFound: 0,
    ambiguous: 0, pageMissing: 0, containerMissing: 0, errors: 0,
  };
  const problems = [];

  for (const rec of DATA) {
    const targets = (rec.textOverrides || []).filter((o) => o.value !== o.default);
    if (!targets.length) continue;
    const where = `${rec.page} » ${rec.container} » ${rec.instance}`;

    const page = figma.root.children.find((p) => p.name === rec.page);
    if (!page) {
      stats.pageMissing += targets.length;
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

    const containers = candidatesIn(scope, rec);
    if (!containers.length) {
      stats.notFound += targets.length;
      problems.push(`not found by name or component: ${where}`);
      continue;
    }

    for (const t of targets) {
      const leafName = t.layerPath.split(" > ").pop();
      let cands = [];
      for (const c of containers) {
        for (const n of c.findAll((x) => x.type === "TEXT" && x.name === leafName)) cands.push(n);
      }
      if (!cands.length) {
        for (const c of containers) {
          for (const n of c.findAll((x) => x.type === "TEXT")) cands.push(n);
        }
      }
      const label = `${where} » ${leafName}`;
      if (!cands.length) {
        stats.notFound++;
        problems.push(`no text nodes found: ${label}`);
        continue;
      }
      if (cands.some((n) => collapse(n.characters) === collapse(t.value))) {
        stats.ok++;
        continue;
      }
      let pick = cands.filter((n) => n.characters === t.default);
      if (!pick.length && t.default != null) {
        pick = cands.filter((n) => collapse(n.characters) === collapse(t.default));
      }
      if (!pick.length) {
        stats.unknown++;
        const cur = cands.slice(0, 2).map((n) => JSON.stringify(collapse(n.characters).slice(0, 50))).join(" | ");
        problems.push(
          `neither target nor default: ${label} — current ${cur}; ` +
          `default ${JSON.stringify(collapse(t.default).slice(0, 50))}; target ${JSON.stringify(collapse(t.value).slice(0, 50))}`
        );
        continue;
      }
      if (!write) {
        stats.showsDefault++;
        continue;
      }
      if (pick.length > 1) {
        stats.ambiguous++;
        problems.push(`ambiguous (${pick.length} default-matching nodes, none written): ${label}`);
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
        problems.push(`error (font not replaced yet?): ${label}: ${e}`);
      }
    }
  }
  return { stats, problems };
}

if (figma.editorType === "dev") {
  figma.notify("Dev Mode is read-only for plugins — switch to design mode (Shift+D) and run again.", { timeout: 8000 });
  figma.closePlugin();
} else {
  figma.showUI(
    `<style>
       body{font:12px -apple-system,sans-serif;margin:12px;color:#eee;background:#2c2c2c}
       button{margin:0 8px 8px 0;padding:6px 12px;border-radius:6px;border:0;cursor:pointer}
       #a{background:#4c9638;color:#fff} #f{background:#fbb040}
       pre{white-space:pre-wrap;font-size:11px;max-height:190px;overflow:auto;background:#1e1e1e;padding:8px;border-radius:6px}
     </style>
     <div style="margin-bottom:8px"><b>Sketch text overrides</b></div>
     <button id="a">Audit (read-only)</button>
     <button id="f">Restore (writes)</button>
     <pre id="out">Run the missing-font replacement (all pages) first, then Audit.
Restore only writes nodes still showing the master default.</pre>
     <script>
       document.getElementById('a').onclick = () => parent.postMessage({pluginMessage:{cmd:'run',write:false}}, '*');
       document.getElementById('f').onclick = () => parent.postMessage({pluginMessage:{cmd:'run',write:true}}, '*');
       onmessage = (e) => { const m = e.data.pluginMessage; if (m && m.cmd === 'result') document.getElementById('out').textContent = m.text; };
     </script>`,
    { width: 400, height: 320 }
  );
  figma.ui.onmessage = async (msg) => {
    if (!msg || msg.cmd !== "run") return;
    const { stats, problems } = await run(msg.write);
    console.log(`=== Vegify override ${msg.write ? "restore" : "audit"} ===`);
    console.log(stats);
    for (const p of problems) console.log("  - " + p);
    const text =
      JSON.stringify(stats, null, 1) + "\n\n" +
      problems.slice(0, 40).join("\n") +
      (problems.length > 40 ? `\n…and ${problems.length - 40} more (see console)` : "");
    figma.ui.postMessage({ cmd: "result", text });
    figma.notify(`${msg.write ? "Restore" : "Audit"} done — details in panel/console`, { timeout: 6000 });
  };
}
