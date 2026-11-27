#define _GNU_SOURCE
#include "zs_native.h"
#include <stdlib.h>
#include <string.h>
#include <stdarg.h>
#include <math.h>
#include <errno.h>
#include <unistd.h>
#include <dirent.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <time.h>
#include <ctype.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <netdb.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <arpa/inet.h>
#include <poll.h>
#include <fcntl.h>

ZsHandler *zs_h = NULL;
V zs_cur_exc;
int64_t zs_line = 0;
static int g_argc; static char **g_argv;

/* ───────── ZAMM (M4): Z# Adaptive Memory Management — areny łańcuchowe, BEZ odśmiecacza ─────────
 * Model (docs/MEMORY.md): każde wywołanie funkcji dostaje własną *arenę ramki* (łańcuch chunków 4…64 KiB, alokacja = przesunięcie wskaźnika).
 * Wyjście z ramki (`zs_leave`) zwalnia całą arenę jednym ruchem — bez skanowania sterty, bez pauz, bez zliczania referencji.
 *   • Poziom (level) wartości = głębokość ramki, która ją zaalokowała (tabela stron `zm_pgl`; duże obiekty: nagłówek). Poziom 0 = trwała (persistent).
 *   • Inwariant: wartość nigdy nie wskazuje na wartość o *wyższym* poziomie (młodszą). Gdy wartość miałaby uciec do starszej areny —
 *     `return` (do areny wywołującego), zapis do starszego kontenera (bariera `zm_bar`), pole statyczne, wyjątek (do ramki z `catch`) —
 *     jest *promowana*: kopiowana głęboko do areny docelowej (`zm_prom`, kolejka zamiast rekurencji). Obiekty mają tożsamość → po promocji
 *     stary egzemplarz dostaje wskaźnik przekierowujący `fwd` (wszystkie dostępy idą przez `obj_canon`).
 *   • Duże obiekty (> 16 KiB) to Large Object Space (malloc + lista przy arenie, zwalniane z areną); duże napisy są promowane bez kopiowania.
 * Strojenie/diagnostyka: ZS_MEM_STATS=1 (raport na stderr przy wyjściu). Runtime jest jednowątkowy. */
#include <sys/mman.h>
#define ZM_PAGE 4096u
#define ZM_MAXRUN 16u
#define ZM_LARGE ((size_t)(16 * 1024))
typedef struct ZmChunk { struct ZmChunk *next; uint32_t pages, pad; } ZmChunk;
typedef struct ZmLarge { struct ZmLarge *prev, *next; uint32_t level, pad; size_t size; } ZmLarge;
typedef struct { char *cur, *end; ZmChunk *chunks; ZmLarge *large; uint32_t next_pages; size_t used; } ZmArena;
static char *zm_base, *zm_lim; static uint32_t *zm_pgl; static size_t zm_npages, zm_top;
static ZmChunk *zm_freel[ZM_MAXRUN + 1]; static size_t zm_free_pages;
static ZmArena *zm_ar; static uint32_t zm_cap;
uint32_t zs_depth; static uint32_t zm_tgt;
static int zm_inited, zm_stats_on;
static uint64_t zm_n_alloc, zm_b_alloc, zm_b_prom, zm_n_prom, zm_b_rel, zm_n_frames, zm_live, zm_peak, zm_n_unwind;
static void zm_oom(void) { fputs("zs: brak pamięci\n", stderr); exit(1); }
static void zm_init(void) {
    if (zm_inited) return; zm_inited = 1;
    static const size_t sz[] = {(size_t)64 << 30, (size_t)16 << 30, (size_t)4 << 30, (size_t)1 << 30, (size_t)256 << 20};
    for (size_t i = 0; i < sizeof sz / sizeof *sz && !zm_base; i++) {
        void *p = mmap(NULL, sz[i], PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE, -1, 0);
        if (p != MAP_FAILED) { zm_base = p; zm_lim = zm_base + sz[i]; zm_npages = sz[i] / ZM_PAGE; }
    }
    if (!zm_base) zm_oom();
    void *t = mmap(NULL, zm_npages * sizeof(uint32_t), PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE, -1, 0);
    if (t == MAP_FAILED) zm_oom();
    zm_pgl = t; zm_cap = 256; zm_ar = calloc(zm_cap, sizeof *zm_ar); if (!zm_ar) zm_oom();
    const char *e = getenv("ZS_MEM_STATS"); if (e && e[0] == '1') zm_stats_on = 1;
}
static inline int zm_in(const void *p) { return (const char *)p >= zm_base && (const char *)p < zm_lim; }
static inline uint32_t zm_level(const void *p) { return zm_in(p) ? zm_pgl[(size_t)((const char *)p - zm_base) / ZM_PAGE] : ((const ZmLarge *)p)[-1].level; }
static ZmChunk *zm_take(uint32_t n, uint32_t level) {
    ZmChunk *c = NULL;
    if (zm_freel[n]) { c = zm_freel[n]; zm_freel[n] = c->next; zm_free_pages -= n; }
    else for (uint32_t m = n + 1; m <= ZM_MAXRUN; m++) if (zm_freel[m]) {
        c = zm_freel[m]; zm_freel[m] = c->next; ZmChunk *rest = (ZmChunk *)((char *)c + (size_t)n * ZM_PAGE);
        rest->pages = m - n; rest->next = zm_freel[m - n]; zm_freel[m - n] = rest; zm_free_pages -= n; break;
    }
    if (!c) { if (zm_top + n > zm_npages) zm_oom(); c = (ZmChunk *)(zm_base + zm_top * ZM_PAGE); zm_top += n; }
    c->pages = n; c->next = NULL;
    size_t pg = (size_t)((char *)c - zm_base) / ZM_PAGE; for (uint32_t i = 0; i < n; i++) zm_pgl[pg + i] = level;
    return c;
}
static void zm_put(ZmChunk *c) {
    uint32_t n = c->pages; zm_free_pages += n;
    if (zm_free_pages > 32768) madvise(c, (size_t)n * ZM_PAGE, MADV_DONTNEED);   /* zapas wolnych stron > 128 MiB wraca do systemu */
    c->pages = n; c->next = zm_freel[n]; zm_freel[n] = c;
}
static void *zm_slow(ZmArena *A, uint32_t level, size_t n) {
    if (n > ZM_LARGE) {
        ZmLarge *L = malloc(sizeof *L + n); if (!L) zm_oom();
        L->level = level; L->size = n; L->prev = NULL; L->next = A->large; if (A->large) A->large->prev = L; A->large = L;
        return L + 1;
    }
    uint32_t pages = A->next_pages ? A->next_pages : 1;
    while ((size_t)pages * ZM_PAGE - sizeof(ZmChunk) < n) pages <<= 1;
    ZmChunk *c = zm_take(pages, level); c->next = A->chunks; A->chunks = c;
    A->next_pages = pages < ZM_MAXRUN ? pages << 1 : ZM_MAXRUN;
    A->cur = (char *)c + sizeof *c; A->end = (char *)c + (size_t)pages * ZM_PAGE;
    void *p = A->cur; A->cur += n; return p;
}
static inline void *zm_alloc_at(uint32_t level, size_t n, int zero) {
    if (!zm_inited) zm_init();
    n = (n + 15) & ~(size_t)15; if (!n) n = 16;
    ZmArena *A = &zm_ar[level]; void *p;
    zm_n_alloc++; zm_b_alloc += n; A->used += n; zm_live += n; if (zm_live > zm_peak) zm_peak = zm_live;
    if (n <= ZM_LARGE && (uintptr_t)A->cur + n <= (uintptr_t)A->end) { p = A->cur; A->cur += n; } else p = zm_slow(A, level, n);
    if (zero) memset(p, 0, n);
    return p;
}
static inline void *zm_alloc(size_t n, int atomic) { return zm_alloc_at(zm_tgt, n, !atomic); }
static void zm_free(void *p) {                      /* jawne zwolnienie dużego bufora (małe żyją do końca areny) */
    if (!p || zm_in(p)) return;
    ZmLarge *L = (ZmLarge *)p - 1; ZmArena *A = &zm_ar[L->level];
    if (L->prev) L->prev->next = L->next; else A->large = L->next;
    if (L->next) L->next->prev = L->prev;
    A->used -= L->size; zm_live -= L->size; free(L);
}
static void *zm_grow(uint32_t level, void *old, size_t oldn, size_t newn) {
    void *p = zm_alloc_at(level, newn, 0); if (old) { memcpy(p, old, oldn < newn ? oldn : newn); zm_free(old); } return p;
}
static void zm_release(ZmArena *A) {
    for (ZmChunk *c = A->chunks, *n; c; c = n) { n = c->next; zm_put(c); }
    for (ZmLarge *L = A->large, *n; L; L = n) { n = L->next; free(L); }
    zm_live -= A->used; zm_b_rel += A->used;
    A->chunks = NULL; A->large = NULL; A->cur = A->end = NULL; A->next_pages = 0; A->used = 0;
}
/* ── promocja (głęboka kopia do areny `L`) ── */
typedef struct ZObj { ZStr *cls; ZArr *f; struct ZObj *fwd; } ZObj;
typedef struct ZFn { V id; V cap; } ZFn;
static inline ZObj *obj_canon(void *p) { ZObj *x = p; while (x->fwd) x = x->fwd; return x; }
struct ZArr { int64_t rc, n, cap, next; V *keys; V *vals; int32_t *idx; int64_t icap; };
static V **zm_wl; static size_t zm_wn, zm_wcap;
static void zm_wpush(V *s) {
    if (s->tag < ZS_STR) return;
    if (zm_wn == zm_wcap) { zm_wcap = zm_wcap ? zm_wcap * 2 : 1024; zm_wl = realloc(zm_wl, zm_wcap * sizeof *zm_wl); if (!zm_wl) zm_oom(); }
    zm_wl[zm_wn++] = s;
}
static ZStr *zm_str_to(ZStr *s, uint32_t L) {
    uint32_t lv = zm_level(s); if (lv <= L) return s;
    if (!zm_in(s)) {                                  /* duży napis: zmiana właściciela zamiast kopiowania */
        ZmLarge *h = (ZmLarge *)s - 1; ZmArena *A = &zm_ar[lv], *B = &zm_ar[L];
        if (h->prev) h->prev->next = h->next; else A->large = h->next;
        if (h->next) h->next->prev = h->prev;
        A->used -= h->size; zm_live -= h->size; B->used += h->size; zm_live += h->size;
        h->level = L; h->prev = NULL; h->next = B->large; if (B->large) B->large->prev = h; B->large = h; return s;
    }
    size_t n = sizeof(ZStr) + (size_t)s->len + 1; ZStr *d = zm_alloc_at(L, n, 0); memcpy(d, s, n); zm_b_prom += n; zm_n_prom++; return d;
}
static ZArr *zm_arr_to(ZArr *o, uint32_t L) {
    ZArr *a = zm_alloc_at(L, sizeof *a, 1); a->rc = 1; a->n = o->n; a->next = o->next; a->cap = o->n < 4 ? 4 : o->n;
    a->keys = zm_alloc_at(L, sizeof(V) * (size_t)a->cap, 0); a->vals = zm_alloc_at(L, sizeof(V) * (size_t)a->cap, 0);
    memcpy(a->keys, o->keys, sizeof(V) * (size_t)o->n); memcpy(a->vals, o->vals, sizeof(V) * (size_t)o->n);
    zm_b_prom += sizeof *a + 2 * sizeof(V) * (size_t)o->n; zm_n_prom++;
    for (int64_t i = 0; i < a->n; i++) { zm_wpush(&a->keys[i]); zm_wpush(&a->vals[i]); }
    return a;
}
static void zm_prom_one(V *s, uint32_t L) {
    switch (s->tag) {
        case ZS_STR: s->u.s = zm_str_to(s->u.s, L); break;
        case ZS_ARRAY: if (zm_level(s->u.a) > L) s->u.a = zm_arr_to(s->u.a, L); break;
        case ZS_OBJ: {
            ZObj *x = obj_canon(s->u.p);
            if (zm_level(x) > L) {
                ZObj *n = zm_alloc_at(L, sizeof *n, 1); x->fwd = n; n->fwd = NULL;
                n->cls = zm_str_to(x->cls, L); n->f = zm_arr_to(x->f, L); zm_n_prom++; x = n;
            }
            s->u.p = x; break;
        }
        case ZS_EXC: if (zm_level(s->u.x) > L) { ZExc *n = zm_alloc_at(L, sizeof *n, 1); *n = *s->u.x; s->u.x = n; zm_wpush(&n->msg); zm_n_prom++; } break;
        case ZS_FN: if (zm_level(s->u.p) > L) { ZFn *n = zm_alloc_at(L, sizeof *n, 1); *n = *(ZFn *)s->u.p; s->u.p = n; zm_wpush(&n->id); zm_wpush(&n->cap); zm_n_prom++; } break;
        default: break;
    }
}
static V zm_prom(V v, uint32_t L) {
    size_t base = zm_wn; V r = v; zm_prom_one(&r, L);
    while (zm_wn > base) { V *s = zm_wl[--zm_wn]; zm_prom_one(s, L); }
    return r;
}
/* bariera zapisu: wartość zapisywana do kontenera `owner` nie może być młodsza od niego */
static inline V zm_bar(V v, const void *owner) {
    if (v.tag < ZS_STR) return v;
    if (v.tag == ZS_OBJ) v.u.p = obj_canon(v.u.p);
    uint32_t lo = zm_level(owner);
    if (zm_level(v.u.p) > lo) v = zm_prom(v, lo);
    return v;
}
/* ── ramki ── */
static void zm_unwind(uint32_t to) { while (zs_depth > to) { zm_release(&zm_ar[zs_depth]); zs_depth--; zm_n_unwind++; } zm_tgt = zs_depth; }
/* regiony iteracji pętli: promocja zmiennej przenoszonej między iteracjami do poziomu `lvl` + zwolnienie aren głębszych niż `lvl` */
V zs_carry_slow(V v, uint32_t lvl) {
    if (v.tag == ZS_OBJ) v.u.p = obj_canon(v.u.p);
    return zm_level(v.u.p) > lvl ? zm_prom(v, lvl) : v;
}
V zs_carry_v(V v, uint32_t lvl) { return zs_carry(v, lvl); }   /* wersja dla LLVM (wszystkie tagi) */
void zs_pop(uint32_t lvl) { if (zs_depth > lvl) zm_unwind(lvl); }
uint32_t zs_cur_depth(void) { return zs_depth; }
uint32_t zs_enter(void) {
    if (!zm_inited) zm_init();
    if (zs_depth + 2 >= zm_cap) { uint32_t nc = zm_cap * 2; zm_ar = realloc(zm_ar, nc * sizeof *zm_ar); if (!zm_ar) zm_oom(); memset(zm_ar + zm_cap, 0, (nc - zm_cap) * sizeof *zm_ar); zm_cap = nc; }
    zs_depth++; zm_tgt = zs_depth; zm_n_frames++; return zs_depth;
}
V zs_leave(uint32_t fl, V r) {                      /* wyjście z ramki `fl`: zwalnia także otwarte regiony pętli (return w pętli) */
    if (r.tag >= ZS_STR) {
        if (r.tag == ZS_OBJ) r.u.p = obj_canon(r.u.p);
        if (zm_level(r.u.p) >= fl) r = zm_prom(r, fl - 1);
    }
    zm_unwind(fl - 1); return r;
}
void zs_try_push(ZsHandler *h) { h->prev = zs_h; h->depth = zs_depth; zs_h = h; }
static void zm_report(void) {
    if (!zm_stats_on) return;
    fprintf(stderr, "[zs-zamm] alokacje=%llu (%llu B) ramki=%llu zwolnione=%llu B promocje=%llu (%llu B) odwinięte=%llu żywe=%llu B szczyt=%llu B strony=%zu\n",
            (unsigned long long)zm_n_alloc, (unsigned long long)zm_b_alloc, (unsigned long long)zm_n_frames, (unsigned long long)zm_b_rel,
            (unsigned long long)zm_n_prom, (unsigned long long)zm_b_prom, (unsigned long long)zm_n_unwind, (unsigned long long)zm_live, (unsigned long long)zm_peak, zm_top);
}
#define gc_alloc(n, a) zm_alloc((n), (a))
#define gc_alloc_scan(n) zm_alloc((n), 0)
#define gc_free(p) zm_free(p)


