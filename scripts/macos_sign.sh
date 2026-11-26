#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Ad-hoc sign the macOS wcr and wcr-gui in a release folder, then check every
# program in it (#404). Used by release.yml before packing and by
# macos-signing.yml to prove it on a real Apple Silicon runner.
#
# The linker's own signature ("linker-signed") is only meant for a program
# built and run on the same Mac. A downloaded copy needs a real signature;
# Apple Silicon refuses to start an arm64 program without one. modem73 and
# libs/ are already signed by modem73.yml, so they are only checked here.
#
# Usage: scripts/macos_sign.sh DIR
set -eu
DIR="$1"

sign() {
  [ -f "$DIR/$1" ] || return 0
  codesign --force -s - --identifier "$2" "$DIR/$1"
}
sign wcr com.thaum-labs.wcr
sign wcr-gui com.thaum-labs.wcr-gui

HOST_ARCH=$(uname -m)
for f in "$DIR/wcr" "$DIR/wcr-gui" "$DIR/modem73" "$DIR"/libs/*.dylib; do
  [ -f "$f" ] || continue
  echo "== $f"
  file "$f"
  INFO=$(codesign -dv --verbose=4 "$f" 2>&1)
  printf '%s\n' "$INFO" | grep -E '^(Identifier|Format|CodeDirectory|Signature|flags)' || true
  if printf '%s\n' "$INFO" | grep -q 'linker-signed'; then
    echo "::error::$f only has the linker's signature"
    exit 1
  fi
  codesign --verify --strict --verbose "$f"
done

# Start the programs, not just look at them. A cross-built Intel copy on an
# Apple Silicon runner is skipped: it would need Rosetta.
run_ok() {
  name="$1"
  shift
  ARCHS=$(lipo -archs "$DIR/$name")
  case " $ARCHS " in
    *" $HOST_ARCH "*) ;;
    *) echo "skip running $name ($ARCHS on a $HOST_ARCH host)"; return 0 ;;
  esac
  echo "== $name $*"
  "$DIR/$name" "$@"
}
run_ok wcr --version
run_ok wcr --help > /dev/null
echo "wcr --help ran"
if [ -f "$DIR/modem73" ]; then
  ARCHS=$(lipo -archs "$DIR/modem73")
  case " $ARCHS " in
    *" $HOST_ARCH "*)
      # modem73 --help exits non-zero; check it printed its usage instead.
      HELP=$("$DIR/modem73" --help 2>&1 || true)
      printf '%s\n' "$HELP" | grep -q "tcp:HOST:PORT" || { printf '%s\n' "$HELP"; echo "::error::modem73 --help did not run"; exit 1; }
      echo "modem73 --help ran"
      ;;
  esac
fi
echo "macOS signing check passed for $DIR"
