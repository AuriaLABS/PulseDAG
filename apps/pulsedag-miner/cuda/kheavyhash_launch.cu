#include "kheavyhash.cu"

namespace pulsedag_cuda_launch {

struct LaunchXoshiro256PlusPlus {
    std::uint64_t s0;
    std::uint64_t s1;
    std::uint64_t s2;
    std::uint64_t s3;

    PULSEDAG_HD std::uint64_t next() {
        const std::uint64_t result =
            s0 + pulsedag_cuda::rotl64(s0 + s3, 23U);
        const std::uint64_t shifted = s1 << 17U;
        s2 ^= s0;
        s3 ^= s1;
        s1 ^= s2;
        s0 ^= s3;
        s2 ^= shifted;
        s3 = pulsedag_cuda::rotl64(s3, 45U);
        return result;
    }
};

PULSEDAG_HD static inline double abs_double(double value) {
    return value < 0.0 ? -value : value;
}

PULSEDAG_HD static int matrix_rank(const std::uint16_t matrix[64][64]) {
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
               (selected[row] || abs_double(values[row][column]) <= epsilon)) {
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
                    abs_double(values[other][column]) > epsilon) {
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

PULSEDAG_HD static void generate_matrix(
    const std::uint8_t pre_pow_hash[32],
    std::uint16_t matrix[64][64]
) {
    LaunchXoshiro256PlusPlus generator{
        pulsedag_cuda::load64_le(pre_pow_hash),
        pulsedag_cuda::load64_le(pre_pow_hash + 8),
        pulsedag_cuda::load64_le(pre_pow_hash + 16),
        pulsedag_cuda::load64_le(pre_pow_hash + 24),
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

}  // namespace pulsedag_cuda_launch

#ifdef __CUDACC__
extern "C" __global__ void pulsedag_generate_matrix_kernel(
    const std::uint8_t* pre_pow_hash,
    std::uint16_t* matrix
) {
    if (blockIdx.x != 0U || threadIdx.x != 0U) {
        return;
    }
    pulsedag_cuda_launch::generate_matrix(
        pre_pow_hash,
        reinterpret_cast<std::uint16_t (*)[64]>(matrix));
}
#endif

#ifdef PULSEDAG_CUDA_LAUNCH_SELF_TEST
namespace {

int launch_hex_nibble(char value) {
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

bool launch_decode_hash(const char* hex, std::uint8_t output[32]) {
    for (unsigned index = 0; index < 32U; ++index) {
        const int high = launch_hex_nibble(hex[2U * index]);
        const int low = launch_hex_nibble(hex[2U * index + 1U]);
        if (high < 0 || low < 0) {
            return false;
        }
        output[index] = static_cast<std::uint8_t>((high << 4) | low);
    }
    return true;
}

bool check_matrix_vector(const char* id, const char* pre_pow_hash_hex) {
    std::uint8_t pre_pow_hash[32];
    std::uint16_t canonical[64][64];
    std::uint16_t launch[64][64];
    if (!launch_decode_hash(pre_pow_hash_hex, pre_pow_hash)) {
        std::fprintf(stderr, "invalid launch matrix vector: %s\n", id);
        return false;
    }

    pulsedag_cuda::generate_matrix(pre_pow_hash, canonical);
    pulsedag_cuda_launch::generate_matrix(pre_pow_hash, launch);
    if (std::memcmp(canonical, launch, sizeof(canonical)) != 0) {
        std::fprintf(stderr, "CUDA launch matrix mismatch: %s\n", id);
        return false;
    }
    std::printf("cuda_launch_matrix_vector=%s status=PASS\n", id);
    return true;
}

}  // namespace

int main() {
    const bool genesis = check_matrix_vector(
        "genesis-like-low-difficulty",
        "365bb76db67b8339d418038e1db2001c4d9866571249978d324f166dd78a4f1f");
    const bool single_parent = check_matrix_vector(
        "single-parent-mid-difficulty",
        "4437c1c239d16779fab0d4a03109504f7ccbddc494fc7db32662993b35cb5e1a");
    if (!(genesis && single_parent)) {
        return 1;
    }
    std::puts("task38_nvidia_cuda_launch_matrix_vectors=2/2 PASS");
    return 0;
}
#endif
