/*
 * Software-only canonical matrix vector for Task 38 Phase 10.
 *
 * This deliberately includes the exact OpenCL matrix source as C so the
 * XoShiRo256++/rank implementation used by the runtime is exercised directly
 * without requiring a physical GPU or OpenCL ICD.
 */
#include <math.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#define cl_khr_fp64 1
#define __kernel
#define __global
typedef unsigned char uchar;

static size_t get_global_id(unsigned int dimension) {
    (void)dimension;
    return 0;
}

#include "kheavyhash_matrix.cl"

static uint64_t matrix_fingerprint(const pd_u16 matrix[64][64]) {
    uint64_t hash = UINT64_C(14695981039346656037);
    for (unsigned int row = 0; row < 64U; ++row) {
        for (unsigned int column = 0; column < 64U; ++column) {
            hash ^= (uint64_t)matrix[row][column];
            hash *= UINT64_C(1099511628211);
        }
    }
    return hash;
}

int main(void) {
    pd_u8 pre_pow_hash[32];
    for (unsigned int index = 0; index < 32U; ++index) {
        pre_pow_hash[index] = (pd_u8)42U;
    }

    pd_u16 matrix[64][64];
    pd_generate_matrix(pre_pow_hash, matrix);

    if (pd_matrix_rank(matrix) != 64) {
        fprintf(stderr, "canonical matrix rank mismatch\n");
        return 1;
    }

    if (matrix[0][0] != 4U || matrix[0][1] != 5U || matrix[17][0] != 13U ||
        matrix[32][32] != 14U || matrix[63][63] != 14U) {
        fprintf(stderr, "canonical matrix sample mismatch\n");
        return 1;
    }

    const uint64_t fingerprint = matrix_fingerprint(matrix);
    if (fingerprint != UINT64_C(0x4415042fae3636b5)) {
        fprintf(stderr, "canonical matrix fingerprint mismatch: 0x%016llx\n",
                (unsigned long long)fingerprint);
        return 1;
    }

    pd_u16 kernel_output[64U * 64U] = {0};
    pulsedag_generate_matrix_kernel(pre_pow_hash, kernel_output);
    for (unsigned int row = 0; row < 64U; ++row) {
        for (unsigned int column = 0; column < 64U; ++column) {
            if (kernel_output[row * 64U + column] != matrix[row][column]) {
                fprintf(stderr, "matrix kernel entrypoint mismatch at %u,%u\n", row, column);
                return 1;
            }
        }
    }

    puts("task38_amd_opencl_matrix_vector=PASS");
    return 0;
}
