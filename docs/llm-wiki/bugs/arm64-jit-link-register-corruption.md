# ARM64 JIT Link Register Corruption

## Issue Description
When running certain cores (notably `mupen64plus_next` in Dynamic Recompiler mode) on ARM64 macOS, the emulator would occasionally crash with a segmentation fault or exhibit erratic behavior during heavy CPU load. 

The root cause was identified as Link Register (LR) corruption within the JIT-compiled code. On ARM64, the Link Register is used to store the return address. If the JIT-ed code or the boundary between the core and the host does not strictly adhere to the expected stack frame alignment and register preservation (specifically the preservation of the LR during function calls), the return address is lost, leading to a jump into invalid memory.

## Root Cause
The issue was primarily driven by the interaction between the core's dynamic recompiler and the host's thread management. In certain edge cases, the way the host handled the transition between the emulator thread and the audio/video callback threads caused a mismatch in the expected register state, particularly when the JIT-ed code attempted to execute a branch-and-link instruction without a properly established stack frame.

## Resolution
While the fix resides primarily within the core's JIT engine (upstream), the `arcade-libretro` host was updated to support the specific ARM64-optimized binaries.

1.  **Binary Targeting**: The host now explicitly searches for `mupen64plus_next_dynarec_arm64_libretro.dylib` when running on Apple Silicon. These binaries are compiled with stricter adherence to the ARM64 calling convention and stack alignment requirements.
2.s. **Environment Stability**: The host ensures that the environment provided to the core (via `RETRO_ENVIRONMENT`) is consistent with the expectations of the ARM64-optimized recompiler, reducing the likelihood of the core attempting to use incompatible instruction sequences.

## Verification
Verified by running long-duration sessions of N64 titles on M1/M2/M3 hardware without any LR-related crashes or instruction pointer deviations.
