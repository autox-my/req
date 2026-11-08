#!/usr/bin/env bash
REV='<placeholder: the rev of the release you pin, e.g. abc1234 -- fill in after the first release>'
SHA256_x86_64_unknown_linux_gnu='<placeholder: 64 hex digits from the release asset req-REV-x86_64-unknown-linux-gnu.tar.gz.sha256>'
SHA256_aarch64_apple_darwin='<placeholder: 64 hex digits from the release asset req-REV-aarch64-apple-darwin.tar.gz.sha256>'
# >>> not copied into the CI step (this header and the two test-only overrides) >>>
# examples/install-req.sh -- install a prebuilt, checksummed req (ADR 0025).
#
# The three lines above are the pin table, the ONLY thing you edit: the rev you depend on and the
# SHA-256 of each target's tarball. Take the values from the `.sha256` assets of the release
# `rev-<REV>` the first time (trust on first use of the maintainer's release); from then on the
# pin in YOUR repository is the authority, and the release's own checksum is never read. A target
# you do not install on may keep its placeholder: only the detected target's pin is used.
#
# What it does, in order, and each failure is a non-zero exit that names its cause (never a skip,
# never a retry by another path): detect the target from `uname -sm` (Linux x86_64, Darwin arm64);
# download <base>/rev-<REV>/req-<REV>-<TARGET>.tar.gz over https (a public release needs no credential:
# the Authorization header is sent ONLY when REQ_RELEASE_TOKEN is set and non-empty, for a private fork or
# mirror, and then as a header read from stdin, never as an argument, never printed); verify the tarball
# against the pin; check the archive lists exactly one member, `req`; extract it; run
# `req --version` and require `built from` to equal REV exactly; only then install it, mode 0755,
# into ${REQ_INSTALL_DIR:-$HOME/.local/bin}. Needs: bash, curl, tar, sha256sum or shasum.
#
# Overrides, read here and nowhere else, and never set by any ADOPTER-facing YAML (the tests set them):
#   REQ_RELEASE_BASE  replaces the release host (https://github.com/autox-my/req/releases/download)
#   REQ_CURL_PROTO    replaces curl's --proto value (default =https)
install_base=${REQ_RELEASE_BASE:-}
install_proto=${REQ_CURL_PROTO:-}
# <<< end of the part not copied into the CI step <<<
set -eu

# Every refusal goes through here: one line naming the cause, and the rule that there is no fallback.
fail() {
  printf 'install-req: %s\n' "$*" >&2
  printf 'install-req: refused. There is no automatic fallback to a source install (docs/adopting.md).\n' >&2
  exit 1
}

# The target comes first: a platform that cannot be installed cannot be fixed by a pin.
platform=$(uname -sm)
case $platform in
  'Linux x86_64')
    target=x86_64-unknown-linux-gnu
    pin=${SHA256_x86_64_unknown_linux_gnu:-}
    ;;
  'Darwin arm64')
    target=aarch64-apple-darwin
    pin=${SHA256_aarch64_apple_darwin:-}
    ;;
  *)
    fail "unsupported platform: $platform (supported: Linux x86_64, Darwin arm64)"
    ;;
esac

# A placeholder, a typo or an empty pin is a refusal before any request is made.
case $REV in
  '' | *[!0-9a-f]*) fail "the pin table has no usable REV (a bare lower-case hex SHA of 7 to 40 digits); fill it in from the release" ;;
esac
if [ "${#REV}" -lt 7 ] || [ "${#REV}" -gt 40 ]; then
  fail "the pin table has no usable REV (a bare lower-case hex SHA of 7 to 40 digits); fill it in from the release"
fi
if [ -z "$pin" ]; then
  fail "no pin for target $target: fill in its SHA-256 in the pin table, or install on a supported machine"
fi
case $pin in
  *[!0-9a-f]*) fail "the pin table's SHA-256 for $target is not 64 lower-case hex digits; fill it in from the release's .sha256 asset" ;;
esac
if [ "${#pin}" -ne 64 ]; then
  fail "the pin table's SHA-256 for $target is not 64 lower-case hex digits; fill it in from the release's .sha256 asset"
fi

for tool in curl tar; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    fail "$tool is required and was not found on PATH"
  fi
