#ifndef ZS_NATIVE_H
#define ZS_NATIVE_H
#include <stdint.h>
#include <stddef.h>
#include <setjmp.h>
#include <stdio.h>

enum { ZS_NULL = 0, ZS_BOOL = 1, ZS_INT = 2, ZS_FLOAT = 3, ZS_STR = 4, ZS_ARRAY = 5, ZS_OBJ = 6, ZS_EXC = 7, ZS_FN = 8 };

typedef struct ZStr ZStr;
typedef struct ZArr ZArr;
typedef struct ZExc ZExc;

typedef struct V {
    int64_t tag;
    union { int64_t i; double f; ZStr *s; ZArr *a; ZExc *x; void *p; } u;
} V;

struct ZStr { int64_t len; char d[]; };
struct ZExc { int kind; /* 0 Exception, 1 Error */ V msg; int64_t line; };

typedef struct ZsHandler { struct ZsHandler *prev; jmp_buf jb; } ZsHandler;

extern ZsHandler *zs_h;
extern V zs_cur_exc;
extern int64_t zs_line;
#define L(n) (zs_line = (n))

/* ── konstruktory / dostęp ── */
V zv_null(void);
V zv_bool(int b);
V zv_int(int64_t i);
V zv_float(double f);
V zv_strn(const char *p, int64_t n);
V zv_str(const char *p);
int64_t zv_i(V v);
int zv_truthy(V v);
const char *zv_tn(V v);

/* ── tablice ── */
V zv_arr_new(int64_t cap);
int64_t zs_array_len(V a);
V zv_dup(V v);                      /* semantyka wartości: współdzielenie + kopiowanie przy zapisie */
V zv_wr(V v);                       /* tablica gotowa do zapisu (null -> [], rc>1 -> kopia) */
void zv_append(V a, V v);
void zv_set(V a, V k, V v);
V zv_index(V a, V k);               /* błąd gdy brak klucza */
V zv_index_opt(V a, V k);           /* null gdy brak (dla ??) */
V zv_get_raw(V a, int64_t i);
V zv_key_at(V a, int64_t i);

/* ── operatory ── */
V zv_add(V a, V b); V zv_sub(V a, V b); V zv_mul(V a, V b); V zv_div(V a, V b);
V zv_mod(V a, V b); V zv_pow(V a, V b); V zv_concat(V a, V b); V zv_neg(V a); V zv_bitnot(V a);
V zv_bitand(V a, V b); V zv_bitor(V a, V b); V zv_bitxor(V a, V b); V zv_shl(V a, V b); V zv_shr(V a, V b);
int zv_eq(V a, V b); int zv_ident(V a, V b); int zv_cmp(V a, V b);
V zv_chk(V v, const char *type, const char *what);

/* ── wyjątki ── */
V zv_exc_new(int kind, V msg);
void zs_raise(V exc) __attribute__((noreturn));
void zs_reraise(V exc) __attribute__((noreturn));
void zs_die(const char *fmt, ...) __attribute__((noreturn, format(printf, 1, 2)));
int zv_exc_is(V x, int kind);       /* kind: 0 = Exception/Throwable (każdy), 1 = Error */
V b_getmessage(V x);
V b_zs_obj_new(V); V b_zs_obj_class(V); V b_zs_obj_get(V, V); V b_zs_obj_take(V, V); V b_zs_obj_set(V, V, V); V b_zs_obj_put(V, V, V); V b_zs_no_method(V, V); V b_zs_no_class(V);
V b_throw_v(V x);

/* ── wyjście ── */
void zs_echo_v(V v, int first);  void zs_echo_end(void);
void zs_necho_v(V v);
void zs_eecho_v(V v, int first); void zs_eecho_end(void);
V s_exit(V code) __attribute__((noreturn));
void zs_init(int argc, char **argv);
void zs_shutdown(void);

