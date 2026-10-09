import assert from "node:assert/strict"
import { test } from "node:test"
import { selectSigningIdentity } from "./signing-identity.mjs"

test("selects Developer ID by hash even when certificate names repeat", () => {
  let hash = "A".repeat(40)
  let identities = `1) ${"B".repeat(40)} "Apple Development: Example (TEAM)"
2) ${hash} "Developer ID Application: Example (TEAM)"
3) ${"C".repeat(40)} "Developer ID Application: Example (TEAM)"`
  assert.equal(selectSigningIdentity(identities), hash)
})

test("requires an explicit choice when identities differ or are missing", () => {
  assert.throws(() => selectSigningIdentity("0 valid identities found"), /APPLE_SIGNING_IDENTITY/)
  assert.throws(() => selectSigningIdentity(
    `${"A".repeat(40)} "Developer ID Application: One (TEAM1)"\n${"B".repeat(40)} "Developer ID Application: Two (TEAM2)"`,
  ), /APPLE_SIGNING_IDENTITY/)
})
