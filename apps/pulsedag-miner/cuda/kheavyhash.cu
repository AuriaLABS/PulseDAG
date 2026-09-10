/*
 * PulseDAG CUDA kHeavyHash equivalence foundation.
 *
 * Portions of the matrix generator, XoShiRo256++ transition, PowHash and
 * KHeavyHash construction are derived from rusty-kaspa revision
 * cfafeb4c093fa37a303f1b9f19c58f986b870ce3.
 *
 * ISC License
 *
 * Copyright (c) 2022-2024 Kaspa developers
 *
 * Permission to use, copy, modify, and distribute this software for any
 * purpose with or without fee is hereby granted, provided that the above
 * copyright notice and this permission notice appear in all copies.
 *
 * THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
 * WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
 * MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
 * ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
 * WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
 * ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
 * OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
 */

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cmath>

#ifdef __CUDACC__
#define PULSEDAG_HD __host__ __device__
#else
#define PULSEDAG_HD
#endif

namespace pulsedag_cuda {

PULSEDAG_HD static inline std::uint64_t rotl64(std::uint64_t value, unsigned shift) {
    return (value << shift) | (value >> (64U - shift));
}

PULSEDAG_HD static inline std::uint64_t load64_le(const std::uint8_t* bytes) {
    return static_cast<std::uint64_t>(bytes[0]) |
           (static_cast<std::uint64_t>(bytes[1]) << 8U) |
           (static_cast<std::uint64_t>(bytes[2]) << 16U) |
           (static_cast<std::uint64_t>(bytes[3]) << 24U) |
           (static_cast<std::uint64_t>(bytes[4]) << 32U) |
           (static_cast<std::uint64_t>(bytes[5]) << 40U) |
           (static_cast<std::uint64_t>(bytes[6]) << 48U) |
           (static_cast<std::uint64_t>(bytes[7]) << 56U);
}

PULSEDAG_HD static inline void store64_le(std::uint8_t* bytes, std::uint64_t value) {
    for (unsigned index = 0; index < 8U; ++index) {
        bytes[index] = static_cast<std::uint8_t>(value >> (8U * index));
    }
}

PULSEDAG_HD static inline void keccak_f1600(std::uint64_t state[25]) {
    const std::uint64_t round_constants[24] = {
        0x0000000000000001ULL, 0x0000000000008082ULL, 0x800000000000808aULL,
        0x8000000080008000ULL, 0x000000000000808bULL, 0x0000000080000001ULL,
        0x8000000080008081ULL, 0x8000000000008009ULL, 0x000000000000008aULL,
        0x0000000000000088ULL, 0x0000000080008009ULL, 0x000000008000000aULL,
        0x000000008000808bULL, 0x800000000000008bULL, 0x8000000000008089ULL,
        0x8000000000008003ULL, 0x8000000000008002ULL, 0x8000000000000080ULL,
        0x000000000000800aULL, 0x800000008000000aULL, 0x8000000080008081ULL,
        0x8000000000008080ULL, 0x0000000080000001ULL, 0x8000000080008008ULL,
    };
    const unsigned rotation_constants[24] = {
        1U, 3U, 6U, 10U, 15U, 21U, 28U, 36U, 45U, 55U, 2U, 14U,
        27U, 41U, 56U, 8U, 25U, 43U, 62U, 18U, 39U, 61U, 20U, 44U,
    };
    const unsigned pi_lanes[24] = {
        10U, 7U, 11U, 17U, 18U, 3U, 5U, 16U, 8U, 21U, 24U, 4U,
        15U, 23U, 19U, 13U, 12U, 2U, 20U, 14U, 22U, 9U, 6U, 1U,
    };

    std::uint64_t columns[5];
    for (unsigned round = 0; round < 24U; ++round) {
        for (unsigned x = 0; x < 5U; ++x) {
            columns[x] = state[x] ^ state[x + 5U] ^ state[x + 10U] ^
                         state[x + 15U] ^ state[x + 20U];
        }
        for (unsigned x = 0; x < 5U; ++x) {
            const std::uint64_t theta =
                columns[(x + 4U) % 5U] ^ rotl64(columns[(x + 1U) % 5U], 1U);
            for (unsigned y = 0; y < 25U; y += 5U) {
                state[y + x] ^= theta;
            }
        }

        std::uint64_t carried = state[1];
        for (unsigned index = 0; index < 24U; ++index) {
            const unsigned lane = pi_lanes[index];
            const std::uint64_t next = state[lane];
            state[lane] = rotl64(carried, rotation_constants[index]);
            carried = next;
        }

        for (unsigned row = 0; row < 25U; row += 5U) {
            for (unsigned x = 0; x < 5U; ++x) {
                columns[x] = state[row + x];
            }
            for (unsigned x = 0; x < 5U; ++x) {
                state[row + x] ^=
                    (~columns[(x + 1U) % 5U]) & columns[(x + 2U) % 5U];
            }
        }
        state[0] ^= round_constants[round];
    }
}

struct Xoshiro256PlusPlus {
    std::uint64_t s0;
    std::uint64_t s1;
    std::uint64_t s2;
    std::uint64_t s3;