/* ───────── pamięć ───────── */
static void *xmalloc(size_t n) { void *p = malloc(n ? n : 1); if (!p) { fputs("zs: brak pamięci\n", stderr); exit(1); } return p; }
static void *xrealloc(void *q, size_t n) { void *p = realloc(q, n ? n : 1); if (!p) { fputs("zs: brak pamięci\n", stderr); exit(1); } return p; }

/* ───────── wartości podstawowe ───────── */
V zv_null(void) { V v; v.tag = ZS_NULL; v.u.i = 0; return v; }
V zv_bool(int b) { V v; v.tag = ZS_BOOL; v.u.i = b ? 1 : 0; return v; }
V zv_int(int64_t i) { V v; v.tag = ZS_INT; v.u.i = i; return v; }
V zv_float(double f) { V v; v.tag = ZS_FLOAT; v.u.f = f; return v; }
static ZStr *mkstr(int64_t n) { ZStr *s = gc_alloc(sizeof(ZStr) + (size_t)n + 1, 1); s->len = n; s->d[n] = 0; return s; }
V zv_strn(const char *p, int64_t n) { ZStr *s = mkstr(n); if (n) memcpy(s->d, p, (size_t)n); V v; v.tag = ZS_STR; v.u.s = s; return v; }
V zv_str(const char *p) { return zv_strn(p, (int64_t)strlen(p)); }
int64_t zv_i(V v) { return v.u.i; }

const char *zv_tn(V v) {
    if (v.tag == ZS_FN) return "function";
    if (v.tag == ZS_OBJ) return obj_canon(v.u.p)->cls->d;
    switch (v.tag) {
        case ZS_NULL: return "null"; case ZS_BOOL: return "bool"; case ZS_INT: return "int"; case ZS_FLOAT: return "float";
        case ZS_STR: return "string"; case ZS_ARRAY: return "array"; default: return "object";
    }
}

/* ───────── bufor tekstowy ───────── */
typedef struct { char *d; size_t n, cap; } Buf;
static void bput(Buf *b, const char *p, size_t n) {
    if (b->n + n + 1 > b->cap) { b->cap = (b->n + n + 1) * 2 + 32; b->d = xrealloc(b->d, b->cap); }
    memcpy(b->d + b->n, p, n); b->n += n; b->d[b->n] = 0;
}
static void bputs(Buf *b, const char *s) { bput(b, s, strlen(s)); }
static void bfmt(Buf *b, const char *fmt, ...) {
    char tmp[512]; va_list ap; va_start(ap, fmt); int n = vsnprintf(tmp, sizeof tmp, fmt, ap); va_end(ap);
    if (n >= (int)sizeof tmp) { char *big = xmalloc((size_t)n + 1); va_start(ap, fmt); vsnprintf(big, (size_t)n + 1, fmt, ap); va_end(ap); bput(b, big, (size_t)n); free(big); }
    else bput(b, tmp, (size_t)n);
}
static V bval(Buf *b) { V v = zv_strn(b->d ? b->d : "", (int64_t)b->n); free(b->d); return v; }

/* ───────── formatowanie liczb zmiennoprzecinkowych (jak Rust `{}` / fmt_float) ───────── */
static void fmt_float(Buf *b, double f) {
    if (isnan(f)) { bputs(b, "NaN"); return; }
    if (isinf(f)) { bputs(b, f > 0 ? "Infinity" : "-Infinity"); return; }
    if (f == floor(f) && fabs(f) < 1e15) { bfmt(b, "%.1f", f); return; }
    /* najkrótsza reprezentacja round-trip, zapis pozycyjny (bez wykładnika) */
    char e[64]; int p;
    for (p = 0; p < 17; p++) { snprintf(e, sizeof e, "%.*e", p, f); if (strtod(e, NULL) == f) break; }
    snprintf(e, sizeof e, "%.*e", p, f);
    int neg = e[0] == '-'; const char *q = e + neg;
    char digits[40]; int nd = 0;
    for (; *q && *q != 'e'; q++) if (*q != '.') digits[nd++] = *q;
    int ex = atoi(q + 1);
    if (neg) bputs(b, "-");
    int point = ex + 1;      /* pozycja przecinka względem początku cyfr */
    if (point <= 0) { bputs(b, "0."); for (int i = 0; i < -point; i++) bputs(b, "0"); bput(b, digits, (size_t)nd); }
    else if (point >= nd) { bput(b, digits, (size_t)nd); for (int i = nd; i < point; i++) bputs(b, "0"); }
    else { bput(b, digits, (size_t)point); bputs(b, "."); bput(b, digits + point, (size_t)(nd - point)); }
}

static void dump_str(Buf *b, ZStr *s) {
    bputs(b, "\"");
    for (int64_t i = 0; i < s->len; i++) {
        unsigned char c = (unsigned char)s->d[i];
        if (c == '"') bputs(b, "\\\""); else if (c == '\\') bputs(b, "\\\\");
        else if (c == '\n') bputs(b, "\\n"); else if (c == '\r') bputs(b, "\\r"); else if (c == '\t') bputs(b, "\\t");
        else if (c == 0) bputs(b, "\\0");
        else if (c < 0x20 || c == 0x7f) bfmt(b, "\\u{%x}", c);
        else bput(b, (char *)&c, 1);
    }
    bputs(b, "\"");
}

static void text_into(Buf *b, V v, int dumpmode);
static void dump_into(Buf *b, V v) { text_into(b, v, 1); }

static void text_into(Buf *b, V v, int dumpmode) {
    switch (v.tag) {
        case ZS_NULL: bputs(b, "null"); break;
        case ZS_BOOL: bputs(b, v.u.i ? "true" : "false"); break;
        case ZS_INT: bfmt(b, "%lld", (long long)v.u.i); break;
        case ZS_FLOAT: fmt_float(b, v.u.f); break;
        case ZS_STR: if (dumpmode) dump_str(b, v.u.s); else bput(b, v.u.s->d, (size_t)v.u.s->len); break;
        case ZS_ARRAY: {
            ZArr *a = v.u.a; int seq = 1;
            for (int64_t i = 0; i < a->n; i++) if (!(a->keys[i].tag == ZS_INT && a->keys[i].u.i == i)) { seq = 0; break; }
            bputs(b, "[");
            for (int64_t i = 0; i < a->n; i++) {
                if (i) bputs(b, ", ");
                if (!seq) { if (a->keys[i].tag == ZS_INT) bfmt(b, "%lld", (long long)a->keys[i].u.i); else dump_str(b, a->keys[i].u.s); bputs(b, " => "); }
                dump_into(b, a->vals[i]);
            }
            bputs(b, "]"); break;
        }
        case ZS_OBJ: bputs(b, "<object "); bputs(b, obj_canon(v.u.p)->cls->d); bputs(b, ">"); break;
        case ZS_FN: bputs(b, "<function>"); break;
        default: bputs(b, v.tag == ZS_EXC ? (v.u.x->kind ? "<object Error>" : "<object Exception>") : "<object>"); break;
    }
}
static V to_text(V v) { if (v.tag == ZS_STR) return v; Buf b = {0}; text_into(&b, v, 0); return bval(&b); }

int zv_truthy(V v) {
    switch (v.tag) {
        case ZS_NULL: return 0; case ZS_BOOL: case ZS_INT: return v.u.i != 0; case ZS_FLOAT: return v.u.f != 0.0;
        case ZS_STR: return !(v.u.s->len == 0 || (v.u.s->len == 1 && v.u.s->d[0] == '0'));
        case ZS_ARRAY: return v.u.a->n > 0; default: return 1;
    }
}


/* jak String::from_utf8_lossy: każda maksymalna niepoprawna podsekwencja -> U+FFFD */
static V utf8_lossy(const char *p, int64_t n) {
    Buf b = {0}; int64_t i = 0;
    while (i < n) {
        unsigned char c = (unsigned char)p[i]; int64_t need = 0; unsigned char lo = 0x80, hi = 0xBF;
        if (c < 0x80) { bput(&b, p + i, 1); i++; continue; }
        else if (c >= 0xC2 && c <= 0xDF) need = 1;
        else if (c == 0xE0) { need = 2; lo = 0xA0; } else if (c >= 0xE1 && c <= 0xEC) need = 2; else if (c == 0xED) { need = 2; hi = 0x9F; } else if (c >= 0xEE && c <= 0xEF) need = 2;
        else if (c == 0xF0) { need = 3; lo = 0x90; } else if (c >= 0xF1 && c <= 0xF3) need = 3; else if (c == 0xF4) { need = 3; hi = 0x8F; }
        else { bputs(&b, "\xEF\xBF\xBD"); i++; continue; }
        int64_t j = 1, ok = 1;
        for (; j <= need; j++) {
            if (i + j >= n) { ok = 0; break; }
            unsigned char d = (unsigned char)p[i + j]; unsigned char l = j == 1 ? lo : 0x80, h = j == 1 ? hi : 0xBF;
            if (d < l || d > h) { ok = 0; break; }
        }
        if (ok) { bput(&b, p + i, (size_t)(need + 1)); i += need + 1; } else { bputs(&b, "\xEF\xBF\xBD"); i += j; }
    }
    return bval(&b);
}

