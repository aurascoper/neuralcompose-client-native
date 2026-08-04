// Minimal rocSPARSE repro: create handle + query version (what AMDGPU.jl's probe does).
// Build: hipcc repro-rocsparse.c -lrocsparse -o repro-rocsparse
#include <rocsparse/rocsparse.h>
#include <stdio.h>

int main(void) {
    rocsparse_handle h;
    rocsparse_status s = rocsparse_create_handle(&h);
    printf("create_handle: %d\n", (int)s);
    int ver = -1;
    s = rocsparse_get_version(h, &ver);
    printf("get_version: %d ver=%d\n", (int)s, ver);
    return 0;
}
