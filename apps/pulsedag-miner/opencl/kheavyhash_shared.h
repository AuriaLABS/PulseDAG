/*
 * PulseDAG OpenCL kHeavyHash equivalence foundation.
 *
 * Portions of PowHash and kHeavyHash construction are derived from
 * rusty-kaspa revision cfafeb4c093fa37a303f1b9f19c58f986b870ce3 and
 * translated from PulseDAG's canonical CUDA reference implementation.
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

#ifndef PULSEDAG_OPENCL_KHEAVYHASH_SHARED_H
#define PULSEDAG_OPENCL_KHEAVYHASH_SHARED_H

#if defined(__OPENCL_VERSION__) || defined(__OPENCL_C_VERSION__)
typedef uchar pd_u8;
typedef ushort pd_u16;
typedef ulong pd_u64;
#define PD_GLOBAL __global
#define PD_PRIVATE __private
#define PD_U64_C(value) value##UL
#else
#include <stdint.h>
typedef uint8_t pd_u8;
typedef uint16_t pd_u16;
typedef uint64_t pd_u64;
#define PD_GLOBAL
#define PD_PRIVATE
#define PD_U64_C(value) value##ULL
#endif

static inline pd_u64 pd_rotl64(pd_u64 value, unsigned int shift) {
    return (value << shift) | (value >> (64U - shift));
}

static inline pd_u64 pd_load64_le_global(PD_GLOBAL const pd_u8 *bytes) {
    return ((pd_u64)bytes[0]) |
           ((pd_u64)bytes[1] << 8U) |
           ((pd_u64)bytes[2] << 16U) |
           ((pd_u64)bytes[3] << 24U) |
           ((pd_u64)bytes[4] << 32U) |
           ((pd_u64)bytes[5] << 40U) |
           ((pd_u64)bytes[6] << 48U) |
           ((pd_u64)bytes[7] << 56U);
}

static inline pd_u64 pd_load64_le_private(PD_PRIVATE const pd_u8 *bytes) {
    return ((pd_u64)bytes[0]) |
           ((pd_u64)bytes[1] << 8U) |
           ((pd_u64)bytes[2] << 16U) |
           ((pd_u64)bytes[3] << 24U) |
           ((pd_u64)bytes[4] << 32U) |
           ((pd_u64)bytes[5] << 40U) |
           ((pd_u64)bytes[6] << 48U) |
           ((pd_u64)bytes[7] << 56U);
}

static inline void pd_store64_le_private(PD_PRIVATE pd_u8 *bytes, pd_u64 value) {
    for (unsigned int index = 0; index < 8U; ++index) {
        bytes[index] = (pd_u8)(value >> (8U * index));
    }
}

static inline void pd_keccak_f1600(PD_PRIVATE pd_u64 state[25]) {
    const pd_u64 round_constants[24] = {
        PD_U64_C(0x0000000000000001), PD_U64_C(0x0000000000008082),
        PD_U64_C(0x800000000000808a), PD_U64_C(0x8000000080008000),
        PD_U64_C(0x000000000000808b), PD_U64_C(0x0000000080000001),
        PD_U64_C(0x8000000080008081), PD_U64_C(0x8000000000008009),
        PD_U64_C(0x000000000000008a), PD_U64_C(0x0000000000000088),
        PD_U64_C(0x0000000080008009), PD_U64_C(0x000000008000000a),
        PD_U64_C(0x000000008000808b), PD_U64_C(0x800000000000008b),
        PD_U64_C(0x8000000000008089), PD_U64_C(0x8000000000008003),
        PD_U64_C(0x8000000000008002), PD_U64_C(0x8000000000000080),
        PD_U64_C(0x000000000000800a), PD_U64_C(0x800000008000000a),
        PD_U64_C(0x8000000080008081), PD_U64_C(0x8000000000008080),
        PD_U64_C(0x0000000080000001), PD_U64_C(0x8000000080008008),
    };
    const unsigned int rotation_constants[24] = {
        1U, 3U, 6U, 10U, 15U, 21U, 28U, 36U, 45U, 55U, 2U, 14U,
        27U, 41U, 56U, 8U, 25U, 43U, 62U, 18U, 39U, 61U, 20U, 44U,
    };
    const unsigned int pi_lanes[24] = {
        10U, 7U, 11U, 17U, 18U, 3U, 5U, 16U, 8U, 21U, 24U, 4U,
        15U, 23U, 19U, 13U, 12U, 2U, 20U, 14U, 22U, 9U, 6U, 1U,
    };

    pd_u64 columns[5];
    for (unsigned int round = 0; round < 24U; ++round) {
        for (unsigned int x = 0; x < 5U; ++x) {
            columns[x] = state[x] ^ state[x + 5U] ^ state[x + 10U] ^
                         state[x + 15U] ^ state[x + 20U];
        }
        for (unsigned int x = 0; x < 5U; ++x) {
            const pd_u64 theta = columns[(x + 4U) % 5U] ^
                                    pd_rotl64(columns[(x + 1U) % 5U], 1U);
            for (unsigned int y = 0; y < 25U; y += 5U) {
                state[y + x] ^= theta;
            }
        }

        pd_u64 carried = state[1];
        for (unsigned int index = 0; index < 24U; ++index) {
            const unsigned int lane = pi_lanes[index];
            const pd_u64 next = state[lane];
            state[lane] = pd_rotl64(carried, rotation_constants[index]);
            carried = next;
        }

        for (unsigned int row = 0; row < 25U; row += 5U) {
            for (unsigned int x = 0; x < 5U; ++x) {
                columns[x] = state[row + x];
            }
            for (unsigned int x = 0; x < 5U; ++x) {
                state[row + x] ^=
                    (~columns[(x + 1U) % 5U]) & columns[(x + 2U) % 5U];
            }
        }
        state[0] ^= round_constants[round];
    }
}

static inline void pd_pow_hash_with_nonce(
    PD_GLOBAL const pd_u8 pre_pow_hash[32],
    pd_u64 nonce,
    PD_PRIVATE pd_u8 output[32]
) {
    const pd_u64 initial_state[25] = {
        PD_U64_C(1242148031264380989), PD_U64_C(3008272977830772284),
        PD_U64_C(2188519011337848018), PD_U64_C(1992179434288343456),
        PD_U64_C(8876506674959887717), PD_U64_C(5399642050693751366),
        PD_U64_C(1745875063082670864), PD_U64_C(8605242046444978844),
        PD_U64_C(17936695144567157056), PD_U64_C(3343109343542796272),
        PD_U64_C(1123092876221303306), PD_U64_C(4963925045340115282),
        PD_U64_C(17037383077651887893), PD_U64_C(16629644495023626889),
        PD_U64_C(12833675776649114147), PD_U64_C(3784524041015224902),
        PD_U64_C(1082795874807940378), PD_U64_C(13952716920571277634),
        PD_U64_C(13411128033953605860), PD_U64_C(15060696040649351053),
        PD_U64_C(9928834659948351306), PD_U64_C(5237849264682708699),
        PD_U64_C(12825353012139217522), PD_U64_C(6706187291358897596),
        PD_U64_C(196324915476054915),
    };
    pd_u64 state[25];
    for (unsigned int index = 0; index < 25U; ++index) {
        state[index] = initial_state[index];
    }
    for (unsigned int index = 0; index < 4U; ++index) {
        state[index] ^= pd_load64_le_global(pre_pow_hash + 8U * index);
    }

    /* PulseDAG's canonical adapter calls PowHash::new(pre_pow_hash, 0). */
    state[9] ^= nonce;
    pd_keccak_f1600(state);
    for (unsigned int index = 0; index < 4U; ++index) {
        pd_store64_le_private(output + 8U * index, state[index]);
    }
}