/* ───────── wyjątki ───────── */
V zv_exc_new(int kind, V msg) { ZExc *x = gc_alloc_scan(sizeof *x); x->kind = kind; x->msg = msg; x->line = 0; V v; v.tag = ZS_EXC; v.u.x = x; return v; }
static V obj_field(V o, const char *name);           /* null gdy brak */
static void obj_set_field(V o, const char *name, V v);
void zs_reraise(V exc) {
    if (!zs_h) {
        fflush(stdout);
        if (exc.tag == ZS_OBJ) {
            V m = to_text(obj_field(exc, "message")); V ln = obj_field(exc, "line");
            fprintf(stderr, "Nieobsłużony wyjątek %s: %s (linia %lld)\n", zv_tn(exc), m.u.s->d, (long long)(ln.tag == ZS_INT ? ln.u.i : 0));
            exit(1);
        }
        V m = to_text(exc.u.x->msg);
        fprintf(stderr, "Nieobsłużony wyjątek %s: %s (linia %lld)\n", exc.u.x->kind ? "Error" : "Exception", m.u.s->d, (long long)exc.u.x->line);
        exit(1);
    }
    { ZsHandler *h = zs_h; V e = exc; if (e.tag >= ZS_STR) e = zm_prom(e, h->depth); zs_cur_exc = e; zm_unwind(h->depth); longjmp(h->jb, 1); }
}
void zs_raise(V exc) {
    if (exc.tag == ZS_OBJ) { V ln = obj_field(exc, "line"); if (ln.tag != ZS_INT || ln.u.i == 0) obj_set_field(exc, "line", zv_int(zs_line)); zs_reraise(exc); }
    if (exc.u.x->line == 0) exc.u.x->line = zs_line; zs_reraise(exc); }
void zs_die(const char *fmt, ...) {
    char tmp[1024]; va_list ap; va_start(ap, fmt); vsnprintf(tmp, sizeof tmp, fmt, ap); va_end(ap);
    V e = zv_exc_new(1, zv_str(tmp)); e.u.x->line = zs_line; zs_reraise(e);
}
int zv_exc_is(V x, int kind) { return x.tag == ZS_EXC && (kind == 0 || x.u.x->kind == kind); }
V b_getmessage(V x) { if (x.tag != ZS_EXC) zs_die("wywołanie metody `getMessage` na wartości typu %s", zv_tn(x)); return x.u.x->msg; }
V b_throw_v(V x) { if (x.tag != ZS_EXC && x.tag != ZS_OBJ) zs_die("throw wymaga obiektu typu Throwable"); zs_raise(x); }

/* ───────── tablice ───────── */
static uint64_t hash_key(V k) {
    if (k.tag == ZS_INT) { uint64_t x = (uint64_t)k.u.i; x ^= x >> 33; x *= 0xff51afd7ed558ccdULL; x ^= x >> 33; return x; }
    uint64_t h = 1469598103934665603ULL; for (int64_t i = 0; i < k.u.s->len; i++) { h ^= (unsigned char)k.u.s->d[i]; h *= 1099511628211ULL; } return h;
}
static int key_eq(V a, V b) {
    if (a.tag != b.tag) return 0;
    if (a.tag == ZS_INT) return a.u.i == b.u.i;
    return a.u.s->len == b.u.s->len && memcmp(a.u.s->d, b.u.s->d, (size_t)a.u.s->len) == 0;
}
static ZArr *arr_alloc(int64_t cap) {
    ZArr *a = gc_alloc_scan(sizeof *a); a->rc = 1; a->n = 0; a->next = 0; a->cap = cap < 4 ? 4 : cap;
    a->keys = gc_alloc_scan(sizeof(V) * (size_t)a->cap); a->vals = gc_alloc_scan(sizeof(V) * (size_t)a->cap);
    a->idx = NULL; a->icap = 0; return a;
}
static void arr_reindex(ZArr *a) {
    int64_t want = 16; while (want < a->cap * 2) want <<= 1;
    zm_free(a->idx); a->icap = want; a->idx = zm_alloc_at(zm_level(a), sizeof(int32_t) * (size_t)want, 0); memset(a->idx, 0, sizeof(int32_t) * (size_t)want);
    for (int64_t i = 0; i < a->n; i++) { uint64_t h = hash_key(a->keys[i]) & (uint64_t)(want - 1); while (a->idx[h]) h = (h + 1) & (uint64_t)(want - 1); a->idx[h] = (int32_t)(i + 1); }
}
static int64_t arr_find(ZArr *a, V k) {
    if (a->n <= 8) { for (int64_t i = 0; i < a->n; i++) if (key_eq(a->keys[i], k)) return i; return -1; }
    if (!a->idx) arr_reindex(a);
    uint64_t m = (uint64_t)(a->icap - 1), h = hash_key(k) & m;
    while (a->idx[h]) { int64_t i = a->idx[h] - 1; if (key_eq(a->keys[i], k)) return i; h = (h + 1) & m; }
    return -1;
}
static void arr_put(ZArr *a, V k, V v) {
    v = zm_bar(v, a); k = zm_bar(k, a);
    int64_t i = arr_find(a, k);
    if (i >= 0) { a->vals[i] = v; return; }
    if (a->n == a->cap) { { int64_t oc = a->cap; a->cap *= 2; uint32_t lv = zm_level(a); a->keys = zm_grow(lv, a->keys, sizeof(V) * (size_t)oc, sizeof(V) * (size_t)a->cap); a->vals = zm_grow(lv, a->vals, sizeof(V) * (size_t)oc, sizeof(V) * (size_t)a->cap); } if (a->idx) arr_reindex(a); }
    a->keys[a->n] = k; a->vals[a->n] = v; a->n++;
    if (k.tag == ZS_INT && k.u.i >= a->next) a->next = k.u.i + 1;
    if (a->idx) {
        if (a->n * 2 > a->icap) arr_reindex(a);
        else { uint64_t m = (uint64_t)(a->icap - 1), h = hash_key(k) & m; while (a->idx[h]) h = (h + 1) & m; a->idx[h] = (int32_t)a->n; }
    }
}
static V mkarr(ZArr *a) { V v; v.tag = ZS_ARRAY; v.u.a = a; return v; }
V zv_arr_new(int64_t cap) { return mkarr(arr_alloc(cap)); }
int64_t zs_array_len(V a) { return a.tag == ZS_ARRAY ? a.u.a->n : 0; }
V zv_dup(V v) { if (v.tag == ZS_ARRAY) v.u.a->rc++; return v; }
static ZArr *arr_copy(ZArr *o) {
    ZArr *a = arr_alloc(o->n); memcpy(a->keys, o->keys, sizeof(V) * (size_t)o->n); memcpy(a->vals, o->vals, sizeof(V) * (size_t)o->n);
    for (int64_t i = 0; i < o->n; i++) if (a->vals[i].tag == ZS_ARRAY) a->vals[i].u.a->rc++;
    a->n = o->n; a->next = o->next; return a;
}
V zv_wr(V v) {
    if (v.tag == ZS_NULL) return zv_arr_new(4);
    if (v.tag != ZS_ARRAY) zs_die("nie można indeksować wartości typu %s przy zapisie", zv_tn(v));
    if (v.u.a->rc > 1) { v.u.a->rc--; return mkarr(arr_copy(v.u.a)); }
    return v;
}
static V norm_key(V k) {
    switch (k.tag) {
        case ZS_INT: return k;
        case ZS_BOOL: return zv_int(k.u.i);
        case ZS_FLOAT: if (k.u.f == floor(k.u.f) && isfinite(k.u.f)) return zv_int((int64_t)k.u.f); break;
        case ZS_STR: {
            const char *s = k.u.s->d; int64_t n = k.u.s->len, i = 0;
            if (n > 0 && n <= 20) {
                if (s[0] == '-') i = 1;
                if (i < n && ((s[i] != '0') || n == i + 1) && !(i == 1 && s[1] == '0')) {
                    int ok = 1; for (int64_t j = i; j < n; j++) if (s[j] < '0' || s[j] > '9') { ok = 0; break; }
                    if (ok) { errno = 0; char *e; long long x = strtoll(s, &e, 10); if (!errno && *e == 0) return zv_int(x); }
                }
            }
            return k;
        }
    }
    zs_die("niepoprawny klucz tablicy typu %s", zv_tn(k));
}
void zv_append(V a, V v) { arr_put(a.u.a, zv_int(a.u.a->next), v); }
void zv_set(V a, V k, V v) {
    if (a.tag != ZS_ARRAY) zs_die("nie można indeksować wartości typu %s przy zapisie", zv_tn(a));
    arr_put(a.u.a, norm_key(k), v);
}
static V index_impl(V a, V k, int quiet) {
    if (a.tag == ZS_ARRAY) {
        V nk = norm_key(k); int64_t i = arr_find(a.u.a, nk);
        if (i >= 0) return a.u.a->vals[i];
        if (quiet) return zv_null();
        Buf b = {0}; if (nk.tag == ZS_INT) bfmt(&b, "%lld", (long long)nk.u.i); else dump_str(&b, nk.u.s);
        zs_die("brak klucza %s", b.d);
    }
    if (a.tag == ZS_STR) {
        if (k.tag != ZS_INT) zs_die("indeks tekstu musi być liczbą całkowitą");
        int64_t len = a.u.s->len, i = k.u.i < 0 ? len + k.u.i : k.u.i;
        if (i < 0 || i >= len) { if (quiet) return zv_null(); zs_die("indeks %lld poza zakresem tekstu (długość %lld)", (long long)i, (long long)len); }
        unsigned char c = (unsigned char)a.u.s->d[i];
        if ((c & 0xC0) == 0x80) { if (quiet) return zv_null(); zs_die("indeks %lld wskazuje środek znaku UTF-8", (long long)i); }
        int64_t n = c < 0x80 ? 1 : c < 0xE0 ? 2 : c < 0xF0 ? 3 : 4; if (i + n > len) n = len - i;
        return zv_strn(a.u.s->d + i, n);
    }
    if (a.tag == ZS_NULL) { if (quiet) return zv_null(); zs_die("indeksowanie wartości null"); }
    zs_die("typ %s nie obsługuje indeksowania", zv_tn(a));
}
V zv_index(V a, V k) { return index_impl(a, k, 0); }
V zv_index_opt(V a, V k) { return index_impl(a, k, 1); }
V zv_get_raw(V a, int64_t i) { return a.u.a->vals[i]; }
V zv_key_at(V a, int64_t i) { return a.u.a->keys[i]; }

