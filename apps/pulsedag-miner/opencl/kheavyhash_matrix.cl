/*
 * PulseDAG canonical OpenCL kHeavyHash matrix generation.
 *
 * This mirrors the Matrix::generate/XoShiRo256++ path pinned by pulsedag-core
 * to rusty-kaspa cfafeb4c093fa37a303f1b9f19c58f986b870ce3.
 * The upstream rank test is f64 with EPS=1e-9, so this kernel deliberately
 * requires cl_khr_fp64 rather than silently changing rank semantics.
 */
#ifndef PULSEDAG_OPENCL_EMBEDDED_SHARED
#include "kheavyhash_shared.h"
#endif

#ifndef cl_khr_fp64
#error "PulseDAG canonical OpenCL matrix generation requires cl_khr_fp64"
#endif
#pragma OPENCL EXTENSION cl_khr_fp64 : enable

typedef struct {
    pd_u64 s0;
    pd_u64 s1;
    pd_u64 s2;
    pd_u64 s3;
} pd_matrix_xoshiro256pp;

static inline pd_u64 pd_matrix_xoshiro_next(PD_PRIVATE pd_matrix_xoshiro256pp *state) {
    const pd_u64 result = state->s0 + pd_rotl64(state->s0 + state->s3, 23U);
    const pd_u64 shifted = state->s1 << 17U;
    state->s2 ^= state->s0;
    state->s3 ^= state->s1;
    state->s1 ^= state->s2;
    state->s0 ^= state->s3;
    state->s2 ^= shifted;
    state->s3 = pd_rotl64(state->s3, 45U);
    return result;
}

static inline int pd_matrix_rank(PD_PRIVATE const pd_u16 matrix[64][64]) {
    const double epsilon = 1.0e-9;
    double values[64][64];
    uchar selected[64];

    for (unsigned int row = 0; row < 64U; ++row) {
        selected[row] = (uchar)0;
        for (unsigned int column = 0; column < 64U; ++column) {
            values[row][column] = (double)matrix[row][column];
        }
    }

    int rank = 0;
    for (unsigned int column = 0; column < 64U; ++column) {
        unsigned int row = 0;
        while (row < 64U &&
               (selected[row] != (uchar)0 || fabs(values[row][column]) <= epsilon)) {
            ++row;
        }
        if (row != 64U) {
            ++rank;
            selected[row] = (uchar)1;
            for (unsigned int trailing = column + 1U; trailing < 64U; ++trailing) {
                values[row][trailing] /= values[row][column];
            }
            for (unsigned int other = 0; other < 64U; ++other) {
                if (other != row && fabs(values[other][column]) > epsilon) {
                    for (unsigned int trailing = column + 1U; trailing < 64U; ++trailing) {
                        values[other][trailing] -=
                            values[row][trailing] * values[other][column];
                    }
                }
            }
        }
    }
    return rank;
}

static inline void pd_generate_matrix(
    PD_GLOBAL const pd_u8 pre_pow_hash[32],
    PD_PRIVATE pd_u16 matrix[64][64]
) {
    pd_matrix_xoshiro256pp generator = {
        pd_load64_le_global(pre_pow_hash),
        pd_load64_le_global(pre_pow_hash + 8),
        pd_load64_le_global(pre_pow_hash + 16),
        pd_load64_le_global(pre_pow_hash + 24),
    };

    for (;;) {
        for (unsigned int row = 0; row < 64U; ++row) {
            pd_u64 value = 0;
            for (unsigned int column = 0; column < 64U; ++column) {
                const unsigned int nibble = column % 16U;
                if (nibble == 0U) {
                    value = pd_matrix_xoshiro_next(&generator);
                }
                matrix[row][column] =
                    (pd_u16)((value >> (4U * nibble)) & PD_U64_C(0x0f));
            }
        }
        if (pd_matrix_rank(matrix) == 64) {
            return;
        }
    }
}

__kernel void pulsedag_generate_matrix_kernel(
    __global const pd_u8 *pre_pow_hash,
    __global pd_u16 *matrix_output
) {
    if (get_global_id(0) != 0U) {
        return;
    }

    pd_u16 matrix[64][64];
    pd_generate_matrix(pre_pow_hash, matrix);
    for (unsigned int row = 0; row < 64U; ++row) {
        for (unsigned int column = 0; column < 64U; ++column) {
            matrix_output[row * 64U + column] = matrix[row][column];
        }
    }
}