/* ── wbudowane ── */
V b_count(V); V b_strlen(V); V b_typeof(V); V b_is_int(V); V b_is_float(V); V b_is_string(V); V b_is_bool(V);
V b_is_array(V); V b_is_null(V); V b_strval(V); V b_boolval(V); V b_intval(V); V b_floatval(V); V b_ord(V); V b_chr(V);
V b_contains(V, V); V b_starts_with(V, V); V b_ends_with(V, V); V b_index_of(V, V); V b_repeat(V, V);
V b_substr(V s, V a, V l, int has_len);
V b_trim(V); V b_join(V, V); V b_in_array(V, V); V b_range(V, V); V b_keys(V); V b_values(V); V b_has_key(V, V);
V b_upper(V); V b_lower(V); V b_replace(V, V, V); V b_split(V, V); V b_chars(V); V b_pad_left(V, V); V b_pad_right(V, V);
V b_pad_left3(V, V, V); V b_pad_right3(V, V, V);
V b_array_slice(V, V); V b_array_slice3(V, V, V); V b_reverse(V); V b_sort(V); V b_push(V, V); V b_dump(V);
V b_is_digit(V); V b_is_alpha(V); V b_is_alnum(V); V b_is_space(V); V b_is_hex_digit(V); V b_to_hex(V);
V b_read_line(void);
/* M3: wartości funkcyjne (domknięcia) i statyczne pola klas */
V b_zs_mkfn(V id, V cap); V b_zs_fn_id(V f); V b_zs_fn_cap(V f); V b_is_callable(V f);
V b_zs_sget(V name); V b_zs_sset(V name, V v);

/* moduły: Math / System / File / Directory / Process */
V m_Math_sqrt(V); V m_Math_floor(V); V m_Math_ceil(V); V m_Math_round(V); V m_Math_abs(V); V m_Math_min(V, V); V m_Math_max(V, V); V m_Math_pow(V, V);
V m_System_args(void); V m_System_env(V); V m_System_os(void); V m_System_architecture(void); V m_System_time(void);
V m_System_resolveImport(V, V); V m_System_cpuCount(void); V m_System_dirname(V); V m_System_version(void);
V m_File_read(V); V m_File_write(V, V); V m_File_append(V, V); V m_File_exists(V); V m_File_delete(V);
V m_Directory_create(V); V m_Directory_exists(V); V m_Directory_list(V);
V m_Process_run(V, V); V m_Process_exec(V, V);
/* rozszerzenia systemowe dla Std.Os / Std.Time / Std.Net (zgodne z stage0/zs-interp/src/sysmods.rs) */
V m_System_setEnv(V, V); V m_System_unsetEnv(V); V m_System_environ(void); V m_System_cwd(void); V m_System_chdir(V); V m_System_pid(void);
V m_System_bytes(V); V m_System_fromBytes(V); V m_System_hostname(void); V m_System_nowNanos(void); V m_System_monotonic(void); V m_System_sleep(V); V m_System_localOffset(V);
V m_File_stat(V); V m_File_rename(V, V); V m_File_realpath(V); V m_File_chmod(V, V); V m_File_symlink(V, V); V m_File_readlink(V);
V m_Directory_remove(V);
V m_Socket_tcpConnect(V, V, V); V m_Socket_tcpListen(V, V); V m_Socket_accept(V); V m_Socket_read(V, V); V m_Socket_write(V, V); V m_Socket_close(V);
V m_Socket_setTimeout(V, V); V m_Socket_udpBind(V, V); V m_Socket_sendTo(V, V, V, V); V m_Socket_recvFrom(V, V); V m_Socket_resolve(V);
V m_Socket_localAddr(V); V m_Socket_peerAddr(V);

/* pomocniki backendu LLVM */
V zl_float_lit(const char *s);
void zl_try_enter(ZsHandler *h);
void zl_try_leave(ZsHandler *h);
V zl_cur_exc(void);
void zl_foreach_die(V it) __attribute__((noreturn));

#endif