static inline void pd_heavy_hash(
    PD_GLOBAL const pd_u16 *matrix,
    PD_PRIVATE const pd_u8 initial_hash[32],
    PD_PRIVATE pd_u8 output[32]
) {
    pd_u8 vector[64];
    for (unsigned int index = 0; index < 32U; ++index) {
        vector[2U * index] = initial_hash[index] >> 4U;
        vector[2U * index + 1U] = initial_hash[index] & 0x0fU;
    }

    pd_u8 product[32];
    for (unsigned int index = 0; index < 32U; ++index) {
        pd_u16 high_sum = 0;
        pd_u16 low_sum = 0;
        for (unsigned int component = 0; component < 64U; ++component) {
            high_sum = (pd_u16)(
                high_sum + matrix[(2U * index) * 64U + component] *
                               (pd_u16)vector[component]);
            low_sum = (pd_u16)(
                low_sum + matrix[(2U * index + 1U) * 64U + component] *
                              (pd_u16)vector[component]);
        }
        product[index] = (pd_u8)(((high_sum >> 10U) << 4U) | (low_sum >> 10U));
        product[index] ^= initial_hash[index];
    }

    const pd_u64 initial_state[25] = {
        PD_U64_C(4239941492252378377), PD_U64_C(8746723911537738262),
        PD_U64_C(8796936657246353646), PD_U64_C(1272090201925444760),
        PD_U64_C(16654558671554924250), PD_U64_C(8270816933120786537),
        PD_U64_C(13907396207649043898), PD_U64_C(6782861118970774626),
        PD_U64_C(9239690602118867528), PD_U64_C(11582319943599406348),
        PD_U64_C(17596056728278508070), PD_U64_C(15212962468105129023),
        PD_U64_C(7812475424661425213), PD_U64_C(3370482334374859748),
        PD_U64_C(5690099369266491460), PD_U64_C(8596393687355028144),
        PD_U64_C(570094237299545110), PD_U64_C(9119540418498120711),
        PD_U64_C(16901969272480492857), PD_U64_C(13372017233735502424),
        PD_U64_C(14372891883993151831), PD_U64_C(5171152063242093102),
        PD_U64_C(10573107899694386186), PD_U64_C(6096431547456407061),
        PD_U64_C(1592359455985097269),
    };
    pd_u64 state[25];
    for (unsigned int index = 0; index < 25U; ++index) {
        state[index] = initial_state[index];
    }
    for (unsigned int index = 0; index < 4U; ++index) {
        state[index] ^= pd_load64_le_private(product + 8U * index);
    }
    pd_keccak_f1600(state);
    for (unsigned int index = 0; index < 4U; ++index) {
        pd_store64_le_private(output + 8U * index, state[index]);
    }
}

static inline void pd_kheavyhash_from_pre_pow_hash(
    PD_GLOBAL const pd_u8 pre_pow_hash[32],
    PD_GLOBAL const pd_u16 *matrix,
    pd_u64 nonce,
    PD_PRIVATE pd_u8 output[32]
) {
    pd_u8 initial_hash[32];
    pd_pow_hash_with_nonce(pre_pow_hash, nonce, initial_hash);
    pd_heavy_hash(matrix, initial_hash, output);
}

#endif
