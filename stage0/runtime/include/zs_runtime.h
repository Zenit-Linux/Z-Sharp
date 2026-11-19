#ifndef ZS_RUNTIME_H
#define ZS_RUNTIME_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

enum { ZS_NULL = 0, ZS_BOOL = 1, ZS_INT = 2, ZS_FLOAT = 3, ZS_STR = 4, ZS_ARRAY = 5, ZS_OBJ = 6 };
enum { ZS_FRAME = 0, ZS_TEMP = 1, ZS_PERSISTENT = 2 };           /* zadanie: wartość zwrócona przez zs_task_new (>= 16) */
enum { ZS_OK = 0, ZS_E_OOM = 1, ZS_E_SCOPE = 2, ZS_E_REFERENCED = 3, ZS_E_STALE = 4, ZS_E_TYPE = 5, ZS_E_BOUNDS = 6, ZS_E_INTERNAL = 7 };

typedef struct ZRuntime ZRuntime;
typedef struct { uint64_t tag; uint64_t bits; } ZValue;
typedef struct {
    uint64_t frame_used, temp_used, persistent_used, task_used, arena_chunks, live_handles,
             large_live_bytes, large_live_blocks, allocs, promoted, pool_free_chunks, pool_hits, pool_misses;
} ZMemStats;

ZRuntime *zs_runtime_new(uint32_t chunk_kib, uint32_t large_kib);   /* 0 = domyślne */
void zs_runtime_free(ZRuntime *rt);

void    zs_frame_push(ZRuntime *rt);
int32_t zs_frame_pop(ZRuntime *rt);       /* ZS_E_REFERENCED, jeśli istnieją legalne odwołania — nic nie zwolniono */
void    zs_temp_begin(ZRuntime *rt);
int32_t zs_temp_end(ZRuntime *rt);
int64_t zs_task_new(ZRuntime *rt);
int32_t zs_task_end(ZRuntime *rt, int64_t task);

int32_t zs_str_new(ZRuntime *rt, int64_t life, const uint8_t *p, size_t n, ZValue *out);
int64_t zs_str_len(ZValue v);
const uint8_t *zs_str_ptr(ZValue v);
int32_t zs_str_concat(ZRuntime *rt, int64_t life, ZValue a, ZValue b, ZValue *out);

int32_t zs_array_new(ZRuntime *rt, int64_t life, size_t cap, ZValue *out);
int32_t zs_array_push(ZRuntime *rt, int64_t life, ZValue arr, ZValue v);
int64_t zs_array_len(ZValue arr);
int32_t zs_array_get(ZValue arr, size_t i, ZValue *out);
int32_t zs_array_set(ZValue arr, size_t i, ZValue v);   /* i < len; ZS_E_BOUNDS poza zakresem */

int32_t zs_resource_new(ZRuntime *rt, int64_t life, void *ctx, void (*close)(void *), uint64_t *out_handle);
int32_t zs_retain(ZRuntime *rt, uint64_t h);
int32_t zs_release(ZRuntime *rt, uint64_t h);       /* 1 = obiekt zniszczony */
int32_t zs_handle_live(ZRuntime *rt, uint64_t h);

void zs_mem_stats(ZRuntime *rt, ZMemStats *out);

#ifdef __cplusplus
}
#endif
#endif