/* ───────── porównania i operatory ───────── */
static double as_f(V v) { return v.tag == ZS_INT ? (double)v.u.i : v.u.f; }
static int isnum(V v) { return v.tag == ZS_INT || v.tag == ZS_FLOAT; }
int zv_ident(V a, V b) {
    if (a.tag != b.tag) return 0;
    switch (a.tag) {
        case ZS_NULL: return 1; case ZS_BOOL: case ZS_INT: return a.u.i == b.u.i; case ZS_FLOAT: return a.u.f == b.u.f;
        case ZS_STR: return a.u.s->len == b.u.s->len && memcmp(a.u.s->d, b.u.s->d, (size_t)a.u.s->len) == 0;
        case ZS_ARRAY:
            if (a.u.a == b.u.a) return 1; if (a.u.a->n != b.u.a->n) return 0;
            for (int64_t i = 0; i < a.u.a->n; i++) if (!key_eq(a.u.a->keys[i], b.u.a->keys[i]) || !zv_ident(a.u.a->vals[i], b.u.a->vals[i])) return 0;
            return 1;
        case ZS_OBJ: return obj_canon(a.u.p) == obj_canon(b.u.p);
        default: return a.u.p == b.u.p;
    }
}
int zv_eq(V a, V b) { if (isnum(a) && isnum(b) && a.tag != b.tag) return as_f(a) == as_f(b); return zv_ident(a, b); }
int zv_cmp(V a, V b) {
    if (a.tag == ZS_INT && b.tag == ZS_INT) return a.u.i < b.u.i ? -1 : a.u.i > b.u.i;
    if (isnum(a) && isnum(b)) { double x = as_f(a), y = as_f(b); return x < y ? -1 : x > y; }
    if (a.tag == ZS_STR && b.tag == ZS_STR) {
        int64_t n = a.u.s->len < b.u.s->len ? a.u.s->len : b.u.s->len; int c = memcmp(a.u.s->d, b.u.s->d, (size_t)n);
        if (c) return c < 0 ? -1 : 1; return a.u.s->len < b.u.s->len ? -1 : a.u.s->len > b.u.s->len;
    }
    if (a.tag == ZS_BOOL && b.tag == ZS_BOOL) return a.u.i < b.u.i ? -1 : a.u.i > b.u.i;
    zs_die("nie można porównać %s z %s", zv_tn(a), zv_tn(b));
}
static void bad_op(const char *op, V a, V b) { zs_die("operacja %s niedostępna dla %s i %s", op, zv_tn(a), zv_tn(b)); }
V zv_add(V a, V b) {
    if (a.tag == ZS_INT && b.tag == ZS_INT) { int64_t r; if (__builtin_add_overflow(a.u.i, b.u.i, &r)) zs_die("przepełnienie liczby całkowitej"); return zv_int(r); }
    if (isnum(a) && isnum(b)) return zv_float(as_f(a) + as_f(b));
    if (a.tag == ZS_ARRAY && b.tag == ZS_ARRAY) {
        ZArr *r = arr_copy(a.u.a);
        for (int64_t i = 0; i < b.u.a->n; i++) if (arr_find(r, b.u.a->keys[i]) < 0) { if (b.u.a->vals[i].tag == ZS_ARRAY) b.u.a->vals[i].u.a->rc++; arr_put(r, b.u.a->keys[i], b.u.a->vals[i]); }
        return mkarr(r);
    }
    bad_op("Add", a, b);
}
V zv_sub(V a, V b) {
    if (a.tag == ZS_INT && b.tag == ZS_INT) { int64_t r; if (__builtin_sub_overflow(a.u.i, b.u.i, &r)) zs_die("przepełnienie liczby całkowitej"); return zv_int(r); }
    if (isnum(a) && isnum(b)) return zv_float(as_f(a) - as_f(b));
    bad_op("Sub", a, b);
}
V zv_mul(V a, V b) {
    if (a.tag == ZS_INT && b.tag == ZS_INT) { int64_t r; if (__builtin_mul_overflow(a.u.i, b.u.i, &r)) zs_die("przepełnienie liczby całkowitej"); return zv_int(r); }
    if (isnum(a) && isnum(b)) return zv_float(as_f(a) * as_f(b));
    bad_op("Mul", a, b);
}
V zv_div(V a, V b) {
    if (!(isnum(a) && isnum(b))) bad_op("Div", a, b);
    if (as_f(b) == 0.0) zs_die("dzielenie przez zero");
    if (a.tag == ZS_INT && b.tag == ZS_INT && a.u.i % b.u.i == 0) return zv_int(a.u.i / b.u.i);
    return zv_float(as_f(a) / as_f(b));
}
V zv_mod(V a, V b) {
    if (a.tag == ZS_INT && b.tag == ZS_INT) { if (b.u.i == 0) zs_die("modulo przez zero"); if (b.u.i == -1) return zv_int(0); return zv_int(a.u.i % b.u.i); }
    bad_op("Mod", a, b);
}
V zv_bitand(V a, V b) { if (a.tag == ZS_INT && b.tag == ZS_INT) return zv_int(a.u.i & b.u.i); bad_op("BitAnd", a, b); }
V zv_bitor(V a, V b)  { if (a.tag == ZS_INT && b.tag == ZS_INT) return zv_int(a.u.i | b.u.i); bad_op("BitOr", a, b); }
V zv_bitxor(V a, V b) { if (a.tag == ZS_INT && b.tag == ZS_INT) return zv_int(a.u.i ^ b.u.i); bad_op("BitXor", a, b); }
V zv_shl(V a, V b) { if (a.tag == ZS_INT && b.tag == ZS_INT) return zv_int((int64_t)((uint64_t)a.u.i << (b.u.i & 63))); bad_op("Shl", a, b); }
V zv_shr(V a, V b) { if (a.tag == ZS_INT && b.tag == ZS_INT) return zv_int(a.u.i >> (b.u.i & 63)); bad_op("Shr", a, b); }
V zv_bitnot(V a) {
    if (a.tag == ZS_INT) return zv_int(~a.u.i);
    zs_die("operator jednoargumentowy BitNot niedostępny dla %s", zv_tn(a));
}
V zv_pow(V a, V b) {
    if (a.tag == ZS_INT && b.tag == ZS_INT && b.u.i >= 0) {
        int64_t r = 1, base = a.u.i, e = b.u.i; int ovf = 0;
        while (e > 0) {
            if (e & 1) { if (__builtin_mul_overflow(r, base, &r)) { ovf = 1; break; } }
            e >>= 1; if (e && __builtin_mul_overflow(base, base, &base)) { ovf = 1; break; }
        }
        if (!ovf) return zv_int(r);
        return zv_float(pow((double)a.u.i, (double)b.u.i));
    }
    if (isnum(a) && isnum(b)) return zv_float(pow(as_f(a), as_f(b)));
    bad_op("Pow", a, b);
}
V zv_concat(V a, V b) {
    V x = to_text(a), y = to_text(b); ZStr *s = mkstr(x.u.s->len + y.u.s->len);
    memcpy(s->d, x.u.s->d, (size_t)x.u.s->len); memcpy(s->d + x.u.s->len, y.u.s->d, (size_t)y.u.s->len);
    V v; v.tag = ZS_STR; v.u.s = s; return v;
}
V zv_neg(V a) {
    if (a.tag == ZS_INT) { if (a.u.i == INT64_MIN) zs_die("przepełnienie liczby całkowitej"); return zv_int(-a.u.i); }
    if (a.tag == ZS_FLOAT) return zv_float(-a.u.f);
    zs_die("operator jednoargumentowy Neg niedostępny dla %s", zv_tn(a));
}
V zv_chk(V v, const char *type, const char *what) {
    int ok = 0;
    if (!strcmp(type, "int")) ok = v.tag == ZS_INT; else if (!strcmp(type, "float")) { ok = v.tag == ZS_FLOAT; if (!ok && v.tag == ZS_INT) return zv_float((double)v.u.i); }
    else if (!strcmp(type, "string")) ok = v.tag == ZS_STR; else if (!strcmp(type, "bool")) ok = v.tag == ZS_BOOL; else if (!strcmp(type, "array")) ok = v.tag == ZS_ARRAY;
    else if (v.tag == ZS_OBJ || v.tag == ZS_EXC) ok = strcmp(type, "int") && strcmp(type, "float") && strcmp(type, "string") && strcmp(type, "bool") && strcmp(type, "array");
    if (ok) return v;
    zs_die("%s: oczekiwano %s, otrzymano %s", what, type, zv_tn(v));
}

/* ───────── obiekty (klasy sprowadzone do funkcji przez lib/classes.zsharp) ───────── */
static V obj_field(V o, const char *name) {
    ZObj *x = obj_canon(o.u.p); int64_t i = arr_find(x->f, zv_str(name));
    return i < 0 ? zv_null() : x->f->vals[i];
}
static void obj_set_field(V o, const char *name, V v) { arr_put(obj_canon(o.u.p)->f, zv_str(name), v); }
V b_zs_obj_new(V cls) {
    ZObj *o = gc_alloc_scan(sizeof(ZObj)); o->cls = cls.u.s; o->f = arr_alloc(4);
    V v; v.tag = ZS_OBJ; v.u.p = o; return v;
}
V b_zs_obj_class(V o) { return o.tag == ZS_OBJ ? zv_str(obj_canon(o.u.p)->cls->d) : zv_null(); }
V b_zs_obj_get(V o, V name) {
    if (o.tag != ZS_OBJ) {
        if (o.tag == ZS_NULL) zs_die("dostęp do właściwości `%s` wartości null", name.u.s->d);
        zs_die("dostęp do właściwości `%s` typu %s", name.u.s->d, zv_tn(o));
    }
    ZObj *x = obj_canon(o.u.p); int64_t i = arr_find(x->f, name);
    if (i < 0) zs_die("obiekt klasy %s nie ma właściwości %s", x->cls->d, name.u.s->d);
    return zv_dup(x->f->vals[i]);
}
V b_zs_obj_take(V o, V name) {   /* wyjmuje wartość pola (pole := null), aby zapis w miejscu nie kopiował tablicy */
    V v = b_zs_obj_get(o, name); ZObj *x = obj_canon(o.u.p); int64_t i = arr_find(x->f, name);
    if (v.tag == ZS_ARRAY) v.u.a->rc--;
    x->f->vals[i] = zv_null(); return v;
}
V b_zs_obj_set(V o, V name, V v) {
    if (o.tag != ZS_OBJ) zs_die("zapis właściwości `%s` na wartości typu %s", name.u.s->d, zv_tn(o));
    arr_put(obj_canon(o.u.p)->f, name, zv_dup(v)); return v;
}
V b_zs_obj_put(V o, V name, V v) { arr_put(obj_canon(o.u.p)->f, name, v); return v; }   /* zapis bez zwiększania rc (para z take) */
V b_zs_no_method(V o, V name) {
    if (o.tag == ZS_OBJ) zs_die("klasa %s nie ma metody %s", zv_tn(o), name.u.s->d);
    zs_die("wywołanie metody `%s` na wartości typu %s", name.u.s->d, zv_tn(o));
}
V b_zs_mkfn(V id, V cap) { ZFn *f = gc_alloc_scan(sizeof *f); f->id = id; f->cap = cap; V v; v.tag = ZS_FN; v.u.p = f; return v; }
V b_zs_fn_id(V f) { if (f.tag == ZS_FN) return ((ZFn *)f.u.p)->id; if (f.tag == ZS_STR) return f; return zv_null(); }
V b_zs_fn_cap(V f) { return f.tag == ZS_FN ? ((ZFn *)f.u.p)->cap : zv_null(); }
V b_is_callable(V f) { return zv_bool(f.tag == ZS_FN); }
static V g_statics;
V b_zs_sget(V name) { if (g_statics.tag != ZS_ARRAY) return zv_null(); int64_t i = arr_find(g_statics.u.a, name); return i < 0 ? zv_null() : zv_dup(g_statics.u.a->vals[i]); }
V b_zs_sset(V name, V v) { if (g_statics.tag != ZS_ARRAY) { uint32_t sv = zm_tgt; zm_tgt = 0; g_statics = zv_arr_new(8); zm_tgt = sv; } arr_put(g_statics.u.a, name, zv_dup(v)); return v; }
V b_zs_no_class(V name) { zs_die("nieznana klasa %s", name.u.s->d); }

/* ───────── wyjście ───────── */
void zs_init(int argc, char **argv) {
    /* ZS_ARG0: launcher (zsi) podaje ścieżkę skryptu jako argv[0], jak interpreter; zmienna nie przecieka do procesów potomnych */
    { const char *a0 = getenv("ZS_ARG0"); if (a0 && *a0 && argc > 0) { argv[0] = strdup(a0); unsetenv("ZS_ARG0"); } }
    g_argc = argc; g_argv = argv; zs_cur_exc = zv_null(); zm_init(); }
void zs_shutdown(void) { fflush(stdout); zm_report(); }
static void out_text(FILE *f, V v) { if (v.tag == ZS_STR) fwrite(v.u.s->d, 1, (size_t)v.u.s->len, f); else { Buf b = {0}; text_into(&b, v, 0); fwrite(b.d, 1, b.n, f); free(b.d); } }
void zs_echo_v(V v, int first) { if (!first) fputc(' ', stdout); out_text(stdout, v); }
void zs_echo_end(void) { fputc('\n', stdout); }
void zs_necho_v(V v) { out_text(stdout, v); }
void zs_eecho_v(V v, int first) { if (first) fflush(stdout); if (!first) fputc(' ', stderr); out_text(stderr, v); }
void zs_eecho_end(void) { fputc('\n', stderr); }
V s_exit(V code) { fflush(stdout); zm_report(); exit(code.tag == ZS_INT ? (int)code.u.i : 0); }

/* ───────── wbudowane ───────── */
static ZStr *need_s(V v, const char *f, int n) { if (v.tag != ZS_STR) zs_die("%s: argument #%d powinien być tekstem, jest %s", f, n, zv_tn(v)); return v.u.s; }
static int64_t need_i(V v, const char *f, int n) { if (v.tag != ZS_INT) zs_die("%s: argument #%d powinien być liczbą całkowitą, jest %s", f, n, zv_tn(v)); return v.u.i; }
static double need_f(V v, const char *f, int n) { if (!isnum(v)) zs_die("%s: argument #%d powinien być liczbą, jest %s", f, n, zv_tn(v)); return as_f(v); }
static ZArr *need_a(V v, const char *f, int n) { if (v.tag != ZS_ARRAY) zs_die("%s: argument #%d powinien być tablicą, jest %s", f, n, zv_tn(v)); return v.u.a; }

