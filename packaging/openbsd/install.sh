#!/bin/sh
# packaging/openbsd/install.sh
#
# UWAGA UCZCIWOŚCI: prawdziwy natywny pakiet OpenBSD (.tgz w formacie
# pkg_create/pkg_add, z sygnaturą pkg_sign) wymaga hosta OpenBSD i narzędzi
# BSD (pkg_create, signify) - nie da się tego zbudować wiarygodnie na
# hostowanym runnerze Ubuntu w GitHub Actions bez emulacji całego systemu.
#
# Zamiast udawać, że to prawdziwy .tgz pkg_add, workflow buduje binarkę
# GOOS=openbsd (cross-kompilacja Go działa bez problemu) i pakuje ją razem
# z tym skryptem instalacyjnym w zwykły tar.gz. To jest uczciwy odpowiednik
# "pliku instalacyjnego" bez fałszowania natywnego formatu pakietu.
#
# Użycie na maszynie OpenBSD:
#   tar xzf hyperc-<wersja>-openbsd-amd64.tar.gz
#   doas sh install.sh

set -e

PREFIX="${PREFIX:-/usr/local}"
BINDIR="$PREFIX/bin"

if [ "$(id -u)" -ne 0 ]; then
  echo "install.sh: uruchom przez 'doas sh install.sh' (wymagane uprawnienia zapisu do $BINDIR)" >&2
  exit 1
fi

install -d "$BINDIR"
install -m 755 hyperc "$BINDIR/hyperc"

echo "hyperc zainstalowany w $BINDIR/hyperc"
echo "sprawdź: hyperc build --help"
