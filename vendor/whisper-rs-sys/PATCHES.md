# Local patches to whisper-rs-sys 0.15.0

Vendored from crates.io and wired in with `[patch.crates-io]` (murmur and hablo).

1. **Target, not host, decides Apple BLAS** (`build.rs`): `cfg!(target_os = "macos")` in a
   build script describes the build machine, so cross-compiling from a Mac to Android
   tried to link `ggml-blas`. Now checks `TARGET`.
2. **Android NDK toolchain + ABI** (`build.rs`): passes `CMAKE_TOOLCHAIN_FILE`,
   `ANDROID_ABI` and `ANDROID_PLATFORM` to CMake. Without an explicit ABI the NDK
   toolchain silently builds 32-bit ARM.
3. **`external-ggml` feature** (`build.rs`, `Cargo.toml`): link only `libwhisper` and let
   another crate (llama-cpp-sys-2) provide ggml. Two ggml copies in one binary corrupt
   each other's backend registry (whisper aborts in `ggml_tallocr_alloc`).
4. **ggml upgraded to llama.cpp's copy** (`whisper.cpp/ggml/`, 0.9.5 → 0.24.0, from
   llama-cpp-sys-2 0.1.159) so whisper.cpp compiles against the same ggml it links to.
   Transcripts were unchanged on the test clips.