V b_count(V v) { if (v.tag == ZS_ARRAY) return zv_int(v.u.a->n); if (v.tag == ZS_STR) return zv_int(v.u.s->len); zs_die("count: niedostępne dla %s", zv_tn(v)); }
V b_strlen(V v) { return zv_int(need_s(v, "strlen", 1)->len); }
V b_typeof(V v) { if (v.tag == ZS_OBJ) return zv_str(zv_tn(v)); return zv_str(v.tag == ZS_EXC ? (v.u.x->kind ? "Error" : "Exception") : zv_tn(v)); }
V b_is_int(V v) { return zv_bool(v.tag == ZS_INT); } V b_is_float(V v) { return zv_bool(v.tag == ZS_FLOAT); }
V b_is_string(V v) { return zv_bool(v.tag == ZS_STR); } V b_is_bool(V v) { return zv_bool(v.tag == ZS_BOOL); }
V b_is_array(V v) { return zv_bool(v.tag == ZS_ARRAY); } V b_is_null(V v) { return zv_bool(v.tag == ZS_NULL); }
V b_strval(V v) { return to_text(v); }
V b_boolval(V v) { return zv_bool(zv_truthy(v)); }
static const char *trimmed(ZStr *s, int64_t *n) {
    const char *p = s->d; int64_t len = s->len;
    while (len > 0 && (*p == ' ' || (*p >= 9 && *p <= 13))) { p++; len--; }
    while (len > 0 && (p[len - 1] == ' ' || (p[len - 1] >= 9 && p[len - 1] <= 13))) len--;
    *n = len; return p;
}
V b_intval(V v) {
    switch (v.tag) {
        case ZS_INT: return v; case ZS_FLOAT: return zv_int(isnan(v.u.f) ? 0 : v.u.f >= 9.2233720368547758e18 ? INT64_MAX : v.u.f <= -9.2233720368547758e18 ? INT64_MIN : (int64_t)v.u.f);
        case ZS_BOOL: return zv_int(v.u.i); case ZS_NULL: return zv_int(0);
        case ZS_STR: {
            int64_t n; const char *p = trimmed(v.u.s, &n); char *t = xmalloc((size_t)n + 1); memcpy(t, p, (size_t)n); t[n] = 0;
            char *e; errno = 0; long long x = strtoll(t, &e, 10);
            if (n > 0 && !errno && *e == 0 && !isspace((unsigned char)t[0])) { free(t); return zv_int(x); }
            double f = strtod(t, &e); int ok = n > 0 && *e == 0 && !isspace((unsigned char)t[0]); free(t);
            return zv_int(ok && !isnan(f) ? (int64_t)f : 0);
        }
        default: zs_die("intval: nie można przekonwertować %s", zv_tn(v));
    }
}
V b_floatval(V v) {
    if (v.tag == ZS_INT) return zv_float((double)v.u.i); if (v.tag == ZS_FLOAT) return v;
    if (v.tag == ZS_STR) { int64_t n; const char *p = trimmed(v.u.s, &n); char *t = xmalloc((size_t)n + 1); memcpy(t, p, (size_t)n); t[n] = 0; char *e; double f = strtod(t, &e); int ok = n > 0 && *e == 0; free(t); return zv_float(ok ? f : 0.0); }
    return zv_float(0.0);
}
V b_ord(V v) { ZStr *s = need_s(v, "ord", 1); if (!s->len) zs_die("ord: pusty tekst"); return zv_int((unsigned char)s->d[0]); }
static void utf8_put(Buf *b, uint32_t c) {
    char t[4]; int n;
    if (c < 0x80) { t[0] = (char)c; n = 1; } else if (c < 0x800) { t[0] = (char)(0xC0 | c >> 6); t[1] = (char)(0x80 | (c & 63)); n = 2; }
    else if (c < 0x10000) { t[0] = (char)(0xE0 | c >> 12); t[1] = (char)(0x80 | ((c >> 6) & 63)); t[2] = (char)(0x80 | (c & 63)); n = 3; }
    else { t[0] = (char)(0xF0 | c >> 18); t[1] = (char)(0x80 | ((c >> 12) & 63)); t[2] = (char)(0x80 | ((c >> 6) & 63)); t[3] = (char)(0x80 | (c & 63)); n = 4; }
    bput(b, t, (size_t)n);
}
V b_chr(V v) { int64_t n = need_i(v, "chr", 1); Buf b = {0}; if (n >= 0 && n < 0x110000 && !(n >= 0xD800 && n < 0xE000)) utf8_put(&b, (uint32_t)n); return bval(&b); }
V b_trim(V v) { int64_t n; const char *p = trimmed(need_s(v, "trim", 1), &n); return zv_strn(p, n); }
static const char *memfind(const char *h, int64_t hn, const char *n, int64_t nn) {
    if (nn == 0) return h; if (nn > hn) return NULL; return memmem(h, (size_t)hn, n, (size_t)nn);
}
V b_contains(V a, V b) { ZStr *x = need_s(a, "contains", 1), *y = need_s(b, "contains", 2); return zv_bool(memfind(x->d, x->len, y->d, y->len) != NULL); }
V b_starts_with(V a, V b) { ZStr *x = need_s(a, "starts_with", 1), *y = need_s(b, "starts_with", 2); return zv_bool(y->len <= x->len && !memcmp(x->d, y->d, (size_t)y->len)); }
V b_ends_with(V a, V b) { ZStr *x = need_s(a, "ends_with", 1), *y = need_s(b, "ends_with", 2); return zv_bool(y->len <= x->len && !memcmp(x->d + x->len - y->len, y->d, (size_t)y->len)); }
V b_index_of(V a, V b) { ZStr *x = need_s(a, "index_of", 1), *y = need_s(b, "index_of", 2); const char *p = memfind(x->d, x->len, y->d, y->len); return zv_int(p ? p - x->d : -1); }
V b_repeat(V a, V b) { ZStr *x = need_s(a, "repeat", 1); int64_t n = need_i(b, "repeat", 2); if (n < 0) n = 0; Buf bf = {0}; for (int64_t i = 0; i < n; i++) bput(&bf, x->d, (size_t)x->len); return bval(&bf); }
V b_substr(V s, V a, V l, int has_len) {
    ZStr *x = need_s(s, "substr", 1); int64_t len = x->len, st = need_i(a, "substr", 2);
    if (st < 0) { st = len + st; if (st < 0) st = 0; } if (st > len) st = len;
    int64_t n;
    if (has_len && l.tag == ZS_INT) { n = l.u.i >= 0 ? l.u.i : len - st + l.u.i; if (n < 0) n = 0; } else n = len - st;
    if (st + n > len) n = len - st;
    return utf8_lossy(x->d + st, n);
}
V b_join(V a, V sep) {
    ZArr *x = need_a(a, "join", 1); ZStr *s = sep.tag == ZS_STR ? sep.u.s : NULL; Buf b = {0};
    for (int64_t i = 0; i < x->n; i++) { if (i && s) bput(&b, s->d, (size_t)s->len); text_into(&b, x->vals[i], 0); }
    return bval(&b);
}
V b_in_array(V needle, V arr) { ZArr *x = need_a(arr, "in_array", 2); for (int64_t i = 0; i < x->n; i++) if (zv_ident(x->vals[i], needle)) return zv_bool(1); return zv_bool(0); }
V b_range(V a, V b) {
    int64_t x = need_i(a, "range", 1), y = need_i(b, "range", 2); V r = zv_arr_new(8);
    if (x <= y) for (int64_t i = x; ; i++) { zv_append(r, zv_int(i)); if (i == y) break; } else for (int64_t i = x; ; i--) { zv_append(r, zv_int(i)); if (i == y) break; }
    return r;
}
V b_keys(V a) { ZArr *x = need_a(a, "keys", 1); V r = zv_arr_new(x->n); for (int64_t i = 0; i < x->n; i++) zv_append(r, x->keys[i]); return r; }
V b_values(V a) { ZArr *x = need_a(a, "values", 1); V r = zv_arr_new(x->n); for (int64_t i = 0; i < x->n; i++) zv_append(r, zv_dup(x->vals[i])); return r; }
V b_has_key(V a, V k) { ZArr *x = need_a(a, "has_key", 1); return zv_bool(arr_find(x, norm_key(k)) >= 0); }
V b_upper(V v) { ZStr *s = need_s(v, "upper", 1); V r = zv_strn(s->d, s->len); for (int64_t i = 0; i < s->len; i++) if (r.u.s->d[i] >= 'a' && r.u.s->d[i] <= 'z') r.u.s->d[i] -= 32; return r; }
V b_lower(V v) { ZStr *s = need_s(v, "lower", 1); V r = zv_strn(s->d, s->len); for (int64_t i = 0; i < s->len; i++) if (r.u.s->d[i] >= 'A' && r.u.s->d[i] <= 'Z') r.u.s->d[i] += 32; return r; }
V b_replace(V a, V f, V t) {
    ZStr *s = need_s(a, "replace", 1), *x = need_s(f, "replace", 2), *y = need_s(t, "replace", 3); Buf b = {0};
    if (x->len == 0) { bput(&b, s->d, (size_t)s->len); return bval(&b); }
    int64_t i = 0; while (i < s->len) { const char *p = memfind(s->d + i, s->len - i, x->d, x->len); if (!p) break; bput(&b, s->d + i, (size_t)(p - s->d - i)); bput(&b, y->d, (size_t)y->len); i = p - s->d + x->len; }
    bput(&b, s->d + i, (size_t)(s->len - i)); return bval(&b);
}
static int64_t u8len(unsigned char c) { return c < 0x80 ? 1 : c < 0xE0 ? 2 : c < 0xF0 ? 3 : 4; }
V b_split(V a, V sep) {
    ZStr *s = need_s(a, "split", 1), *x = need_s(sep, "split", 2); V r = zv_arr_new(8);
    if (x->len == 0) { for (int64_t i = 0; i < s->len;) { int64_t n = u8len((unsigned char)s->d[i]); if (i + n > s->len) n = s->len - i; zv_append(r, zv_strn(s->d + i, n)); i += n; } return r; }
    int64_t i = 0; for (;;) { const char *p = memfind(s->d + i, s->len - i, x->d, x->len); if (!p) { zv_append(r, zv_strn(s->d + i, s->len - i)); break; } zv_append(r, zv_strn(s->d + i, p - s->d - i)); i = p - s->d + x->len; }
    return r;
}
V b_chars(V a) { return b_split(a, zv_str("")); }
static int64_t u8count(ZStr *s) { int64_t n = 0; for (int64_t i = 0; i < s->len; i++) if (((unsigned char)s->d[i] & 0xC0) != 0x80) n++; return n; }
static V pad(V a, V n, V p, int left, const char *f) {
    ZStr *s = need_s(a, f, 1); int64_t w = need_i(n, f, 2); if (w < 0) w = 0;
    const char *pc = " "; int64_t pl = 1; if (p.tag == ZS_STR && p.u.s->len > 0) { pc = p.u.s->d; pl = u8len((unsigned char)pc[0]); if (pl > p.u.s->len) pl = p.u.s->len; }
    int64_t l = u8count(s); if (l >= w) return a;
    Buf b = {0}; if (!left) bput(&b, s->d, (size_t)s->len);
    for (int64_t i = 0; i < w - l; i++) bput(&b, pc, (size_t)pl);
    if (left) bput(&b, s->d, (size_t)s->len); return bval(&b);
}
V b_pad_left(V a, V n) { return pad(a, n, zv_null(), 1, "pad_left"); } V b_pad_right(V a, V n) { return pad(a, n, zv_null(), 0, "pad_right"); }
V b_pad_left3(V a, V n, V p) { return pad(a, n, p, 1, "pad_left"); } V b_pad_right3(V a, V n, V p) { return pad(a, n, p, 0, "pad_right"); }
V b_array_slice3(V a, V s, V l) {
    ZArr *x = need_a(a, "array_slice", 1); int64_t len = x->n, st = need_i(s, "array_slice", 2); if (st < 0) { st = len + st; if (st < 0) st = 0; } if (st > len) st = len;
    int64_t n = l.tag == ZS_INT ? l.u.i : len; if (n < 0) n = 0; int64_t end = st + n; if (end > len) end = len;
    V r = zv_arr_new(end - st); for (int64_t i = st; i < end; i++) zv_append(r, zv_dup(x->vals[i])); return r;
}
V b_array_slice(V a, V s) { return b_array_slice3(a, s, zv_null()); }
V b_reverse(V a) {
    if (a.tag == ZS_STR) { Buf b = {0}; ZStr *s = a.u.s; for (int64_t i = s->len; i > 0;) { int64_t j = i - 1; while (j > 0 && ((unsigned char)s->d[j] & 0xC0) == 0x80) j--; bput(&b, s->d + j, (size_t)(i - j)); i = j; } return bval(&b); }
    if (a.tag != ZS_ARRAY) zs_die("reverse: niedostępne dla %s", zv_tn(a));
    V r = zv_arr_new(a.u.a->n); for (int64_t i = a.u.a->n - 1; i >= 0; i--) zv_append(r, zv_dup(a.u.a->vals[i])); return r;
}
static int cmp_v(const void *p, const void *q) { return zv_cmp(*(const V *)p, *(const V *)q); }
V b_sort(V a) {
    ZArr *x = need_a(a, "sort", 1); V *t = gc_alloc_scan(sizeof(V) * (size_t)(x->n ? x->n : 1)); memcpy(t, x->vals, sizeof(V) * (size_t)x->n);
    /* sortowanie stabilne (scalanie), błędy porównania jako wyjątek `sort: ...` */
    V *tmp = gc_alloc_scan(sizeof(V) * (size_t)(x->n ? x->n : 1));
    for (int64_t w = 1; w < x->n; w <<= 1) {
        for (int64_t lo = 0; lo < x->n; lo += 2 * w) {
            int64_t mid = lo + w < x->n ? lo + w : x->n, hi = lo + 2 * w < x->n ? lo + 2 * w : x->n, i = lo, j = mid, k = lo;
            while (i < mid && j < hi) tmp[k++] = (zv_cmp(t[j], t[i]) < 0) ? t[j++] : t[i++];
            while (i < mid) tmp[k++] = t[i++]; while (j < hi) tmp[k++] = t[j++];
        }
        memcpy(t, tmp, sizeof(V) * (size_t)x->n);
    }
    (void)cmp_v; V r = zv_arr_new(x->n); for (int64_t i = 0; i < x->n; i++) zv_append(r, zv_dup(t[i])); gc_free(t); gc_free(tmp); return r;
}
V b_push(V a, V v) { ZArr *x = need_a(a, "push", 1); (void)x; V r = zv_wr(zv_dup(a)); zv_append(r, v); return r; }
V b_dump(V v) { Buf b = {0}; dump_into(&b, v); bputs(&b, "\n"); fwrite(b.d, 1, b.n, stdout); free(b.d); return zv_null(); }
static int first_is(V v, const char *f, int (*pred)(int)) { ZStr *s = need_s(v, f, 1); return s->len > 0 && pred((unsigned char)s->d[0]); }
static int p_alpha(int c) { return isalpha(c) || c == '_' || c >= 0x80; } static int p_alnum(int c) { return isalnum(c) || c == '_' || c >= 0x80; }
static int p_space(int c) { return c == ' ' || (c >= 9 && c <= 13); }
static int p_digit(int c) { return c >= '0' && c <= '9'; } static int p_hex(int c) { return isxdigit(c); }
V b_is_digit(V v) { return zv_bool(first_is(v, "is_digit", p_digit)); } V b_is_alpha(V v) { return zv_bool(first_is(v, "is_alpha", p_alpha)); }
V b_is_alnum(V v) { return zv_bool(first_is(v, "is_alnum", p_alnum)); } V b_is_space(V v) { return zv_bool(first_is(v, "is_space", p_space)); }
V b_is_hex_digit(V v) { return zv_bool(first_is(v, "is_hex_digit", p_hex)); }
V b_to_hex(V v) { char t[32]; snprintf(t, sizeof t, "%llx", (unsigned long long)need_i(v, "to_hex", 1)); return zv_str(t); }
V b_read_line(void) {
    fflush(stdout); Buf b = {0}; int c, any = 0; while ((c = getchar()) != EOF) { any = 1; if (c == '\n') break; char ch = (char)c; bput(&b, &ch, 1); }
    if (!any) return zv_null(); while (b.n && b.d[b.n - 1] == '\r') b.n--; if (!b.d) return zv_str(""); b.d[b.n] = 0; return bval(&b);
}

