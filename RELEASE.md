# Releasing the Noren macOS app

Single source of truth for cutting a new release. Follow the steps in order. Run everything from `/Users/onomeokajevo/noren/app` unless noted.

Branch: `develop`. Submodule pointer in the parent repo gets bumped at the end.

## 0. Pre-flight

```bash
# Confirm signing identity
security find-identity -v -p codesigning
# Expect:
# - "Developer ID Application: Okajevo Onome (YZ64BWQC3R)" for .app signing
# - "Developer ID Installer: Okajevo Onome (YZ64BWQC3R)" for public .pkg installers

# Confirm notarytool credentials reach Apple
xcrun notarytool history --keychain-profile noren-notary | head -5

# Confirm sidecars exist
ls src-tauri/binaries/noren-keychain-host-{aarch64,x86_64}-apple-darwin
```

If any of these are missing, see `~/.claude/projects/-Users-onomeokajevo-noren/memory/reference_app-release.md` for the recovery path. To rebuild sidecars:

```bash
cargo build --manifest-path src-tauri/Cargo.toml --bin noren-keychain-host --target aarch64-apple-darwin --release
cp src-tauri/target/aarch64-apple-darwin/release/noren-keychain-host src-tauri/binaries/noren-keychain-host-aarch64-apple-darwin

rustup target add x86_64-apple-darwin
cargo build --manifest-path src-tauri/Cargo.toml --bin noren-keychain-host --target x86_64-apple-darwin --release
cp src-tauri/target/x86_64-apple-darwin/release/noren-keychain-host src-tauri/binaries/noren-keychain-host-x86_64-apple-darwin
```

## 1. Bump version

Update both files together. The frontend `package.json` is independent and is not touched here.

- `src-tauri/tauri.conf.json` -> `"version": "X.Y.Z"`
- `src-tauri/Cargo.toml` -> `version = "X.Y.Z"`

Then refresh the lockfile:

```bash
cargo update -p noren --manifest-path src-tauri/Cargo.toml
```

## 2. Load credentials

```bash
source ~/.noren-release.env
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/noren-update.key)"
```

The env file holds `APPLE_SIGNING_IDENTITY`, `APPLE_TEAM_ID`, `APPLE_ID`, `APPLE_PASSWORD`, and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Verify they are set without printing values:

```bash
for v in APPLE_SIGNING_IDENTITY APPLE_TEAM_ID APPLE_ID APPLE_PASSWORD TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD; do
  if [ -z "${(P)v}" ]; then echo "MISSING: $v"; else echo "ok: $v"; fi
done
```

## 3. Build Apple Silicon

```bash
cargo tauri build
```

Expected output: `src-tauri/target/release/bundle/macos/Noren.app` (signed), plus updater artifacts under the macOS bundle directory. Tauri may also produce a DMG, but the public website installer is the `.pkg` built in step 6.

## 4. Build Intel

```bash
cargo tauri build --target x86_64-apple-darwin
```

Expected output: `src-tauri/target/x86_64-apple-darwin/release/bundle/macos/Noren.app` (signed), plus updater artifacts under the macOS bundle directory. Tauri may also produce a DMG, but do not use it as the public website installer.

## 5. Verify signing and notarization

```bash
codesign --verify --deep --strict --verbose=2 src-tauri/target/release/bundle/macos/Noren.app
spctl -a -t exec -vvv src-tauri/target/release/bundle/macos/Noren.app
# Expect: "accepted, source=Notarized Developer ID"
```

Repeat for the Intel `.app` under `src-tauri/target/x86_64-apple-darwin/release/bundle/macos/Noren.app`.

## 6. Build public installer packages

The public website download should be a signed, notarized, stapled `.pkg` installer. The `.pkg` installs `Noren.app` into `/Applications` and gives users the normal macOS Installer flow.

Do not point the website at the updater `.app.tar.gz` files. Those are only for Tauri auto-update.