done
if command -v sha256sum >/dev/null 2>&1; then
  sha_tool=sha256sum
elif command -v shasum >/dev/null 2>&1; then
  sha_tool=shasum
else
  fail "a SHA-256 tool (sha256sum or shasum) is required and was not found on PATH"
fi

dir=${REQ_INSTALL_DIR:-$HOME/.local/bin}
mkdir -p "$dir"
# Staged inside the install directory: the same filesystem (the final move is a rename) and a
# place that is known to allow execution. Removed on every exit, so a refusal leaves nothing.
stage=$(mktemp -d "$dir/.req-install.XXXXXX")
trap 'rm -rf "$stage"' EXIT

base=${install_base:-https://github.com/autox-my/req/releases/download}
proto=${install_proto:-=https}
asset=req-$REV-$target.tar.gz
url=$base/rev-$REV/$asset

# The header exists only when a token was given (a public release needs none). It is built by the
# shell builtin `printf` and read by curl from stdin: the token is never in an argument list (`ps`),
# and nothing here is traced. With no token the header file is empty and no Authorization is sent.
status=0
code=$({ if [ -n "${REQ_RELEASE_TOKEN:-}" ]; then printf 'Authorization: token %s\n' "$REQ_RELEASE_TOKEN"; fi; } |
  curl --proto "$proto" --tlsv1.2 -fsSL -H @- -o "$stage/$asset" -w '%{http_code}' "$url") || status=$?
case $code in
  2??)
    if [ "$status" -ne 0 ]; then
      fail "download failed (curl exit $status) for $url"
    fi
    ;;
  401 | 403)
    fail "HTTP $code for $url: the host refused the request; if the release is private, check that REQ_RELEASE_TOKEN is valid and can read it, otherwise unset REQ_RELEASE_TOKEN"
    ;;
  404)
    fail "HTTP 404 for $url: there is no release rev-$REV for $target, or (for a private fork) the token cannot see it; check REV and REQ_RELEASE_TOKEN"
    ;;
  *)
    fail "HTTP $code (curl exit $status) for $url"
    ;;
esac

# Verified against the PIN above, never against a checksum fetched from the release.
sha_ok() {
  if [ "$sha_tool" = sha256sum ]; then
    printf '%s  %s\n' "$pin" "$asset" | (cd "$stage" && sha256sum -c -) >/dev/null 2>&1
  else
    printf '%s  %s\n' "$pin" "$asset" | (cd "$stage" && shasum -a 256 -c -) >/dev/null 2>&1
  fi
}
if ! sha_ok; then
  fail "SHA-256 mismatch: $asset does not match the SHA-256 pinned for $target"
fi

# One member, `req`, a regular file, at the archive root: no prefix, no second member, no `..`,
# no absolute path, no link. Checked from the listing, before anything is extracted.
members=$(tar -tzf "$stage/$asset" 2>/dev/null) || members='(unreadable archive)'
kind=$(tar -tvzf "$stage/$asset" 2>/dev/null | cut -c1) || kind='?'
if [ "$members" != req ] || [ "$kind" != - ]; then
  fail "$asset must hold exactly one member named req, a regular file at the archive root"
fi
mkdir "$stage/unpacked"
tar -xzf "$stage/$asset" -C "$stage/unpacked"
chmod 0755 "$stage/unpacked/req"

# The binary is run from the staging directory and installed only if it is the pinned rev.
version=0
out=$("$stage/unpacked/req" --version 2>&1) || version=$?
if [ "$version" -ne 0 ]; then
  fail "the downloaded req does not run (exit $version): $(printf '%s' "$out" | head -c 300 | LC_ALL=C tr -c '[:print:]' '?')"
fi
built=$(printf '%s\n' "$out" | sed -n 's/^ *built from  *\([^ ]*\) *$/\1/p')
if [ "$built" != "$REV" ]; then
  fail "the downloaded req reports built from '${built:-(no such line)}', not the pinned $REV"
fi

mv -f "$stage/unpacked/req" "$dir/req"
printf 'installed %s\n' "$dir/req"
printf '%s\n' "$out" | sed -n -e '/^ *rules_v /p' -e '/^ *built from /p'
