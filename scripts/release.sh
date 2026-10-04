#!/usr/bin/env bash
#
# Builds Pixieflow for macOS (one universal app for Intel + Apple Silicon)
# and publishes it as a GitHub release, together with the `latest.json`
# manifest the app's auto-updater checks on launch.
#
# Usage:
#   scripts/release.sh <version> ["release notes"]   # e.g. scripts/release.sh 0.2.0 "Faster conversion"
#   scripts/release.sh --dry-run                     # build + manifest only, nothing is committed or published
#
# Needs: the updater signing key at ~/.tauri/pixieflow.key (or set
# TAURI_SIGNING_PRIVATE_KEY_PATH), and `gh` logged in to GitHub.

set -euo pipefail

REPO="robertontiu/pixieflow"
TARGET="universal-apple-darwin"
KEY="${TAURI_SIGNING_PRIVATE_KEY_PATH:-$HOME/.tauri/pixieflow.key}"

die() { echo "Error: $*" >&2; exit 1; }
step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

cd "$(dirname "$0")/.."

DRY_RUN=0
if [ "${1:-}" = "--dry-run" ]; then
    DRY_RUN=1
    VERSION="$(node -p 'require("./package.json").version')"
    NOTES=""
else
    [ $# -ge 1 ] || die "usage: $0 <version> [\"release notes\"]   (or --dry-run)"
    VERSION="${1#v}"
    NOTES="${2:-}"
fi
TAG="v$VERSION"

# ---- Preflight ----
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "version must look like 1.2.3 (got '$VERSION')"
[ -f "$KEY" ] || die "updater signing key not found at $KEY"
if [ "$DRY_RUN" = 0 ]; then
    [ -z "$(git status --porcelain)" ] || die "commit or stash your changes first"
    [ "$(git branch --show-current)" = "main" ] || die "releases are made from main"
    gh auth status >/dev/null 2>&1 || die "run 'gh auth login' first"
    git fetch --tags --quiet origin
    ! git rev-parse -q --verify "refs/tags/$TAG" >/dev/null || die "$TAG already exists"
fi

# ---- Version bump ----
if [ "$DRY_RUN" = 0 ]; then
    step "Setting version to $VERSION"
    for f in package.json src-tauri/tauri.conf.json; do
        node -e '
            const fs = require("fs"), [f, v] = process.argv.slice(1);
            const json = JSON.parse(fs.readFileSync(f, "utf8"));
            json.version = v;
            fs.writeFileSync(f, JSON.stringify(json, null, 2) + "\n");
        ' "$f" "$VERSION"
    done
    # Only the first `version =` line, which is the [package] one.
    perl -0pi -e "s/^version = \"[^\"]*\"/version = \"$VERSION\"/m" src-tauri/Cargo.toml
    npm install --package-lock-only --silent
fi

# ---- Build ----
step "Building Pixieflow $VERSION ($TARGET)"
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
npm ci --silent
export TAURI_SIGNING_PRIVATE_KEY="$KEY"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"
npx tauri build --target "$TARGET"

BUNDLE="src-tauri/target/$TARGET/release/bundle"
OUT="src-tauri/target/release-assets/$TAG"
rm -rf "$OUT" && mkdir -p "$OUT"

DMG_NAME="Pixieflow_${VERSION}_universal.dmg"
TARBALL_NAME="Pixieflow_${VERSION}_universal.app.tar.gz"
cp "$BUNDLE"/dmg/*.dmg "$OUT/$DMG_NAME"
cp "$BUNDLE/macos/Pixieflow.app.tar.gz" "$OUT/$TARBALL_NAME"
SIGNATURE="$(cat "$BUNDLE/macos/Pixieflow.app.tar.gz.sig")"

# ---- Updater manifest ----
# The app fetches .../releases/latest/download/latest.json, so every release
# carries its own copy. Both architectures point at the same universal build.
step "Writing latest.json"
URL="https://github.com/$REPO/releases/download/$TAG/$TARBALL_NAME"
node -e '
    const [version, notes, url, signature, out] = process.argv.slice(1);
    const platform = { signature, url };
    const manifest = {
        version,
        notes,
        pub_date: new Date().toISOString(),
        platforms: { "darwin-aarch64": platform, "darwin-x86_64": platform },
    };
    require("fs").writeFileSync(out, JSON.stringify(manifest, null, 2) + "\n");
' "$VERSION" "$NOTES" "$URL" "$SIGNATURE" "$OUT/latest.json"

if [ "$DRY_RUN" = 1 ]; then
    step "Dry run finished — nothing was committed or published"
    ls -lh "$OUT"
    exit 0
fi

# ---- Publish ----
step "Committing and tagging $TAG"
git commit --quiet -am "Release $TAG"
git tag -a "$TAG" -m "Pixieflow $VERSION"
git push --quiet origin main "$TAG"

step "Creating GitHub release"
notes_args=(--generate-notes)
[ -n "$NOTES" ] && notes_args=(--notes "$NOTES")
gh release create "$TAG" \
    --repo "$REPO" \
    --title "Pixieflow $VERSION" \
    "${notes_args[@]}" \
    "$OUT/$DMG_NAME" "$OUT/$TARBALL_NAME" "$OUT/latest.json"

step "Released $TAG"
echo "https://github.com/$REPO/releases/tag/$TAG"
