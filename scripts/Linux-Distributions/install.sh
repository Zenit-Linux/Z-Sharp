#!/bin/sh
# Instalator narzędzi Z# (zsi, zzc, zsc, zst) dla dowolnej dystrybucji Linuksa.
#
#   curl -fsSL https://raw.githubusercontent.com/Zenit-Linux/Z-Sharp/main/scripts/Linux-Distributions/install.sh | sh
#   curl -fsSL .../install.sh | sh -s -- --immutable
#
# Wersja każdego narzędzia pochodzi z jego Z#.toml (https://github.com/Zenit-Linux/<narzędzie>/blob/main/Z%23.toml),
# a binarka z https://github.com/Zenit-Linux/<narzędzie>/releases/download/<wersja>/<narzędzie>
# (próbowane są tagi `<wersja>` i `v<wersja>`).
#
# Domyślnie: instalacja systemowa do /usr/local/bin (sudo/doas, gdy nie jesteś rootem).
# --immutable (także zapis --immuntable): dla dystrybucji niezmiennych (Silverblue, Kinoite, SteamOS, NixOS, Vanilla OS…) —
#   instaluje w katalogu domowym ($HOME/.local/bin), bez uprawnień roota.
#
# Użycie: install.sh [opcje] [narzędzie...]
#   narzędzie            zsi zzc zsc zst (domyślnie wszystkie)
#   --immutable          instalacja w $HOME (alias: --immuntable)
#   --prefix DIR         katalog docelowy na binarki (nadpisuje /usr/local/bin i $HOME/.local/bin)
#   --version WER        wymuś wersję (pomija odczyt Z#.toml)
#   --dry-run            tylko pokaż, co zostałoby zrobione
#   -q, --quiet          mniej komunikatów
#   -h, --help           ta pomoc
# Zmienne: ZS_ORG (domyślnie Zenit-Linux), ZS_RAW_BASE, ZS_RELEASE_BASE (nadpisanie adresów, np. lustro lub test lokalny).
set -eu

ORG="${ZS_ORG:-Zenit-Linux}"
RAW_BASE="${ZS_RAW_BASE:-https://raw.githubusercontent.com/$ORG}"
REL_BASE="${ZS_RELEASE_BASE:-https://github.com/$ORG}"
ALL_TOOLS="zsi zzc zsc zst"

IMMUTABLE=0; PREFIX=""; FORCE_VERSION=""; DRY=0; QUIET=0; TOOLS=""

say()  { [ "$QUIET" = 1 ] || printf '%s\n' "$*"; }
warn() { printf 'uwaga: %s\n' "$*" >&2; }
die()  { printf 'błąd: %s\n' "$*" >&2; exit 1; }
usage() { sed -n '2,/^set -eu/p' "$0" 2>/dev/null | sed '$d' | sed 's/^# \{0,1\}//' || true; }

while [ $# -gt 0 ]; do
  case "$1" in
    --immutable|--immuntable) IMMUTABLE=1 ;;
    --prefix)   [ $# -ge 2 ] || die "--prefix wymaga argumentu"; PREFIX="$2"; shift ;;
    --prefix=*) PREFIX="${1#--prefix=}" ;;
    --version)  [ $# -ge 2 ] || die "--version wymaga argumentu"; FORCE_VERSION="$2"; shift ;;
    --version=*) FORCE_VERSION="${1#--version=}" ;;
    --dry-run)  DRY=1 ;;
    -q|--quiet) QUIET=1 ;;
    -h|--help)  usage; exit 0 ;;
    -*)         die "nieznana opcja: $1 (spróbuj --help)" ;;
    *)          case " $ALL_TOOLS " in *" $1 "*) TOOLS="$TOOLS $1" ;; *) die "nieznane narzędzie: $1 (dostępne: $ALL_TOOLS)" ;; esac ;;
  esac
  shift
done
[ -n "$TOOLS" ] || TOOLS="$ALL_TOOLS"

[ "$(uname -s)" = "Linux" ] || die "ten instalator obsługuje tylko Linuksa (wykryto: $(uname -s))"
case "$(uname -m)" in
  x86_64|amd64) ;;
  *) warn "architektura $(uname -m): wydania są budowane dla x86_64 — instalacja może się nie uruchomić" ;;
esac

# ── pobieranie ──
if command -v curl >/dev/null 2>&1; then
  fetch()   { curl -fsSL --retry 3 --connect-timeout 15 -o "$2" "$1"; }
  fetch_s() { curl -fsSL --retry 3 --connect-timeout 15 "$1"; }
elif command -v wget >/dev/null 2>&1; then
  fetch()   { wget -q -O "$2" "$1"; }
  fetch_s() { wget -q -O - "$1"; }
else
  die "potrzebne jest curl albo wget"
fi

