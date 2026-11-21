#!/bin/sh
# Deinstalator narzędzi Z# (zsi, zzc, zsc, zst) dla dowolnej dystrybucji Linuksa.
#
#   curl -fsSL https://raw.githubusercontent.com/Zenit-Linux/Z-Sharp/main/scripts/Linux-Distributions/remove.sh | sh
#   curl -fsSL .../remove.sh | sh -s -- --immutable
#
# Usuwa tylko te narzędzia, które faktycznie istnieją w katalogu docelowym; brakujące są pomijane.
# Domyślnie: /usr/local/bin (sudo/doas, gdy nie jesteś rootem).
# --immutable (także zapis --immuntable): usuwa z katalogu domowego ($HOME/.local/bin) — odpowiednik instalacji z install.sh --immutable.
#
# Użycie: remove.sh [opcje] [narzędzie...]
#   narzędzie            zsi zzc zsc zst (domyślnie wszystkie)
#   --immutable          usuwa z $HOME (alias: --immuntable)
#   --prefix DIR         katalog z binarkami (nadpisuje /usr/local/bin i $HOME/.local/bin)
#   --purge              usuwa też pamięć podręczną biblioteki standardowej ($HOME/.cache/z-sharp)
#   --dry-run            tylko pokaż, co zostałoby usunięte
#   -q, --quiet          mniej komunikatów
#   -h, --help           ta pomoc
set -eu

ALL_TOOLS="zsi zzc zsc zst"
IMMUTABLE=0; PREFIX=""; DRY=0; QUIET=0; PURGE=0; TOOLS=""

say()  { [ "$QUIET" = 1 ] || printf '%s\n' "$*"; }
warn() { printf 'uwaga: %s\n' "$*" >&2; }
die()  { printf 'błąd: %s\n' "$*" >&2; exit 1; }
usage() { sed -n '2,/^set -eu/p' "$0" 2>/dev/null | sed '$d' | sed 's/^# \{0,1\}//' || true; }

while [ $# -gt 0 ]; do
  case "$1" in
    --immutable|--immuntable) IMMUTABLE=1 ;;
    --prefix)   [ $# -ge 2 ] || die "--prefix wymaga argumentu"; PREFIX="$2"; shift ;;
    --prefix=*) PREFIX="${1#--prefix=}" ;;
    --purge)    PURGE=1 ;;
    --dry-run)  DRY=1 ;;
    -q|--quiet) QUIET=1 ;;
    -h|--help)  usage; exit 0 ;;
    -*)         die "nieznana opcja: $1 (spróbuj --help)" ;;
    *)          case " $ALL_TOOLS " in *" $1 "*) TOOLS="$TOOLS $1" ;; *) die "nieznane narzędzie: $1 (dostępne: $ALL_TOOLS)" ;; esac ;;
  esac
  shift
done
[ -n "$TOOLS" ] || TOOLS="$ALL_TOOLS"

[ "$(uname -s)" = "Linux" ] || die "ten skrypt obsługuje tylko Linuksa (wykryto: $(uname -s))"

if [ -n "$PREFIX" ]; then BINDIR="$PREFIX"
elif [ "$IMMUTABLE" = 1 ]; then
  [ -n "${HOME:-}" ] || die "brak \$HOME — nie można usuwać w trybie --immutable"
  BINDIR="$HOME/.local/bin"
else BINDIR="/usr/local/bin"; fi

SUDO=""
if [ -d "$BINDIR" ] && [ ! -w "$BINDIR" ] && [ "$(id -u)" != 0 ]; then
  if command -v sudo >/dev/null 2>&1; then SUDO="sudo"
  elif command -v doas >/dev/null 2>&1; then SUDO="doas"
  else die "brak uprawnień do $BINDIR i brak sudo/doas. Uruchom jako root albo użyj --immutable"; fi
fi

say "Z# — usuwanie narzędzi:$TOOLS (katalog: $BINDIR)"
REMOVED=0; FAILED=""
for t in $TOOLS; do
  p="$BINDIR/$t"
  if [ -e "$p" ] || [ -L "$p" ]; then
    if [ "$DRY" = 1 ]; then say "[dry-run] usunięto by $p"; REMOVED=$((REMOVED + 1)); continue; fi
    if $SUDO rm -f "$p"; then say "usunięto $p"; REMOVED=$((REMOVED + 1)); else warn "nie udało się usunąć $p"; FAILED="$FAILED $t"; fi
  else
    say "pominięto $t (brak $p)"
  fi
done

if [ "$PURGE" = 1 ]; then
  cache="${XDG_CACHE_HOME:-${HOME:-/root}/.cache}/z-sharp"
  if [ -d "$cache" ]; then
    if [ "$DRY" = 1 ]; then say "[dry-run] usunięto by $cache"; else rm -rf "$cache" && say "usunięto $cache"; fi
  fi
fi

[ -z "$FAILED" ] || { printf 'nie udało się usunąć:%s\n' "$FAILED" >&2; exit 1; }
[ "$REMOVED" -gt 0 ] || say "nic do usunięcia."
say "gotowe."
