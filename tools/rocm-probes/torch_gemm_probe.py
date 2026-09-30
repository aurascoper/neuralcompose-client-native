"""ROCm-torch-on-gfx1150 probe. Same shape as the C reproducers in
docs/hardware/rocm-gfx1150-jacc-amdgpu.evidence/ — one operation, one verdict —
but for the path JACC never exercised: JACC passed 249/249 on gfx1150 because
JACC.BLAS is its own kernels; torch's matmul goes hipBLAS -> rocBLAS, whose
Ubuntu build ships no gfx1150 Tensile targets (repro-rocblas-output.txt).

Standing hypothesis: HSA_OVERRIDE_GFX_VERSION=11.0.0 maps gfx1150 onto the
gfx1100 Tensile targets that ARE shipped (both RDNA3-family). If that makes the
GEMM run, the dangerous failure mode is silently wrong numbers, not a crash —
so this asserts against a CPU reference and nothing may train on the device
until it exits 0, both with and without the override.

Exit codes: 0 all pass; 2 no HIP device; 3 elementwise wrong (device broken);
4 GEMM wrong (rocBLAS path broken, elementwise fine — the JACC-shaped split).
Run: python3 torch_gemm_probe.py            (needs a ROCm torch build)
     HSA_OVERRIDE_GFX_VERSION=11.0.0 python3 torch_gemm_probe.py
"""

import os
import sys

import torch

print("torch", torch.__version__, "| hip", getattr(torch.version, "hip", None))
print("HSA_OVERRIDE_GFX_VERSION =", os.environ.get("HSA_OVERRIDE_GFX_VERSION", "(unset)"))
if not torch.cuda.is_available():
    print("no HIP device visible")
    sys.exit(2)
props = torch.cuda.get_device_properties(0)
print("device:", torch.cuda.get_device_name(0), "|", getattr(props, "gcnArchName", "?"))

torch.manual_seed(0)
a = torch.randn(512, 512)
b = torch.randn(512, 512)

# 1. Elementwise: the path JACC's own kernels proved works on gfx1150.
ew = (a.cuda() + b.cuda()).cpu()
if not torch.allclose(ew, a + b, rtol=0, atol=0):
    print("FAIL elementwise: device compute broken")
    sys.exit(3)
print("elementwise add: exact match")

# 2. GEMM: the rocBLAS-backed path JACC bypasses.
gemm = (a.cuda() @ b.cuda()).cpu()
ref = a @ b
err = (gemm - ref).abs().max().item()
print(f"gemm max abs err vs CPU reference: {err:.3e}")
if not torch.allclose(gemm, ref, rtol=1e-4, atol=1e-3):
    print("FAIL gemm: rocBLAS path wrong (elementwise was fine)")
    sys.exit(4)
print("gemm: within tolerance — still not a license to train; run both override states first")