# ── katalog docelowy i uprawnienia ──
if [ -n "$PREFIX" ]; then BINDIR="$PREFIX"
elif [ "$IMMUTABLE" = 1 ]; then
  [ -n "${HOME:-}" ] || die "brak \$HOME — nie można zainstalować w trybie --immutable"
  BINDIR="$HOME/.local/bin"
else BINDIR="/usr/local/bin"; fi

SUDO=""
needs_root() {
  # czy trzeba uprawnień, żeby zapisać w $BINDIR (lub go utworzyć)?
  d="$BINDIR"
  while [ ! -d "$d" ]; do d="$(dirname "$d")"; done
  [ ! -w "$d" ]
}
if needs_root && [ "$(id -u)" != 0 ]; then
  if command -v sudo >/dev/null 2>&1; then SUDO="sudo"
  elif command -v doas >/dev/null 2>&1; then SUDO="doas"
  else die "brak uprawnień do $BINDIR i brak sudo/doas. Uruchom jako root albo użyj --immutable (instalacja w \$HOME)"; fi
fi

if [ "$IMMUTABLE" = 0 ] && [ -z "$PREFIX" ]; then
  if [ -e /run/ostree-booted ] || { [ -r /etc/os-release ] && grep -qiE '^ID=("?)(nixos|steamos|vanilla|blendos|bazzite)' /etc/os-release; }; then
    warn "wygląda na dystrybucję niezmienną — jeśli zapis do $BINDIR się nie uda, uruchom ponownie z --immutable"
  fi
fi

# ── wersja ──
tool_version() {
  [ -z "$FORCE_VERSION" ] || { printf '%s' "$FORCE_VERSION"; return 0; }
  fetch_s "$RAW_BASE/$1/main/Z%23.toml" 2>/dev/null | awk '
    /^[ \t]*\[/ { sect = $0; gsub(/[ \t]/, "", sect); next }
    sect == "[package]" && /^[ \t]*version[ \t]*=/ { v = $0; sub(/^[^=]*=[ \t]*/, "", v); gsub(/["\047 \t\r]/, "", v); print v; exit }'
}

TMP="$(mktemp -d 2>/dev/null || mktemp -d -t zsinst)"
trap 'rm -rf "$TMP"' EXIT INT TERM

install_one() {
  tool="$1"
  ver="$(tool_version "$tool" || true)"
  [ -n "$ver" ] || { warn "$tool: nie udało się odczytać wersji z Z#.toml ($RAW_BASE/$tool/main/Z%23.toml)"; return 1; }
  got=""
  for tag in "$ver" "v$ver"; do
    url="$REL_BASE/$tool/releases/download/$tag/$tool"
    if [ "$DRY" = 1 ]; then say "[dry-run] $tool $ver <- $url"; got="dry"; break; fi
    if fetch "$url" "$TMP/$tool" 2>/dev/null; then got="$url"; break; fi
  done
  [ -n "$got" ] || { warn "$tool: nie znaleziono binarki dla wersji $ver ($REL_BASE/$tool/releases/download/$ver/$tool)"; return 1; }
  [ "$DRY" = 1 ] && { say "[dry-run] -> $BINDIR/$tool"; return 0; }
  # opcjonalna suma kontrolna, jeśli wydanie ją zawiera
  if fetch "$got.sha256" "$TMP/$tool.sha256" 2>/dev/null && command -v sha256sum >/dev/null 2>&1; then
    want="$(awk '{print $1; exit}' "$TMP/$tool.sha256")"
    have="$(sha256sum "$TMP/$tool" | awk '{print $1}')"
    [ "$want" = "$have" ] || { warn "$tool: suma SHA-256 się nie zgadza"; return 1; }
  fi
  [ -s "$TMP/$tool" ] || { warn "$tool: pobrany plik jest pusty"; return 1; }
  chmod 755 "$TMP/$tool"
  $SUDO mkdir -p "$BINDIR"
  $SUDO install -m 755 "$TMP/$tool" "$BINDIR/$tool.new" 2>/dev/null || { $SUDO cp "$TMP/$tool" "$BINDIR/$tool.new" && $SUDO chmod 755 "$BINDIR/$tool.new"; } \
    || { warn "$tool: nie można zapisać w $BINDIR"; return 1; }
  $SUDO mv -f "$BINDIR/$tool.new" "$BINDIR/$tool"
  say "zainstalowano $tool $ver -> $BINDIR/$tool"
}

say "Z# — instalacja narzędzi:$TOOLS (katalog: $BINDIR)"
FAILED=""
for t in $TOOLS; do install_one "$t" || FAILED="$FAILED $t"; done

case ":$PATH:" in *":$BINDIR:"*) ;; *) [ "$DRY" = 1 ] || say "
$BINDIR nie jest w \$PATH. Dodaj do ~/.profile (lub ~/.bashrc / ~/.zshrc):
    export PATH=\"$BINDIR:\$PATH\"" ;; esac

if [ -n "$FAILED" ]; then printf 'nie udało się zainstalować:%s\n' "$FAILED" >&2; exit 1; fi
say "gotowe."
