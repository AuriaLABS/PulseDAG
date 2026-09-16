/*
 * Software-only host self-test for the exact shared OpenCL kHeavyHash math.
 * No OpenCL runtime or physical GPU is required or claimed by this program.
 */
#include <math.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>

#include "kheavyhash_shared.h"

typedef struct {
    pd_u64 s0;
    pd_u64 s1;
    pd_u64 s2;
    pd_u64 s3;
} pd_xoshiro256pp;

static pd_u64 pd_xoshiro_next(pd_xoshiro256pp *state) {
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

static int pd_matrix_rank(const pd_u16 matrix[64][64]) {
    const double epsilon = 1e-9;
    double values[64][64];
    bool selected[64] = {false};

    for (unsigned int row = 0; row < 64U; ++row) {
        for (unsigned int column = 0; column < 64U; ++column) {
            values[row][column] = (double)matrix[row][column];
        }
    }

    int rank = 0;
    for (unsigned int column = 0; column < 64U; ++column) {
        unsigned int row = 0;
        while (row < 64U &&
               (selected[row] || fabs(values[row][column]) <= epsilon)) {
            ++row;
        }

        if (row != 64U) {
            ++rank;
            selected[row] = true;
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

static void pd_generate_matrix(const pd_u8 pre_pow_hash[32], pd_u16 matrix[64][64]) {
    pd_xoshiro256pp generator = {
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
                    value = pd_xoshiro_next(&generator);
                }
                matrix[row][column] = (pd_u16)((value >> (4U * nibble)) & 0x0fU);
            }
        }
        if (pd_matrix_rank(matrix) == 64) {
            return;
        }
    }
}

static int pd_hex_nibble(char value) {
    if (value >= '0' && value <= '9') {
        return value - '0';
    }
    if (value >= 'a' && value <= 'f') {
        return value - 'a' + 10;
    }
    if (value >= 'A' && value <= 'F') {
        return value - 'A' + 10;
    }
    return -1;
}

static bool pd_decode_hash(const char *hex, pd_u8 output[32]) {
    if (strlen(hex) != 64U) {
        return false;
    }
    for (unsigned int index = 0; index < 32U; ++index) {
        const int high = pd_hex_nibble(hex[2U * index]);
        const int low = pd_hex_nibble(hex[2U * index + 1U]);
        if (high < 0 || low < 0) {
            return false;
        }
        output[index] = (pd_u8)((high << 4) | low);
    }
    return true;
}

static bool pd_check_vector(
    const char *id,
    const char *pre_pow_hash_hex,
    pd_u64 nonce,
    const char *expected_hash_hex
) {
    pd_u8 pre_pow_hash[32];
    pd_u8 expected[32];
    pd_u8 actual[32];
    pd_u16 matrix[64][64];

    if (!pd_decode_hash(pre_pow_hash_hex, pre_pow_hash) ||
        !pd_decode_hash(expected_hash_hex, expected)) {
        fprintf(stderr, "invalid OpenCL self-test vector: %s\n", id);
        return false;
    }

    pd_generate_matrix(pre_pow_hash, matrix);
    pd_kheavyhash_from_pre_pow_hash(pre_pow_hash, &matrix[0][0], nonce, actual);
    if (memcmp(actual, expected, sizeof(actual)) != 0) {
        fprintf(stderr, "OpenCL shared-math vector mismatch: %s\n", id);
        return false;
    }

    printf("opencl_shared_math_vector=%s status=PASS\n", id);
    return true;
}

int main(void) {
    const bool genesis = pd_check_vector(
        "genesis-like-low-difficulty",
        "365bb76db67b8339d418038e1db2001c4d9866571249978d324f166dd78a4f1f",
        PD_U64_C(0),
        "9a3fee6769bdee0013a87b867df1e4d1f8775f3ef4efccc9d5d40452c0d104e5");
    const bool single_parent = pd_check_vector(
        "single-parent-mid-difficulty",
        "4437c1c239d16779fab0d4a03109504f7ccbddc494fc7db32662993b35cb5e1a",
        PD_U64_C(42),
        "3574f4ae3c17028c7d6997b3fc0a634be1f902ccdb9df13a9cadddc887c7ccab");

    if (!(genesis && single_parent)) {
        return 1;
    }
    puts("task38_amd_opencl_shared_math_vectors=2/2 PASS");
    puts("amd_opencl_hardware_execution=NOT_CLAIMED");
    puts("GPU_MINING_AMD_PASS=NOT_CLAIMED");
    return 0;
}