/* ───────── moduły ───────── */
V m_Math_sqrt(V v) { return zv_float(sqrt(need_f(v, "Math.sqrt", 1))); }
V m_Math_floor(V v) { return zv_int((int64_t)floor(need_f(v, "Math.floor", 1))); }
V m_Math_ceil(V v) { return zv_int((int64_t)ceil(need_f(v, "Math.ceil", 1))); }
V m_Math_round(V v) { return zv_int((int64_t)round(need_f(v, "Math.round", 1))); }
V m_Math_abs(V v) { if (v.tag == ZS_INT) return zv_int(v.u.i < 0 ? -v.u.i : v.u.i); if (v.tag == ZS_FLOAT) return zv_float(fabs(v.u.f)); zs_die("Math.abs: %s", zv_tn(v)); }
V m_Math_pow(V a, V b) { return zv_pow(a, b); }
V m_Math_min(V a, V b) { return zv_cmp(a, b) <= 0 ? a : b; }
V m_Math_max(V a, V b) { return zv_cmp(a, b) <= 0 ? b : a; }
V m_System_args(void) { V r = zv_arr_new(g_argc); for (int i = 0; i < g_argc; i++) zv_append(r, zv_str(g_argv[i])); return r; }
V m_System_env(V k) { ZStr *s = need_s(k, "System.env", 1); const char *e = getenv(s->d); return e ? zv_str(e) : zv_null(); }
V m_System_os(void) { return zv_str("linux"); }
V m_System_architecture(void) {
#if defined(__x86_64__)
    return zv_str("x86_64");
#elif defined(__aarch64__)
    return zv_str("aarch64");
#else
    return zv_str("unknown");
#endif
}
V m_System_time(void) { struct timespec t; clock_gettime(CLOCK_REALTIME, &t); return zv_float((double)t.tv_sec + (double)t.tv_nsec / 1e9); }
V m_System_cpuCount(void) { long n = sysconf(_SC_NPROCESSORS_ONLN); return zv_int(n > 0 ? n : 1); }
V m_System_version(void) { return zv_str("0.0.1"); }
V m_System_dirname(V p) {
    ZStr *s = need_s(p, "System.dirname", 1); int64_t i = s->len; while (i > 1 && s->d[i - 1] == '/') i--;
    while (i > 0 && s->d[i - 1] != '/') i--; while (i > 1 && s->d[i - 1] == '/') i--;
    if (i == 0) return zv_str("."); if (i == 1 && s->d[0] == '/') return zv_str("/"); return zv_strn(s->d, i);
}
/* System.resolveImport(["a","b"], katalog): jak stage0 — katalog, cwd, ZS_PATH; "Workspace.x" przez Z#.toml (wpis `x = "ścieżka"`) w katalogu lub jego rodzicach (także w ./Z-Sharp) */
static int is_file_(const char *p) { struct stat st; return stat(p, &st) == 0 && S_ISREG(st.st_mode); }
static int with_ext_(char *base, size_t cap, char *out) {
    static const char *ex[] = {".zsharp", ".zs"};
    for (int i = 0; i < 2; i++) { snprintf(out, cap, "%s%s", base, ex[i]); if (is_file_(out)) return 1; }
    return 0;
}
static int manifest_member(const char *toml, const char *name, char *path, size_t cap) {
    FILE *f = fopen(toml, "r"); if (!f) return 0; char ln[2048]; int ok = 0; size_t nl = strlen(name);
    while (fgets(ln, sizeof ln, f)) {
        char *p = ln; while (*p == ' ' || *p == '\t') p++;
        if (strncmp(p, name, nl) != 0) continue; p += nl; while (*p == ' ' || *p == '\t') p++;
        if (*p != '=') continue; p++; while (*p == ' ' || *p == '\t') p++;
        if (*p != '"') continue; p++; char *e = strchr(p, '"'); if (!e) continue;
        snprintf(path, cap, "%.*s", (int)(e - p), p); ok = 1; break;
    }
    fclose(f); return ok;
}
V m_System_resolveImport(V segs, V dir) {
    if (segs.tag != ZS_ARRAY) zs_die("System.resolveImport: argument 1 musi być tablicą");
    ZStr *here = need_s(dir, "System.resolveImport", 2); ZArr *a = segs.u.a; if (a->n == 0) return zv_null();
    char *names[32]; int n = 0; for (int64_t i = 0; i < a->n && n < 32; i++) { V t = to_text(a->vals[i]); names[n++] = t.u.s->d; }
    char out[4096], base[4096], cur[4096];
    if (strcmp(names[0], "Workspace") == 0 && n >= 2) {
        snprintf(cur, sizeof cur, "%s", here->d);
        for (int up = 0; up < 40; up++) {
            static const char *subs[] = {"", "/Z-Sharp"};
            for (int s = 0; s < 2; s++) {
                char toml[4200], mem[2048]; snprintf(toml, sizeof toml, "%s%s/Z#.toml", cur, subs[s]);
                if (manifest_member(toml, names[1], mem, sizeof mem)) {
                    snprintf(base, sizeof base, "%s%s/%s", cur, subs[s], mem);
                    if (is_file_(base)) {
                        for (int k = 2; k < n; k++) {
                            char *sl = strrchr(base, '/'); if (sl) *sl = 0;
                            char nb[4096]; snprintf(nb, sizeof nb, "%s/%s", base, names[k]);
                            if (!with_ext_(nb, sizeof out, out)) return zv_null();
                            snprintf(base, sizeof base, "%s", out);
                        }
                        return zv_str(base);
                    }
                }
            }
            char *sl = strrchr(cur, '/'); if (!sl || sl == cur) { if (strcmp(cur, "/") != 0 && strcmp(cur, ".") != 0) snprintf(cur, sizeof cur, "%s", "."); else break; if (up > 0 && !strcmp(cur, ".")) break; } else *sl = 0;
            if (!strcmp(cur, ".") && up > 0) { char cw[4096]; if (!getcwd(cw, sizeof cw)) break; snprintf(cur, sizeof cur, "%s", cw); }
        }
        return zv_null();
    }
    if (strcmp(names[0], "Std") == 0 && n == 2) {     /* biblioteka standardowa: $ZS_STD, $ZS_HOME/stdlib */
        char lo[256]; size_t l = strlen(names[1]); if (l >= sizeof lo) return zv_null();
        for (size_t i = 0; i <= l; i++) lo[i] = (char)tolower((unsigned char)names[1][i]);
        const char *e1 = getenv("ZS_STD"), *e2 = getenv("ZS_HOME");
        if (e1) { snprintf(base, sizeof base, "%s/%s.zsharp", e1, lo); if (is_file_(base)) return zv_str(base); }
        if (e2) { snprintf(base, sizeof base, "%s/stdlib/%s.zsharp", e2, lo); if (is_file_(base)) return zv_str(base); }
        return zv_null();
    }
    const char *dirs[40]; char cwd[4096], zp[4096]; int nd = 0; dirs[nd++] = here->d;
    if (getcwd(cwd, sizeof cwd)) dirs[nd++] = cwd;
    const char *e = getenv("ZS_PATH");
    if (e) { snprintf(zp, sizeof zp, "%s", e); for (char *t = strtok(zp, ":"); t && nd < 38; t = strtok(NULL, ":")) dirs[nd++] = t; }
    for (int i = 0; i < nd; i++) {
        snprintf(base, sizeof base, "%s", dirs[i]);
        for (int k = 0; k < n; k++) { size_t l = strlen(base); snprintf(base + l, sizeof base - l, "/%s", names[k]); }
        if (with_ext_(base, sizeof out, out)) return zv_str(out);
    }
    return zv_null();
}
static void io_die(const char *p, int e) { zs_die("%s: %s", p, strerror(e)); }
V m_File_read(V p) {
    ZStr *s = need_s(p, "File.read", 1); FILE *f = fopen(s->d, "rb"); if (!f) io_die(s->d, errno);
    Buf b = {0}; char t[65536]; size_t n; while ((n = fread(t, 1, sizeof t, f)) > 0) bput(&b, t, n); fclose(f); return bval(&b);
}
static V file_put(V p, V d, const char *mode, const char *fn) {
    ZStr *s = need_s(p, fn, 1); V t = to_text(d); FILE *f = fopen(s->d, mode); if (!f) io_die(s->d, errno);
    fwrite(t.u.s->d, 1, (size_t)t.u.s->len, f); fclose(f); return zv_null();
}
V m_File_write(V p, V d) { return file_put(p, d, "wb", "File.write"); }
V m_File_append(V p, V d) { return file_put(p, d, "ab", "File.append"); }
V m_File_exists(V p) { struct stat st; ZStr *s = need_s(p, "File.exists", 1); return zv_bool(stat(s->d, &st) == 0 && S_ISREG(st.st_mode)); }
V m_File_delete(V p) { ZStr *s = need_s(p, "File.delete", 1); if (unlink(s->d)) io_die(s->d, errno); return zv_null(); }
V m_Directory_exists(V p) { struct stat st; ZStr *s = need_s(p, "Directory.exists", 1); return zv_bool(stat(s->d, &st) == 0 && S_ISDIR(st.st_mode)); }
V m_Directory_create(V p) {
    ZStr *s = need_s(p, "Directory.create", 1); char *t = xmalloc((size_t)s->len + 1); memcpy(t, s->d, (size_t)s->len + 1);
    for (char *q = t + 1; *q; q++) if (*q == '/') { *q = 0; mkdir(t, 0777); *q = '/'; }
    if (mkdir(t, 0777) && errno != EEXIST) { int e = errno; free(t); io_die(s->d, e); } free(t); return zv_null();
}
static int cmp_s(const void *a, const void *b) { return strcmp(*(char *const *)a, *(char *const *)b); }
V m_Directory_list(V p) {
    ZStr *s = need_s(p, "Directory.list", 1); DIR *d = opendir(s->d); if (!d) io_die(s->d, errno);
    char **nm = NULL; size_t n = 0, cap = 0; struct dirent *e;
    while ((e = readdir(d))) { if (!strcmp(e->d_name, ".") || !strcmp(e->d_name, "..")) continue; if (n == cap) { cap = cap ? cap * 2 : 16; nm = xrealloc(nm, cap * sizeof *nm); } nm[n++] = strdup(e->d_name); }
    closedir(d); qsort(nm, n, sizeof *nm, cmp_s); V r = zv_arr_new((int64_t)n); for (size_t i = 0; i < n; i++) { zv_append(r, zv_str(nm[i])); free(nm[i]); } free(nm); return r;
}
static char **argv_of(V cmd, V args) {
    ZStr *c = need_s(cmd, "Process", 1); size_t n = args.tag == ZS_ARRAY ? (size_t)args.u.a->n : 0; char **av = gc_alloc_scan(sizeof(char *) * (n + 2));
    av[0] = c->d; for (size_t i = 0; i < n; i++) { V t = to_text(args.u.a->vals[i]); av[i + 1] = t.u.s->d; } av[n + 1] = NULL; return av;
}
V m_Process_exec(V cmd, V args) {
    fflush(stdout); char **av = argv_of(cmd, args); pid_t pid = fork(); if (pid < 0) io_die(av[0], errno);
    if (pid == 0) { execvp(av[0], av); fprintf(stderr, "%s: %s\n", av[0], strerror(errno)); _exit(127); }
    int st; waitpid(pid, &st, 0);
    if (WIFEXITED(st)) { if (WEXITSTATUS(st) == 127) { /* nie znaleziono polecenia */ } return zv_int(WEXITSTATUS(st)); } return zv_int(-1);
}
V m_Process_run(V cmd, V args) {
    fflush(stdout); char **av = argv_of(cmd, args); int po[2], pe[2]; if (pipe(po) || pipe(pe)) io_die(av[0], errno);
    pid_t pid = fork(); if (pid < 0) io_die(av[0], errno);
    if (pid == 0) { dup2(po[1], 1); dup2(pe[1], 2); close(po[0]); close(pe[0]); close(po[1]); close(pe[1]); execvp(av[0], av); fprintf(stderr, "%s: %s\n", av[0], strerror(errno)); _exit(127); }
    close(po[1]); close(pe[1]); Buf bo = {0}, be = {0}; char t[8192]; ssize_t n; int fo = 1, fe = 1;
    while (fo || fe) {
        fd_set fs; FD_ZERO(&fs); int mx = 0; if (fo) { FD_SET(po[0], &fs); mx = po[0]; } if (fe) { FD_SET(pe[0], &fs); if (pe[0] > mx) mx = pe[0]; }
        if (select(mx + 1, &fs, NULL, NULL, NULL) < 0) { if (errno == EINTR) continue; break; }
        if (fo && FD_ISSET(po[0], &fs)) { n = read(po[0], t, sizeof t); if (n > 0) bput(&bo, t, (size_t)n); else fo = 0; }
        if (fe && FD_ISSET(pe[0], &fs)) { n = read(pe[0], t, sizeof t); if (n > 0) bput(&be, t, (size_t)n); else fe = 0; }
    }
    close(po[0]); close(pe[0]); int st; waitpid(pid, &st, 0);
    V r = zv_arr_new(3); zv_set(r, zv_str("status"), zv_int(WIFEXITED(st) ? WEXITSTATUS(st) : -1)); zv_set(r, zv_str("stdout"), bval(&bo)); zv_set(r, zv_str("stderr"), bval(&be)); return r;
}

