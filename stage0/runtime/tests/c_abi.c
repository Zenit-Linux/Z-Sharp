#include <stdio.h>
#include <string.h>
#include "../include/zs_runtime.h"
static int closed = 0;
static void on_close(void *c) { (void)c; closed++; }
int main(void) {
    ZRuntime *rt = zs_runtime_new(0, 0);
    zs_frame_push(rt);
    ZValue s; zs_str_new(rt, ZS_FRAME, (const uint8_t *)"hej", 3, &s);
    ZValue arr; zs_array_new(rt, ZS_FRAME, 4, &arr);
    for (int i = 0; i < 100; i++) { ZValue v = { ZS_INT, (uint64_t)i }; if (zs_array_push(rt, ZS_FRAME, arr, v)) return 1; }
    ZValue last; if (zs_array_get(arr, 99, &last) || last.bits != 99) return 2;
    uint64_t h; zs_resource_new(rt, ZS_FRAME, NULL, on_close, &h);
    if (zs_frame_pop(rt) != ZS_OK || closed != 1) return 3;
    ZMemStats st; zs_mem_stats(rt, &st);
    printf("c_abi ok: str=%lld arr=%lld allocs=%llu\n", (long long)zs_str_len(s), (long long)zs_array_len(arr), (unsigned long long)st.allocs);
    zs_runtime_free(rt);
    return 0;
}
