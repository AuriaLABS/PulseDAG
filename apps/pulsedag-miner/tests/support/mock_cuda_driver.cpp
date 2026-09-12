#include <cstdint>
#include <cstdlib>
#include <cstring>

#include "../../cuda/kheavyhash.cu"

using CUresult = int;
using CUdevice = int;
using CUcontext = void*;
using CUmodule = void*;
using CUfunction = void*;
using CUstream = void*;
using CUdeviceptr = std::uint64_t;

namespace {
constexpr CUresult CUDA_SUCCESS = 0;
constexpr CUresult CUDA_ERROR_INVALID_VALUE = 1;
constexpr CUresult CUDA_ERROR_OUT_OF_MEMORY = 2;
constexpr std::uintptr_t MATRIX_FUNCTION = 1;
constexpr std::uintptr_t HASH_FUNCTION = 2;
}

extern "C" CUresult cuInit(unsigned int) {
    return CUDA_SUCCESS;
}

extern "C" CUresult cuDeviceGetCount(int* count) {
    if (count == nullptr) {
        return CUDA_ERROR_INVALID_VALUE;
    }
    *count = 1;
    return CUDA_SUCCESS;
}

extern "C" CUresult cuDeviceGet(CUdevice* device, int ordinal) {
    if (device == nullptr || ordinal != 0) {
        return CUDA_ERROR_INVALID_VALUE;
    }
    *device = 0;
    return CUDA_SUCCESS;
}

extern "C" CUresult cuCtxCreate_v2(CUcontext* context, unsigned int, CUdevice device) {
    if (context == nullptr || device != 0) {
        return CUDA_ERROR_INVALID_VALUE;
    }
    *context = reinterpret_cast<CUcontext>(0x1000);
    return CUDA_SUCCESS;
}

extern "C" CUresult cuCtxDestroy_v2(CUcontext context) {
    return context == nullptr ? CUDA_ERROR_INVALID_VALUE : CUDA_SUCCESS;
}

extern "C" CUresult cuModuleLoadData(CUmodule* module, const void* image) {
    if (module == nullptr || image == nullptr) {
        return CUDA_ERROR_INVALID_VALUE;
    }
    *module = reinterpret_cast<CUmodule>(0x2000);
    return CUDA_SUCCESS;
}

extern "C" CUresult cuModuleUnload(CUmodule module) {
    return module == nullptr ? CUDA_ERROR_INVALID_VALUE : CUDA_SUCCESS;
}

extern "C" CUresult cuModuleGetFunction(
    CUfunction* function,
    CUmodule module,
    const char* name
) {
    if (function == nullptr || module == nullptr || name == nullptr) {
        return CUDA_ERROR_INVALID_VALUE;
    }
    if (std::strcmp(name, "pulsedag_generate_matrix_kernel") == 0) {
        *function = reinterpret_cast<CUfunction>(MATRIX_FUNCTION);
        return CUDA_SUCCESS;
    }
    if (std::strcmp(name, "pulsedag_kheavyhash_kernel") == 0) {
        *function = reinterpret_cast<CUfunction>(HASH_FUNCTION);
        return CUDA_SUCCESS;
    }
    return CUDA_ERROR_INVALID_VALUE;
}

extern "C" CUresult cuMemAlloc_v2(CUdeviceptr* device_ptr, std::size_t bytes) {
    if (device_ptr == nullptr || bytes == 0) {
        return CUDA_ERROR_INVALID_VALUE;
    }
    void* allocation = std::malloc(bytes);
    if (allocation == nullptr) {
        return CUDA_ERROR_OUT_OF_MEMORY;
    }
    *device_ptr = static_cast<CUdeviceptr>(reinterpret_cast<std::uintptr_t>(allocation));
    return CUDA_SUCCESS;
}