/* ───────── pomocniki dla backendu LLVM ───────── */
V zl_float_lit(const char *s) { return zv_float(strtod(s, NULL)); }
void zl_try_enter(ZsHandler *h) { zs_try_push(h); }
void zl_try_leave(ZsHandler *h) { zs_h = h->prev; }
V zl_cur_exc(void) { return zs_cur_exc; }
void zl_foreach_die(V it) { zs_die("foreach wymaga tablicy, jest %s", zv_tn(it)); }
V zl_undef_var(const char *name) { zs_die("niezdefiniowana zmienna $%s", name); return zv_null(); }


/* ───────── rozszerzenia systemowe (Std.Os / Std.Time / Std.Net) ───────── */
extern char **environ;
static V assoc_new(void) { return zv_arr_new(4); }
static void assoc_set(V a, const char *k, V v) { arr_put(a.u.a, zv_str(k), v); }
static V read_trim_file(const char *p) {
    FILE *f = fopen(p, "rb"); if (!f) return zv_null(); char b[512]; size_t n = fread(b, 1, sizeof b - 1, f); fclose(f);
    while (n > 0 && (b[n - 1] == '\n' || b[n - 1] == '\r' || b[n - 1] == ' ' || b[n - 1] == '\t')) n--;
    size_t s0 = 0; while (s0 < n && (b[s0] == ' ' || b[s0] == '\t' || b[s0] == '\n')) s0++;
    return zv_strn(b + s0, (int64_t)(n - s0));
}
V m_System_setEnv(V k, V v) { ZStr *a = need_s(k, "System.setEnv", 1), *b = need_s(v, "System.setEnv", 2); setenv(a->d, b->d, 1); return zv_null(); }
V m_System_unsetEnv(V k) { unsetenv(need_s(k, "System.unsetEnv", 1)->d); return zv_null(); }
static int cmp_kv(const void *a, const void *b) { return strcmp(*(char *const *)a, *(char *const *)b); }
V m_System_environ(void) {
    int n = 0; while (environ && environ[n]) n++;
    char **tmp = xmalloc(sizeof(char *) * (size_t)(n + 1));
    for (int i = 0; i < n; i++) { tmp[i] = strdup(environ[i]); char *e = strchr(tmp[i], '='); if (e) *e = '\x01'; }   /* sortowanie po (klucz, wartość) */
    qsort(tmp, (size_t)n, sizeof(char *), cmp_kv);
    V r = assoc_new();
    for (int i = 0; i < n; i++) { char *e = strchr(tmp[i], '\x01'); if (e) { *e = 0; arr_put(r.u.a, zv_str(tmp[i]), zv_str(e + 1)); } free(tmp[i]); }
    free(tmp); return r;
}
V m_System_cwd(void) { char b[4096]; if (!getcwd(b, sizeof b)) io_die("System.cwd", errno); return zv_str(b); }
V m_System_chdir(V p) { ZStr *s = need_s(p, "System.chdir", 1); if (chdir(s->d)) io_die(s->d, errno); return zv_null(); }
/* M4: System.gc() wymusza kolekcję; System.gcStats() -> ["collections","allocated","freed","live","heap","peak"] (bajty) */
V m_System_gc(void) { return zv_null(); }   /* ZAMM nie ma kolekcji: pamięć wraca z końcem ramki (nic do wymuszenia) */
V m_System_gcStats(void) {
    V r = assoc_new();
    assoc_set(r, "collections", zv_int(0)); assoc_set(r, "allocated", zv_int((int64_t)zm_b_alloc));
    assoc_set(r, "freed", zv_int((int64_t)zm_b_rel)); assoc_set(r, "live", zv_int((int64_t)zm_live));
    assoc_set(r, "heap", zv_int((int64_t)(zm_top * ZM_PAGE))); assoc_set(r, "peak", zv_int((int64_t)zm_peak));
    assoc_set(r, "promoted", zv_int((int64_t)zm_b_prom)); assoc_set(r, "frames", zv_int((int64_t)zm_n_frames)); assoc_set(r, "depth", zv_int((int64_t)zs_depth));
    return r;
}
V m_System_pid(void) { return zv_int((int64_t)getpid()); }
V m_System_hostname(void) {
    V v = read_trim_file("/proc/sys/kernel/hostname"); if (v.tag == ZS_STR) return v;
    v = read_trim_file("/etc/hostname"); if (v.tag == ZS_STR) return v;
    return zv_str("");
}
V m_System_nowNanos(void) { struct timespec t; clock_gettime(CLOCK_REALTIME, &t); return zv_int((int64_t)t.tv_sec * 1000000000LL + t.tv_nsec); }
V m_System_monotonic(void) {
    static int64_t t0 = -1; struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    int64_t now = (int64_t)t.tv_sec * 1000000000LL + t.tv_nsec; if (t0 < 0) t0 = now;
    return zv_int(now - t0 + 1000000000LL);
}
V m_System_sleep(V ms) {
    int64_t m = need_i(ms, "System.sleep", 1); fflush(stdout);
    if (m > 0) { struct timespec t = { (time_t)(m / 1000), (long)(m % 1000) * 1000000L }; while (nanosleep(&t, &t) && errno == EINTR) {} }
    return zv_null();
}
V m_System_localOffset(V t) { time_t x = (time_t)need_i(t, "System.localOffset", 1); struct tm tm; memset(&tm, 0, sizeof tm); localtime_r(&x, &tm); return zv_int((int64_t)tm.tm_gmtoff); }
V m_File_stat(V p) {
    ZStr *s = need_s(p, "File.stat", 1); struct stat st; if (lstat(s->d, &st)) return zv_null();
    const char *kind = S_ISLNK(st.st_mode) ? "link" : S_ISDIR(st.st_mode) ? "dir" : S_ISREG(st.st_mode) ? "file" : "other";
    V r = assoc_new(); assoc_set(r, "kind", zv_str(kind)); assoc_set(r, "size", zv_int((int64_t)st.st_size));
    assoc_set(r, "mtime", zv_int((int64_t)st.st_mtime)); assoc_set(r, "mode", zv_int((int64_t)(st.st_mode & 07777))); return r;
}
V m_File_rename(V a, V b) { ZStr *x = need_s(a, "File.rename", 1), *y = need_s(b, "File.rename", 2); if (rename(x->d, y->d)) io_die(x->d, errno); return zv_null(); }
V m_File_realpath(V p) { ZStr *s = need_s(p, "File.realpath", 1); char b[4096]; if (!realpath(s->d, b)) io_die(s->d, errno); return zv_str(b); }
V m_File_chmod(V p, V m) { ZStr *s = need_s(p, "File.chmod", 1); if (chmod(s->d, (mode_t)need_i(m, "File.chmod", 2))) io_die(s->d, errno); return zv_null(); }
V m_File_symlink(V t, V l) { ZStr *a = need_s(t, "File.symlink", 1), *b = need_s(l, "File.symlink", 2); if (symlink(a->d, b->d)) io_die(b->d, errno); return zv_null(); }
V m_File_readlink(V p) { ZStr *s = need_s(p, "File.readlink", 1); char b[4096]; ssize_t n = readlink(s->d, b, sizeof b - 1); if (n < 0) io_die(s->d, errno); return zv_strn(b, n); }
V m_Directory_remove(V p) { ZStr *s = need_s(p, "Directory.remove", 1); if (rmdir(s->d)) io_die(s->d, errno); return zv_null(); }

