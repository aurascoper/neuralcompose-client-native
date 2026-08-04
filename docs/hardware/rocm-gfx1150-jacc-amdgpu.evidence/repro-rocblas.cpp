// Minimal rocBLAS repro: one small SGEMM.
// Build: hipcc repro-rocblas.c -lrocblas -o repro-rocblas
#include <hip/hip_runtime.h>
#include <rocblas/rocblas.h>
#include <stdio.h>

int main(void) {
    const int n = 4;
    float *A, *B, *C;
    hipMalloc((void **)&A, n * n * sizeof(float));
    hipMalloc((void **)&B, n * n * sizeof(float));
    hipMalloc((void **)&C, n * n * sizeof(float));
    rocblas_handle h;
    rocblas_status s = rocblas_create_handle(&h);
    printf("create_handle: %d\n", (int)s);
    float alpha = 1.0f, beta = 0.0f;
    s = rocblas_sgemm(h, rocblas_operation_none, rocblas_operation_none,
                      n, n, n, &alpha, A, n, B, n, &beta, C, n);
    printf("sgemm: %d\n", (int)s);
    hipDeviceSynchronize();
    return 0;
}