```bash
mkdir -p /tmp/noren-release-X.Y.Z
mkdir -p /tmp/noren-pkg-root-X.Y.Z/{aarch64,x64}

ditto src-tauri/target/release/bundle/macos/Noren.app \
  /tmp/noren-pkg-root-X.Y.Z/aarch64/Noren.app
ditto src-tauri/target/x86_64-apple-darwin/release/bundle/macos/Noren.app \
  /tmp/noren-pkg-root-X.Y.Z/x64/Noren.app

pkgbuild --analyze --root /tmp/noren-pkg-root-X.Y.Z/aarch64 \
  /tmp/noren-release-X.Y.Z/components-aarch64.plist
pkgbuild --analyze --root /tmp/noren-pkg-root-X.Y.Z/x64 \
  /tmp/noren-release-X.Y.Z/components-x64.plist

# Critical: prevent macOS Installer from relocating the app to an old copy in Downloads/Desktop.
plutil -replace 0.BundleIsRelocatable -bool NO /tmp/noren-release-X.Y.Z/components-aarch64.plist
plutil -replace 0.BundleIsRelocatable -bool NO /tmp/noren-release-X.Y.Z/components-x64.plist

pkgbuild \
  --root /tmp/noren-pkg-root-X.Y.Z/aarch64 \
  --install-location /Applications \
  --identifier ink.noren.desktop \
  --version X.Y.Z \
  --component-plist /tmp/noren-release-X.Y.Z/components-aarch64.plist \
  --sign "Developer ID Installer: Okajevo Onome (YZ64BWQC3R)" \
  /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_aarch64.pkg

pkgbuild \
  --root /tmp/noren-pkg-root-X.Y.Z/x64 \
  --install-location /Applications \
  --identifier ink.noren.desktop \
  --version X.Y.Z \
  --component-plist /tmp/noren-release-X.Y.Z/components-x64.plist \
  --sign "Developer ID Installer: Okajevo Onome (YZ64BWQC3R)" \
  /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_x64.pkg
```

Notarize, staple, and verify both installer packages:

```bash
source ~/.noren-release.env

xcrun notarytool submit /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_aarch64.pkg \
  --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
xcrun notarytool submit /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_x64.pkg \
  --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait

xcrun stapler staple /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_aarch64.pkg
xcrun stapler staple /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_x64.pkg

pkgutil --check-signature /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_aarch64.pkg
pkgutil --check-signature /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_x64.pkg
spctl -a -vv -t install /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_aarch64.pkg
spctl -a -vv -t install /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_x64.pkg
# Expect: accepted, source=Notarized Developer ID
```

For the `v1.0.1` installer rebuild, the public package hashes were:

- `Noren_1.0.1_aarch64.pkg` -> `0f692e9e583b8ae20a20e003fbb9649972da3c148c7c77d7f9759132dd8f80c8`
- `Noren_1.0.1_x64.pkg` -> `c216640b9352cfebc9deb4106012bd1ef6ef152c1d0d60095de73243f9fa5cf3`

The fixed `v1.0.1` packages have `install-location="/Applications"`, `relocatable="false"`, and an empty `<relocate/>` block in `PackageInfo`.

## 7. Locate updater signatures

Tauri produces an `.app.tar.gz` and a `.app.tar.gz.sig` for each target. They live alongside the bundles:

- `src-tauri/target/release/bundle/macos/Noren.app.tar.gz{,.sig}`
- `src-tauri/target/x86_64-apple-darwin/release/bundle/macos/Noren.app.tar.gz{,.sig}`

Capture both `.sig` contents for the server manifest in step 11.

## 8. Commit, tag, push

```bash
git add src-tauri/tauri.conf.json src-tauri/Cargo.toml Cargo.lock
git commit -m "Release vX.Y.Z"
git tag vX.Y.Z
git push origin develop
git push origin vX.Y.Z
```

## 9. GitHub release

Both architectures produce a file literally named `Noren.app.tar.gz` and `Noren.app.tar.gz.sig`. GitHub release assets must have unique names, so stage renamed copies first. Upload both public `.pkg` installers and both updater `.app.tar.gz` artifacts.