/* ── sieć: uchwyty = kolejne liczby całkowite → deskryptory ── */
typedef struct { int fd; int kind; /* 1 tcp, 2 listener, 3 udp */ } NetH;
static NetH *g_nh; static int64_t g_nn, g_ncap;
static int64_t net_add(int fd, int kind) {
    if (g_nn == g_ncap) { g_ncap = g_ncap ? g_ncap * 2 : 16; g_nh = xrealloc(g_nh, sizeof(NetH) * (size_t)g_ncap); }
    g_nh[g_nn].fd = fd; g_nh[g_nn].kind = kind; g_nn++; return g_nn;   /* uchwyt = indeks + 1 */
}
static NetH *net_get(V h, const char *f) {
    int64_t i = need_i(h, f, 1);
    if (i < 1 || i > g_nn || g_nh[i - 1].fd < 0) zs_die("%s: niepoprawny uchwyt sieciowy %lld", f, (long long)i);
    return &g_nh[i - 1];
}
static int net_addr(const char *host, int64_t port, int socktype, struct sockaddr_storage *out, socklen_t *len) {
    struct addrinfo hints, *res; memset(&hints, 0, sizeof hints); hints.ai_socktype = socktype; hints.ai_family = AF_UNSPEC;
    char ps[16]; snprintf(ps, sizeof ps, "%lld", (long long)port);
    int rc = getaddrinfo(host, ps, &hints, &res);
    if (rc) zs_die("%s:%lld: %s", host, (long long)port, gai_strerror(rc));
    memcpy(out, res->ai_addr, res->ai_addrlen); *len = res->ai_addrlen; freeaddrinfo(res); return 0;
}
static V sa_host(struct sockaddr_storage *sa, socklen_t len, int64_t *port) {
    char h[128] = ""; char sv[16] = ""; getnameinfo((struct sockaddr *)sa, len, h, sizeof h, sv, sizeof sv, NI_NUMERICHOST | NI_NUMERICSERV);
    *port = atoi(sv); return zv_str(h);
}
static void set_tmo(int fd, int64_t ms) {
    struct timeval tv; tv.tv_sec = (time_t)(ms > 0 ? ms / 1000 : 0); tv.tv_usec = (suseconds_t)(ms > 0 ? (ms % 1000) * 1000 : 0);
    setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof tv); setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &tv, sizeof tv);
}
V m_Socket_tcpConnect(V h, V p, V ms) {
    ZStr *host = need_s(h, "Socket.tcpConnect", 1); int64_t port = need_i(p, "Socket.tcpConnect", 2), tmo = need_i(ms, "Socket.tcpConnect", 3);
    struct sockaddr_storage sa; socklen_t sl; net_addr(host->d, port, SOCK_STREAM, &sa, &sl);
    int fd = socket(sa.ss_family, SOCK_STREAM, 0); if (fd < 0) zs_die("%s:%lld: %s", host->d, (long long)port, strerror(errno));
    int rc;
    if (tmo > 0) {
        int fl = fcntl(fd, F_GETFL, 0); fcntl(fd, F_SETFL, fl | O_NONBLOCK);
        rc = connect(fd, (struct sockaddr *)&sa, sl);
        if (rc && errno == EINPROGRESS) {
            struct pollfd pf = { fd, POLLOUT, 0 }; int pr = poll(&pf, 1, (int)tmo);
            if (pr <= 0) { close(fd); zs_die("%s:%lld: %s", host->d, (long long)port, pr == 0 ? "connection timed out" : strerror(errno)); }
            int err = 0; socklen_t el = sizeof err; getsockopt(fd, SOL_SOCKET, SO_ERROR, &err, &el); rc = err ? -1 : 0; errno = err;
        }
        fcntl(fd, F_SETFL, fl);
    } else rc = connect(fd, (struct sockaddr *)&sa, sl);
    if (rc) { int e = errno; close(fd); zs_die("%s:%lld: %s", host->d, (long long)port, strerror(e)); }
    int one = 1; setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
    return zv_int(net_add(fd, 1));
}
V m_Socket_tcpListen(V h, V p) {
    ZStr *host = need_s(h, "Socket.tcpListen", 1); int64_t port = need_i(p, "Socket.tcpListen", 2);
    struct sockaddr_storage sa; socklen_t sl; net_addr(host->d, port, SOCK_STREAM, &sa, &sl);
    int fd = socket(sa.ss_family, SOCK_STREAM, 0); if (fd < 0) zs_die("%s:%lld: %s", host->d, (long long)port, strerror(errno));
    int one = 1; setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &one, sizeof one);
    if (bind(fd, (struct sockaddr *)&sa, sl) || listen(fd, 128)) { int e = errno; close(fd); zs_die("%s:%lld: %s", host->d, (long long)port, strerror(e)); }
    return zv_int(net_add(fd, 2));
}
V m_Socket_accept(V h) {
    NetH *l = net_get(h, "Socket.accept"); if (l->kind != 2) zs_die("Socket.accept: to nie jest gniazdo nasłuchujące");
    struct sockaddr_storage sa; socklen_t sl = sizeof sa; int fd;
    do { fd = accept(l->fd, (struct sockaddr *)&sa, &sl); } while (fd < 0 && errno == EINTR);
    if (fd < 0) zs_die("Socket.accept: %s", strerror(errno));
    int one = 1; setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
    int64_t port; V host = sa_host(&sa, sl, &port);
    V r = assoc_new(); assoc_set(r, "handle", zv_int(net_add(fd, 1))); assoc_set(r, "host", host); assoc_set(r, "port", zv_int(port)); return r;
}
V m_Socket_read(V h, V m) {
    NetH *x = net_get(h, "Socket.read"); int64_t max = need_i(m, "Socket.read", 2); if (max < 1) max = 1; if (max > (1 << 24)) max = 1 << 24;
    if (x->kind != 1) zs_die("Socket.read: to nie jest połączenie TCP");
    char *buf = xmalloc((size_t)max + 4); ssize_t n;
    do { n = recv(x->fd, buf, (size_t)max, 0); } while (n < 0 && errno == EINTR);
    if (n < 0) { int e = errno; free(buf); if (e == EAGAIN || e == EWOULDBLOCK) return zv_null(); zs_die("Socket.read: %s", strerror(e)); }
    /* niepełna sekwencja UTF-8 na końcu porcji: dobierz brakujące bajty, żeby znak nie został uszkodzony */
    for (int back = 1; back <= 3 && back <= n; back++) {
        unsigned char c = (unsigned char)buf[n - back];
        if ((c & 0xC0) == 0x80) continue;
        int need = c >= 0xF0 ? 4 : c >= 0xE0 ? 3 : c >= 0xC0 ? 2 : 1;
        if (need > back) { ssize_t got = recv(x->fd, buf + n, (size_t)(need - back), MSG_WAITALL); if (got > 0) n += got; }
        break;
    }
    V r = utf8_lossy(buf, n); free(buf); return r;
}
V m_Socket_write(V h, V d) {
    NetH *x = net_get(h, "Socket.write"); V t = to_text(d); if (x->kind != 1) zs_die("Socket.write: to nie jest połączenie TCP");
    int64_t off = 0; while (off < t.u.s->len) {
        ssize_t n = send(x->fd, t.u.s->d + off, (size_t)(t.u.s->len - off), MSG_NOSIGNAL);
        if (n < 0) { if (errno == EINTR) continue; zs_die("Socket.write: %s", strerror(errno)); }
        off += n;
    }
    return zv_int(t.u.s->len);
}
V m_Socket_close(V h) {
    int64_t i = need_i(h, "Socket.close", 1); if (i < 1 || i > g_nn || g_nh[i - 1].fd < 0) return zv_null();
    if (g_nh[i - 1].kind == 1) shutdown(g_nh[i - 1].fd, SHUT_RDWR);
    close(g_nh[i - 1].fd); g_nh[i - 1].fd = -1; return zv_null();
}
V m_Socket_setTimeout(V h, V ms) { NetH *x = net_get(h, "Socket.setTimeout"); if (x->kind != 2) set_tmo(x->fd, need_i(ms, "Socket.setTimeout", 2)); return zv_null(); }
V m_Socket_udpBind(V h, V p) {
    ZStr *host = need_s(h, "Socket.udpBind", 1); int64_t port = need_i(p, "Socket.udpBind", 2);
    struct sockaddr_storage sa; socklen_t sl; net_addr(host->d, port, SOCK_DGRAM, &sa, &sl);
    int fd = socket(sa.ss_family, SOCK_DGRAM, 0); if (fd < 0) zs_die("%s:%lld: %s", host->d, (long long)port, strerror(errno));
    if (bind(fd, (struct sockaddr *)&sa, sl)) { int e = errno; close(fd); zs_die("%s:%lld: %s", host->d, (long long)port, strerror(e)); }
    return zv_int(net_add(fd, 3));
}
V m_Socket_sendTo(V h, V host, V p, V d) {
    NetH *x = net_get(h, "Socket.sendTo"); ZStr *hs = need_s(host, "Socket.sendTo", 2); int64_t port = need_i(p, "Socket.sendTo", 3); V t = to_text(d);
    if (x->kind != 3) zs_die("Socket.sendTo: to nie jest gniazdo UDP");
    struct sockaddr_storage sa; socklen_t sl; net_addr(hs->d, port, SOCK_DGRAM, &sa, &sl);
    ssize_t n = sendto(x->fd, t.u.s->d, (size_t)t.u.s->len, 0, (struct sockaddr *)&sa, sl);
    if (n < 0) zs_die("Socket.sendTo: %s", strerror(errno));
    return zv_int(n);
}
V m_Socket_recvFrom(V h, V m) {
    NetH *x = net_get(h, "Socket.recvFrom"); int64_t max = need_i(m, "Socket.recvFrom", 2); if (max < 1) max = 1; if (max > (1 << 20)) max = 1 << 20;
    if (x->kind != 3) zs_die("Socket.recvFrom: to nie jest gniazdo UDP");
    char *buf = xmalloc((size_t)max); struct sockaddr_storage sa; socklen_t sl = sizeof sa; ssize_t n;
    do { n = recvfrom(x->fd, buf, (size_t)max, 0, (struct sockaddr *)&sa, &sl); } while (n < 0 && errno == EINTR);
    if (n < 0) { int e = errno; free(buf); if (e == EAGAIN || e == EWOULDBLOCK) return zv_null(); zs_die("Socket.recvFrom: %s", strerror(e)); }
    int64_t port; V host = sa_host(&sa, sl, &port); V r = assoc_new();
    assoc_set(r, "data", utf8_lossy(buf, n)); assoc_set(r, "host", host); assoc_set(r, "port", zv_int(port)); free(buf); return r;
}
V m_Socket_resolve(V h) {
    ZStr *host = need_s(h, "Socket.resolve", 1); struct addrinfo hints, *res, *q; memset(&hints, 0, sizeof hints); hints.ai_socktype = SOCK_STREAM;
    int rc = getaddrinfo(host->d, NULL, &hints, &res); if (rc) zs_die("%s: %s", host->d, gai_strerror(rc));
    V r = zv_arr_new(4);
    for (q = res; q; q = q->ai_next) { int64_t port; V s = sa_host((struct sockaddr_storage *)q->ai_addr, q->ai_addrlen, &port); zv_append(r, s); }
    freeaddrinfo(res); return r;
}
static V net_name(V h, int peer, const char *f) {
    NetH *x = net_get(h, f); struct sockaddr_storage sa; socklen_t sl = sizeof sa;
    int rc = peer ? getpeername(x->fd, (struct sockaddr *)&sa, &sl) : getsockname(x->fd, (struct sockaddr *)&sa, &sl);
    if (rc) zs_die("%s: %s", f, strerror(errno));
    int64_t port; V host = sa_host(&sa, sl, &port); V r = assoc_new(); assoc_set(r, "host", host); assoc_set(r, "port", zv_int(port)); return r;
}
V m_Socket_localAddr(V h) { return net_name(h, 0, "Socket.localAddr"); }
V m_Socket_peerAddr(V h) { return net_name(h, 1, "Socket.peerAddr"); }

V m_System_bytes(V s) {
    ZStr *x = need_s(s, "System.bytes", 1); V r = zv_arr_new(x->len);
    for (int64_t i = 0; i < x->len; i++) zv_append(r, zv_int((unsigned char)x->d[i]));
    return r;
}
V m_System_fromBytes(V a) {
    ZArr *x = need_a(a, "System.fromBytes", 1); char *buf = xmalloc((size_t)x->n + 1);
    for (int64_t i = 0; i < x->n; i++) {
        V e = x->vals[i]; if (e.tag != ZS_INT || e.u.i < 0 || e.u.i > 255) zs_die("System.fromBytes: elementy muszą być liczbami 0..255");
        buf[i] = (char)e.u.i;
    }
    V r = utf8_lossy(buf, x->n); free(buf); return r;
}
