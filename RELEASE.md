# Releasing the Noren macOS app

Single source of truth for cutting a new release. Follow the steps in order. Run everything from `/Users/onomeokajevo/noren/app` unless noted.

Branch: `develop`. Submodule pointer in the parent repo gets bumped at the end.

## 0. Pre-flight

```bash
# Confirm signing identity
security find-identity -v -p codesigning
# Expect: 1 valid identity, "Developer ID Application: Okajevo Onome (YZ64BWQC3R)"

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

Expected output: `src-tauri/target/release/bundle/macos/Noren.app` (signed) and `src-tauri/target/release/bundle/dmg/Noren_X.Y.Z_aarch64.dmg`. Tauri runs notarization automatically when the env vars are set. Watch for `Successfully signed and notarized`.

## 4. Build Intel

```bash
cargo tauri build --target x86_64-apple-darwin
```

Expected output: `src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/Noren_X.Y.Z_x64.dmg`.

If the Intel `.dmg` step is flaky, build the DMG manually from the signed `.app`:

```bash
mkdir -p src-tauri/target/x86_64-apple-darwin/release/bundle/dmg
hdiutil create -volname Noren \
  -srcfolder src-tauri/target/x86_64-apple-darwin/release/bundle/macos/Noren.app \
  -ov -format UDZO \
  src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/Noren_X.Y.Z_x64.dmg
```

## 5. Verify signing and notarization

```bash
codesign --verify --deep --strict --verbose=2 src-tauri/target/release/bundle/macos/Noren.app
spctl -a -t exec -vvv src-tauri/target/release/bundle/macos/Noren.app
# Expect: "accepted, source=Notarized Developer ID"
```

Repeat for the Intel `.app` under `src-tauri/target/x86_64-apple-darwin/release/bundle/macos/Noren.app`.

## 6. Locate updater signatures

Tauri produces an `.app.tar.gz` and a `.app.tar.gz.sig` for each target. They live alongside the bundles:

- `src-tauri/target/release/bundle/macos/Noren.app.tar.gz{,.sig}`
- `src-tauri/target/x86_64-apple-darwin/release/bundle/macos/Noren.app.tar.gz{,.sig}`

Capture both `.sig` contents for the server manifest in step 9.

## 7. Commit, tag, push

```bash
git add src-tauri/tauri.conf.json src-tauri/Cargo.toml Cargo.lock
git commit -m "Release vX.Y.Z"
git tag vX.Y.Z
git push origin develop
git push origin vX.Y.Z
```

## 8. GitHub release

Both architectures produce a file literally named `Noren.app.tar.gz` and `Noren.app.tar.gz.sig`. GitHub release assets must have unique names, so stage renamed copies first.

```bash
mkdir -p /tmp/noren-release-X.Y.Z
cp src-tauri/target/release/bundle/dmg/Noren_X.Y.Z_aarch64.dmg /tmp/noren-release-X.Y.Z/
cp src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/Noren_X.Y.Z_x64.dmg /tmp/noren-release-X.Y.Z/
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
  /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_aarch64.dmg \
  /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_x64.dmg \
  /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_aarch64.app.tar.gz \
  /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_aarch64.app.tar.gz.sig \
  /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_x64.app.tar.gz \
  /tmp/noren-release-X.Y.Z/Noren_X.Y.Z_x64.app.tar.gz.sig
```

## 9. Update the server updater manifest

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

Deploy via rsync + docker recreate (no git on prod):

```bash
rsync -avz -e "ssh -i /Users/onomeokajevo/Downloads/wilfs.pem -o StrictHostKeyChecking=no" \
  /Users/onomeokajevo/noren/server/app/updater/ \
  ec2-user@176.34.83.86:~/noren-server/app/updater/

ssh -i /Users/onomeokajevo/Downloads/wilfs.pem ec2-user@176.34.83.86 \
  "cd ~/noren-server && docker-compose build --no-cache app && docker-compose up -d --force-recreate app"
```

Verify:

```bash
# Returns 200 with the new manifest
curl -sS https://api.usenoren.ai/v1/update/darwin-aarch64/PREV.PREV.PREV | jq .

# Returns 204 (already on latest)
curl -sS -o /dev/null -w "%{http_code}\n" \
  https://api.usenoren.ai/v1/update/darwin-aarch64/X.Y.Z
```

## 10. Bump submodule pointer in the parent repo

```bash
cd /Users/onomeokajevo/noren
git add app
git commit -m "App: bump to vX.Y.Z"
git push origin develop
```

## 11. Sanity check the auto-updater

Install the previous version locally, launch it, and confirm the in-app update prompt appears within ~30s of launch and the new build installs cleanly.