    std::uint64_t next() {
        const std::uint64_t result = s0 + rotl64(s0 + s3, 23U);
        const std::uint64_t shifted = s1 << 17U;
        s2 ^= s0;
        s3 ^= s1;
        s1 ^= s2;
        s0 ^= s3;
        s2 ^= shifted;
        s3 = rotl64(s3, 45U);
        return result;
    }
};

static int matrix_rank(const std::uint16_t matrix[64][64]) {
    constexpr double epsilon = 1e-9;
    double values[64][64];
    bool selected[64] = {};

    for (unsigned row = 0; row < 64U; ++row) {
        for (unsigned column = 0; column < 64U; ++column) {
            values[row][column] = static_cast<double>(matrix[row][column]);
        }
    }

    int rank = 0;
    for (unsigned column = 0; column < 64U; ++column) {
        unsigned row = 0;
        while (row < 64U &&
               (selected[row] || std::fabs(values[row][column]) <= epsilon)) {
            ++row;
        }

        if (row != 64U) {
            ++rank;
            selected[row] = true;
            for (unsigned trailing = column + 1U; trailing < 64U; ++trailing) {
                values[row][trailing] /= values[row][column];
            }
            for (unsigned other = 0; other < 64U; ++other) {
                if (other != row &&
                    std::fabs(values[other][column]) > epsilon) {
                    for (unsigned trailing = column + 1U; trailing < 64U; ++trailing) {
                        values[other][trailing] -=
                            values[row][trailing] * values[other][column];
                    }
                }
            }
        }
    }
    return rank;
}

static void generate_matrix(
    const std::uint8_t pre_pow_hash[32],
    std::uint16_t matrix[64][64]
) {
    Xoshiro256PlusPlus generator{
        load64_le(pre_pow_hash),
        load64_le(pre_pow_hash + 8),
        load64_le(pre_pow_hash + 16),
        load64_le(pre_pow_hash + 24),
    };

    for (;;) {
        for (unsigned row = 0; row < 64U; ++row) {
            std::uint64_t value = 0;
            for (unsigned column = 0; column < 64U; ++column) {
                const unsigned nibble = column % 16U;
                if (nibble == 0U) {
                    value = generator.next();
                }
                matrix[row][column] =
                    static_cast<std::uint16_t>((value >> (4U * nibble)) & 0x0fU);
            }
        }
        if (matrix_rank(matrix) == 64) {
            return;
        }
    }
}

PULSEDAG_HD static inline void pow_hash_with_nonce(
    const std::uint8_t pre_pow_hash[32],
    std::uint64_t nonce,
    std::uint8_t output[32]
) {
    const std::uint64_t initial_state[25] = {
        1242148031264380989ULL, 3008272977830772284ULL, 2188519011337848018ULL,
        1992179434288343456ULL, 8876506674959887717ULL, 5399642050693751366ULL,
        1745875063082670864ULL, 8605242046444978844ULL, 17936695144567157056ULL,
        3343109343542796272ULL, 1123092876221303306ULL, 4963925045340115282ULL,
        17037383077651887893ULL, 16629644495023626889ULL, 12833675776649114147ULL,
        3784524041015224902ULL, 1082795874807940378ULL, 13952716920571277634ULL,
        13411128033953605860ULL, 15060696040649351053ULL, 9928834659948351306ULL,
        5237849264682708699ULL, 12825353012139217522ULL, 6706187291358897596ULL,
        196324915476054915ULL,
    };
    std::uint64_t state[25];
    for (unsigned index = 0; index < 25U; ++index) {
        state[index] = initial_state[index];
    }
    for (unsigned index = 0; index < 4U; ++index) {
        state[index] ^= load64_le(pre_pow_hash + 8U * index);
    }

    // PulseDAG's canonical adapter calls PowHash::new(pre_pow_hash, 0).
    state[9] ^= nonce;
    keccak_f1600(state);
    for (unsigned index = 0; index < 4U; ++index) {
        store64_le(output + 8U * index, state[index]);
    }
}

PULSEDAG_HD static inline void heavy_hash(
    const std::uint16_t* matrix,
    const std::uint8_t initial_hash[32],
    std::uint8_t output[32]
) {
    std::uint8_t vector[64];
    for (unsigned index = 0; index < 32U; ++index) {
        vector[2U * index] = initial_hash[index] >> 4U;
        vector[2U * index + 1U] = initial_hash[index] & 0x0fU;
    }

    std::uint8_t product[32];
    for (unsigned index = 0; index < 32U; ++index) {
        std::uint16_t high_sum = 0;
        std::uint16_t low_sum = 0;
        for (unsigned component = 0; component < 64U; ++component) {
            high_sum = static_cast<std::uint16_t>(
                high_sum + matrix[(2U * index) * 64U + component] *
                               static_cast<std::uint16_t>(vector[component]));
            low_sum = static_cast<std::uint16_t>(
                low_sum + matrix[(2U * index + 1U) * 64U + component] *
                              static_cast<std::uint16_t>(vector[component]));
        }
        product[index] = static_cast<std::uint8_t>(
            ((high_sum >> 10U) << 4U) | (low_sum >> 10U));
        product[index] ^= initial_hash[index];
    }

    const std::uint64_t initial_state[25] = {
        4239941492252378377ULL, 8746723911537738262ULL, 8796936657246353646ULL,
        1272090201925444760ULL, 16654558671554924250ULL, 8270816933120786537ULL,
        13907396207649043898ULL, 6782861118970774626ULL, 9239690602118867528ULL,
        11582319943599406348ULL, 17596056728278508070ULL, 15212962468105129023ULL,
        7812475424661425213ULL, 3370482334374859748ULL, 5690099369266491460ULL,
        8596393687355028144ULL, 570094237299545110ULL, 9119540418498120711ULL,
        16901969272480492857ULL, 13372017233735502424ULL, 14372891883993151831ULL,
        5171152063242093102ULL, 10573107899694386186ULL, 6096431547456407061ULL,
        1592359455985097269ULL,
    };
    std::uint64_t state[25];
    for (unsigned index = 0; index < 25U; ++index) {
        state[index] = initial_state[index];
    }
    for (unsigned index = 0; index < 4U; ++index) {
        state[index] ^= load64_le(product + 8U * index);
    }
    keccak_f1600(state);
    for (unsigned index = 0; index < 4U; ++index) {
        store64_le(output + 8U * index, state[index]);
    }
}

PULSEDAG_HD static inline void kheavyhash_from_pre_pow_hash(
    const std::uint8_t pre_pow_hash[32],
    const std::uint16_t* matrix,
    std::uint64_t nonce,
    std::uint8_t output[32]
) {
    std::uint8_t initial_hash[32];
    pow_hash_with_nonce(pre_pow_hash, nonce, initial_hash);
    heavy_hash(matrix, initial_hash, output);
}

#ifdef __CUDACC__
extern "C" __global__ void pulsedag_kheavyhash_kernel(
    const std::uint8_t* pre_pow_hash,
    const std::uint16_t* matrix,
    const std::uint64_t* nonces,
    std::uint8_t* outputs,
    std::uint64_t count
) {
    const std::uint64_t index =
        static_cast<std::uint64_t>(blockIdx.x) * blockDim.x + threadIdx.x;
    if (index >= count) {
        return;
    }
    kheavyhash_from_pre_pow_hash(
        pre_pow_hash,
        matrix,
        nonces[index],
        outputs + index * 32U);
}
#endif

#ifdef PULSEDAG_CUDA_HOST_SELF_TEST
static int hex_nibble(char value) {
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

static bool decode_hash(const char* hex, std::uint8_t output[32]) {
    for (unsigned index = 0; index < 32U; ++index) {
        const int high = hex_nibble(hex[2U * index]);
        const int low = hex_nibble(hex[2U * index + 1U]);
        if (high < 0 || low < 0) {
            return false;
        }
        output[index] = static_cast<std::uint8_t>((high << 4) | low);
    }
    return true;
}

static bool check_vector(
    const char* id,
    const char* pre_pow_hash_hex,
    std::uint64_t nonce,
    const char* expected_hash_hex
) {
    std::uint8_t pre_pow_hash[32];
    std::uint8_t expected[32];
    std::uint8_t actual[32];
    std::uint16_t matrix[64][64];

    if (!decode_hash(pre_pow_hash_hex, pre_pow_hash) ||
        !decode_hash(expected_hash_hex, expected)) {
        std::fprintf(stderr, "invalid self-test vector: %s\n", id);
        return false;
    }

    generate_matrix(pre_pow_hash, matrix);
    kheavyhash_from_pre_pow_hash(pre_pow_hash, &matrix[0][0], nonce, actual);
    if (std::memcmp(actual, expected, sizeof(actual)) != 0) {
        std::fprintf(stderr, "CUDA shared-math vector mismatch: %s\n", id);
        return false;
    }

    std::printf("cuda_shared_math_vector=%s status=PASS\n", id);
    return true;
}
#endif

}  // namespace pulsedag_cuda

#ifdef PULSEDAG_CUDA_HOST_SELF_TEST
int main() {
    // pre_pow_hash values are Keccak-256 of each canonical preimage in the
    // repository's official PoW vectors. The final hashes are the canonical
    // CPU kHeavyHash outputs for the matching nonce.
    const bool genesis = pulsedag_cuda::check_vector(
        "genesis-like-low-difficulty",
        "365bb76db67b8339d418038e1db2001c4d9866571249978d324f166dd78a4f1f",
        0,
        "9a3fee6769bdee0013a87b867df1e4d1f8775f3ef4efccc9d5d40452c0d104e5");
    const bool single_parent = pulsedag_cuda::check_vector(
        "single-parent-mid-difficulty",
        "4437c1c239d16779fab0d4a03109504f7ccbddc494fc7db32662993b35cb5e1a",
        42,
        "3574f4ae3c17028c7d6997b3fc0a634be1f902ccdb9df13a9cadddc887c7ccab");

    if (!(genesis && single_parent)) {
        return 1;
    }
    std::puts("task38_nvidia_cuda_shared_math_vectors=2/2 PASS");
    return 0;
}
#endif