extern "C" CUresult cuMemFree_v2(CUdeviceptr device_ptr) {
    if (device_ptr == 0) {
        return CUDA_ERROR_INVALID_VALUE;
    }
    std::free(reinterpret_cast<void*>(static_cast<std::uintptr_t>(device_ptr)));
    return CUDA_SUCCESS;
}

extern "C" CUresult cuMemcpyHtoD_v2(
    CUdeviceptr destination,
    const void* source,
    std::size_t bytes
) {
    if (destination == 0 || source == nullptr) {
        return CUDA_ERROR_INVALID_VALUE;
    }
    std::memcpy(
        reinterpret_cast<void*>(static_cast<std::uintptr_t>(destination)),
        source,
        bytes);
    return CUDA_SUCCESS;
}

extern "C" CUresult cuMemcpyDtoH_v2(
    void* destination,
    CUdeviceptr source,
    std::size_t bytes
) {
    if (destination == nullptr || source == 0) {
        return CUDA_ERROR_INVALID_VALUE;
    }
    std::memcpy(
        destination,
        reinterpret_cast<const void*>(static_cast<std::uintptr_t>(source)),
        bytes);
    return CUDA_SUCCESS;
}

extern "C" CUresult cuLaunchKernel(
    CUfunction function,
    unsigned int,
    unsigned int,
    unsigned int,
    unsigned int,
    unsigned int,
    unsigned int,
    unsigned int,
    CUstream,
    void** kernel_params,
    void**
) {
    if (function == nullptr || kernel_params == nullptr) {
        return CUDA_ERROR_INVALID_VALUE;
    }

    const auto function_id = reinterpret_cast<std::uintptr_t>(function);
    if (function_id == MATRIX_FUNCTION) {
        const CUdeviceptr pre_ptr = *reinterpret_cast<CUdeviceptr*>(kernel_params[0]);
        const CUdeviceptr matrix_ptr = *reinterpret_cast<CUdeviceptr*>(kernel_params[1]);
        auto* pre_pow_hash = reinterpret_cast<std::uint8_t*>(
            static_cast<std::uintptr_t>(pre_ptr));
        auto* matrix = reinterpret_cast<std::uint16_t (*)[64]>(
            static_cast<std::uintptr_t>(matrix_ptr));
        pulsedag_cuda::generate_matrix(pre_pow_hash, matrix);
        return CUDA_SUCCESS;
    }

    if (function_id == HASH_FUNCTION) {
        const CUdeviceptr pre_ptr = *reinterpret_cast<CUdeviceptr*>(kernel_params[0]);
        const CUdeviceptr matrix_ptr = *reinterpret_cast<CUdeviceptr*>(kernel_params[1]);
        const CUdeviceptr nonce_ptr = *reinterpret_cast<CUdeviceptr*>(kernel_params[2]);
        const CUdeviceptr output_ptr = *reinterpret_cast<CUdeviceptr*>(kernel_params[3]);
        const std::uint64_t count = *reinterpret_cast<std::uint64_t*>(kernel_params[4]);

        auto* pre_pow_hash = reinterpret_cast<std::uint8_t*>(
            static_cast<std::uintptr_t>(pre_ptr));
        auto* matrix = reinterpret_cast<std::uint16_t*>(
            static_cast<std::uintptr_t>(matrix_ptr));
        auto* nonces = reinterpret_cast<std::uint64_t*>(
            static_cast<std::uintptr_t>(nonce_ptr));
        auto* outputs = reinterpret_cast<std::uint8_t*>(
            static_cast<std::uintptr_t>(output_ptr));

        for (std::uint64_t index = 0; index < count; ++index) {
            pulsedag_cuda::kheavyhash_from_pre_pow_hash(
                pre_pow_hash,
                matrix,
                nonces[index],
                outputs + index * 32U);
        }
        return CUDA_SUCCESS;
    }

    return CUDA_ERROR_INVALID_VALUE;
}

extern "C" CUresult cuCtxSynchronize() {
    return CUDA_SUCCESS;
}
