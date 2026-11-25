#!/usr/bin/env bash
# Signs a release that CI built, on this computer, where the signing key is.
#
#   scripts/sign-release.sh v0.2.0 [rollout percent] [--publish]
#
# Downloads the draft release's package descriptions, writes a signed manifest.json
# offered to that percentage of installs (100 if left out), and uploads it. Run it
# again with a higher percentage to widen a rollout. --publish makes the draft public
# on the repository's releases page. See docs/releasing.md.
set -euo pipefail

tag=${1:?usage: scripts/sign-release.sh <tag> [rollout percent] [--publish]}
rollout=100
publish=no
for arg in "${@:2}"; do
  case $arg in
    --publish) publish=yes ;;
    *) rollout=$arg ;;
  esac
done
key=${STARDUST_RELEASE_KEY:-$HOME/.stardust-keys/release.key}
[ -f "$key" ] || { echo "No signing key at $key"; exit 1; }

cd "$(dirname "$0")/.."
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

gh release download "$tag" --pattern '*.package.json' --dir "$work"
cargo xtask sign --key "$key" --dir "$work" --rollout "$rollout"
gh release upload "$tag" "$work/manifest.json" "$work/manifest.json.sig" --clobber
echo "Uploaded the signed manifest to $tag at $rollout% rollout."

if [ "$publish" = yes ]; then
  gh release edit "$tag" --draft=false
  echo "Published $tag."
fi
