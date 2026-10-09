import { execFileSync, spawnSync } from "node:child_process"
import { selectSigningIdentity } from "./signing-identity.mjs"

let identity = process.env.APPLE_SIGNING_IDENTITY
if (!identity) {
  let identities = execFileSync("/usr/bin/security", ["find-identity", "-v", "-p", "codesigning"], { encoding: "utf8" })
  identity = selectSigningIdentity(identities)
}
if (identity === "-") {
  throw new Error("Ad-hoc signing cannot preserve local privacy permissions. Choose a code-signing certificate.")
}

let build = spawnSync("pnpm", [
  "tauri", "build", "--bundles", "app", "--config",
  JSON.stringify({ bundle: { createUpdaterArtifacts: false } }), "--", "--jobs", "4",
], { stdio: "inherit", env: { ...process.env, APPLE_SIGNING_IDENTITY: identity } })
if (build.error) throw build.error
if (build.status !== 0) process.exit(build.status ?? 1)
await import("./install-app.mjs")
