// Ensures src-tauri/embedded.provisionprofile exists so tauri's `bundle.macOS.files` copy succeeds.
// CI writes the REAL Developer ID profile here (decoded from the VEGIFY_PROVISION_PROFILE_B64 secret)
// BEFORE the bundle step, so this no-ops in CI. It only fills a placeholder for local/unsigned builds,
// where macOS ignores the embedded profile anyway — the entitlement is only validated for signed apps.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const dir = path.dirname(fileURLToPath(import.meta.url));
const target = path.join(dir, '..', 'src-tauri', 'embedded.provisionprofile');

if (fs.existsSync(target) && fs.statSync(target).size > 0) {
  console.log(`embedded.provisionprofile present (${fs.statSync(target).size} bytes) — leaving it as-is`);
} else {
  fs.writeFileSync(
    target,
    'PLACEHOLDER — the real Developer ID provisioning profile is injected in CI from the ' +
      'VEGIFY_PROVISION_PROFILE_B64 secret. Local unsigned builds ignore this file.\n',
  );
  console.log('wrote placeholder embedded.provisionprofile (local build — unsigned apps ignore it)');
}
