// Minimal rocRAND repro: create + generate on the default pseudo generator.
// Build: hipcc repro-rocrand.c -lrocrand -o repro-rocrand
#include <hip/hip_runtime.h>
#include <rocrand/rocrand.h>
#include <stdio.h>

int main(void) {
    float *d = NULL;
    if (hipMalloc((void **)&d, 64 * sizeof(float)) != hipSuccess) {
        printf("hipMalloc failed\n");
        return 1;
    }
    rocrand_generator gen;
    rocrand_status s = rocrand_create_generator(&gen, ROCRAND_RNG_PSEUDO_DEFAULT);
    printf("create_generator: %d\n", (int)s);
    s = rocrand_generate_uniform(gen, d, 64);
    printf("generate_uniform: %d\n", (int)s);
    return 0;
}