```bash
mkdir -p /tmp/noren-release-X.Y.Z
cp src-tauri/target/release/bundle/macos/Noren.app.tar.gz \
   /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_aarch64.app.tar.gz
cp src-tauri/target/release/bundle/macos/Noren.app.tar.gz.sig \
   /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_aarch64.app.tar.gz.sig
cp src-tauri/target/x86_64-apple-darwin/release/bundle/macos/Noren.app.tar.gz \
   /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_x64.app.tar.gz
cp src-tauri/target/x86_64-apple-darwin/release/bundle/macos/Noren.app.tar.gz.sig \
   /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_x64.app.tar.gz.sig

gh release create vX.Y.Z \
  --title "vX.Y.Z" \
  --notes "Release notes here" \
  /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_aarch64.pkg \
  /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_x64.pkg \
  /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_aarch64.app.tar.gz \
  /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_aarch64.app.tar.gz.sig \
  /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_x64.app.tar.gz \
  /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_x64.app.tar.gz.sig
```

## 10. Update the website download links

The website should point users to the `.pkg` installers:

- `website/lib/download-config.ts`
  - `DOWNLOAD_URLS.macOS.arm64` -> `Noren_X.Y.Z_aarch64.pkg`
  - `DOWNLOAD_URLS.macOS.x64` -> `Noren_X.Y.Z_x64.pkg`

The download page format label should say `.pkg installer`, not `.dmg`.

## 11. Update the server updater manifest

The desktop auto-updater hits `https://api.usenoren.ai/v1/update/{target}/{current_version}`. Without this step, clients on the previous version are never prompted, no matter what was uploaded to GitHub.

The manifest source-of-truth is `server/app/updater/routes.py`. Edit the four constants at the top:

- `LATEST_VERSION` -> `"X.Y.Z"`
- `LATEST_PUB_DATE` -> ISO 8601 timestamp of the GitHub release
- `LATEST_NOTES` -> short user-facing release notes (one sentence is fine)
- For each platform in `PLATFORMS`, replace the `signature` block with the contents of the matching `.sig` file from the build:

```bash
cat src-tauri/target/release/bundle/macos/Noren.app.tar.gz.sig                  # aarch64
cat src-tauri/target/x86_64-apple-darwin/release/bundle/macos/Noren.app.tar.gz.sig  # x86_64
```

The `url` fields key off `LATEST_VERSION`, so they self-update once the version constant is bumped (assuming the GitHub asset names follow `Noren_X.Y.Z_<arch>.app.tar.gz`).

Commit and push the server change:

```bash
cd /Users/onomeokajevo/noren/server
git add app/updater/routes.py
git commit -m "Bump updater manifest to vX.Y.Z"
git push origin main
```

Then deploy the server so the updater endpoint serves the new manifest.

This repo is public, so the deploy procedure lives with the private server docs
rather than here. Follow the "Deploy" section of `docs/SERVER-OPS.md` in the
`noren` repo, which covers the current host, credentials and build commands.

Verify:

```bash
# Returns 200 with the new manifest
curl -sS https://api.usenoren.ai/v1/update/darwin-aarch64/PREV.PREV.PREV | jq .

# Returns 204 (already on latest)
curl -sS -o /dev/null -w "%{http_code}\n" \
  https://api.usenoren.ai/v1/update/darwin-aarch64/X.Y.Z
```

## 12. Bump submodule pointer in the parent repo

```bash
cd /Users/onomeokajevo/noren
git add app website server
git commit -m "Ship vX.Y.Z app release"
git push origin develop
```

## 13. Sanity check installers and auto-updater

Re-download the public `.pkg` installers from GitHub and verify Gatekeeper acceptance:

```bash
curl -L -o /tmp/Noren_X.Y.Z_aarch64.pkg \
  https://github.com/Usenoren/noren-app/releases/download/vX.Y.Z/Noren_X.Y.Z_aarch64.pkg
curl -L -o /tmp/Noren_X.Y.Z_x64.pkg \
  https://github.com/Usenoren/noren-app/releases/download/vX.Y.Z/Noren_X.Y.Z_x64.pkg

spctl -a -vv -t install /tmp/Noren_X.Y.Z_aarch64.pkg
spctl -a -vv -t install /tmp/Noren_X.Y.Z_x64.pkg

pkgutil --expand-full /tmp/Noren_X.Y.Z_aarch64.pkg /tmp/Noren_X.Y.Z_aarch64-expanded
grep -E 'install-location|relocatable|relocate' /tmp/Noren_X.Y.Z_aarch64-expanded/PackageInfo
# Expect install-location="/Applications", relocatable="false", and <relocate/>
```

Install the previous version locally, launch it, and confirm the in-app update prompt appears within ~30s of launch and the new build installs cleanly.
