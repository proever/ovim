export let selectSigningIdentity = (identities) => {
  let matches = [...identities.matchAll(/([A-F0-9]{40}) "(Developer ID Application: [^"]+)"/g)]
  let names = [...new Set(matches.map((match) => match[2]))]
  if (names.length !== 1) {
    throw new Error("Set APPLE_SIGNING_IDENTITY to the certificate to use for every local build. Expected one Developer ID identity.")
  }
  // Duplicate certificates can share a name; codesign needs an unambiguous hash.
  return matches[0][1]
}
