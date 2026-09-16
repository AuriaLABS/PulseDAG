/*
 * PulseDAG OpenCL kHeavyHash batch kernel.
 * Shared canonical math and provenance live in kheavyhash_shared.h.
 */
#include "kheavyhash_shared.h"

__kernel void pulsedag_kheavyhash_kernel(
    __global const pd_u8 *pre_pow_hash,
    __global const pd_u16 *matrix,
    __global const pd_u64 *nonces,
    __global pd_u8 *outputs,
    pd_u64 count
) {
    const pd_u64 index = (pd_u64)get_global_id(0);
    if (index >= count) {
        return;
    }

    pd_u8 digest[32];
    pd_kheavyhash_from_pre_pow_hash(pre_pow_hash, matrix, nonces[index], digest);
    for (unsigned int byte = 0; byte < 32U; ++byte) {
        outputs[index * 32U + byte] = digest[byte];
    }
}
