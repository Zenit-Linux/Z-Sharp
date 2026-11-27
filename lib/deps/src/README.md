# Ziarna bootstrapu

Pliki wygenerowane z `zsc/src` (nie edytować ręcznie). Każde z nich wystarcza do zbudowania `zsc` bez Rusta:

| plik | wymaga | zbudowany przez |
|---|---|---|
| `zsc-seed.c` | kompilator C11 (`cc -O1 -std=gnu11 zsc-seed.c -lm`) | `zst --to c` |
| `zsc-seed.ll` | `llc` >= 21 + kompilator C (runtime) | `zsc --emit ll` |
| `zsc-seed.js` | Node >= 16 | `zst --to js` |
| `zsc-seed.php` | PHP >= 8.1 | `zst --to php` |

`SHA256SUMS` — sumy kontrolne. Odświeżanie: `UPDATE_SEED=1 sh bootstrap/bootstrap.sh`.
Test spójności: wszystkie cztery ziarna prowadzą do tej samej binarki `zsc` po czterech etapach.
