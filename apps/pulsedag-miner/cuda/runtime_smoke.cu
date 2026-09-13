#include <cstdint>

extern "C" __global__ void pulsedag_cuda_runtime_smoke_kernel(
    const std::uint64_t* input,
    std::uint64_t* output
) {
    constexpr std::uint64_t kPulseDagSmokeMagic = 0x50554c5345444147ULL;
    output[0] = input[0] ^ kPulseDagSmokeMagic;
}
