//! A safe Rust wrapper over llama.cpp's embedding path.
//!
//! This is the repository's FIRST executing model backend. Every row in
//! `docs/support-matrix.md` reads `Contracted` — "schema and deterministic Rust
//! behaviour exist, and nothing has ever executed" — and the `linux/x86_64/
//! llama-cpp-cpu` row is the one this crate is built to move.
//!
//! WHAT IT DOES AND DOES NOT CLAIM
//!
//! Loading a model and producing an embedding substantiates
//! `RuntimeSmokeValidated` for that row and nothing else. It says nothing about
//! the Vulkan row (a different backend), about any other OS, or about
//! `DeviceValidated`, which requires the real candidate model rather than a
//! fixture. Promotion is never by implication — see ADR-002.
//!
//! THE SHELL BOUNDARY STILL HOLDS
//!
//! This is a separate crate for the same reason `neuralcompose-headless` is:
//! `neuralcompose-mobile-core` promises to have almost no dependencies and no
//! I/O, and a crate that dlopens a 100 MB model and links a C++ library cannot
//! live inside that promise. The core defines the contracts
//! (`ProviderDescriptor`, `ModelPackKind`); this crate is one implementation
//! behind them.
//!
//! BUILDS WITHOUT LLAMA.CPP. With `LLAMA_CPP_DIR` unset the crate compiles to a
//! stub whose every entry point returns [`LlamaError::Unavailable`]. CI on four
//! platforms therefore does not need a C++ toolchain to keep the workspace
//! green, and a machine without a checkout still builds the workspace.

// `CString` is only reachable on the real path; on a stub build nothing
// crosses the FFI boundary, so importing it unconditionally is dead code.
#[cfg(not(nc_llama_stub))]
use std::ffi::CString;
use std::ffi::NulError;
use std::fmt;
use std::path::Path;

/// The llama.cpp commit this was built against, or `"stub"`.
pub const BACKEND_COMMIT: &str = {
    #[cfg(nc_llama_stub)]
    {
        "stub"
    }
    #[cfg(not(nc_llama_stub))]
    {
        env!("NC_LLAMA_COMMIT")
    }
};

/// Backend identifiers, matching the `Backend ID` column of the support matrix.
/// A value that disagreed with the matrix would make any evidence produced here
/// unattributable to a row.
pub const BACKEND_ID_CPU: &str = "llama-cpp-cpu";
pub const BACKEND_ID_VULKAN: &str = "llama-cpp-vulkan";

/// Kept for callers written before the Vulkan row existed.
pub const BACKEND_ID: &str = BACKEND_ID_CPU;

/// What kind of device ggml enumerated. Mirrors the shim's NC_DEV_* codes,
/// which are deliberately this crate's own numbers rather than ggml's enum
/// passed through — an upstream reordering must not silently change meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Cpu,
    /// Discrete GPU with dedicated memory.
    Gpu,
    /// Integrated GPU sharing host memory. ggml distinguishes this from `Gpu`,
    /// and on this project's target hardware (Radeon 890M) it is the one that
    /// appears — so collapsing the two would misdescribe the machine.
    IGpu,
    Accel,
    Other,
}

impl DeviceKind {
    /// Only reachable on a real build — a stub enumerates no devices, so
    /// there is no code to translate.
    #[cfg(not(nc_llama_stub))]
    fn from_code(c: i32) -> Self {
        match c {
            0 => Self::Cpu,
            1 => Self::Gpu,
            2 => Self::IGpu,
            3 => Self::Accel,
            _ => Self::Other,
        }
    }

    /// True for devices that are not the CPU. This is what distinguishes a
    /// Vulkan run from a CPU run.
    pub fn is_accelerator(self) -> bool {
        matches!(self, Self::Gpu | Self::IGpu | Self::Accel)
    }
}

/// One device ggml can see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub description: String,
    pub kind: DeviceKind,
}

/// Runtime ABI identifier, matching the matrix's `Runtime ABI` column.
pub const RUNTIME_ABI: &str = "nc-gguf-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlamaError {
    /// Built without llama.cpp. Not a failure of the model or the input.
    Unavailable,
    /// The path could not be turned into a C string (interior NUL).
    BadPath(String),
    ModelLoad(String),
    ContextCreate,
    Tokenize,
    Encode,
    /// The model produced no embedding — typically a generative model with no
    /// pooling layer, which is a model-choice error rather than a bug.
    NoEmbedding,
    BufferTooSmall {
        needed: usize,
        got: usize,
    },
    EmptyInput,
    NullArgument,
    /// A negative code the shim does not document. Kept distinct so an
    /// unrecognised code is never silently folded into a known one.
    Unknown(i32),
}

impl fmt::Display for LlamaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => write!(
                f,
                "built without llama.cpp (set LLAMA_CPP_DIR and rebuild to enable)"
            ),
            Self::BadPath(p) => write!(f, "model path is not a valid C string: {p}"),
            Self::ModelLoad(p) => write!(f, "could not load model at {p}"),
            Self::ContextCreate => write!(f, "could not create a llama context"),
            Self::Tokenize => write!(f, "tokenisation failed"),
            Self::Encode => write!(f, "encode failed"),
            Self::NoEmbedding => write!(
                f,
                "model produced no embedding — is this an embedding model?"
            ),
            Self::BufferTooSmall { needed, got } => {
                write!(f, "output buffer holds {got}, needs {needed}")
            }
            Self::EmptyInput => write!(f, "empty input text"),
            Self::NullArgument => write!(f, "null argument"),
            Self::Unknown(c) => write!(f, "unknown shim error code {c}"),
        }
    }
}

impl std::error::Error for LlamaError {}

impl From<NulError> for LlamaError {
    fn from(_: NulError) -> Self {
        Self::BadPath("<contains NUL>".into())
    }
}

/// Maps the shim's documented negative codes. Only reachable on a real build —
/// a stub never calls into C, so there is no code to map.
#[cfg(not(nc_llama_stub))]
fn error_from_code(code: i32) -> LlamaError {
    match code {
        -1 => LlamaError::ModelLoad("<see stderr>".into()),
        -2 => LlamaError::ContextCreate,
        -3 => LlamaError::Tokenize,
        -4 => LlamaError::Encode,
        -5 => LlamaError::NoEmbedding,
        -6 => LlamaError::BufferTooSmall { needed: 0, got: 0 },
        -7 => LlamaError::NullArgument,
        -8 => LlamaError::EmptyInput,
        other => LlamaError::Unknown(other),
    }
}

#[cfg(not(nc_llama_stub))]
mod ffi {
    use std::os::raw::{c_char, c_void};

    // Mirrors csrc/nc_llama_shim.c EXACTLY. Every type here is either an opaque
    // pointer or a fixed-width scalar, so nothing in this block can drift when
    // llama.cpp changes a struct — that is the entire reason the shim exists.
    //
    // `nc_llama_backend_free` is exported by the shim but deliberately NOT
    // bound here. It tears down process-global backend state, so calling it
    // from `Drop` would break any other live `Embedder`, and there is no
    // owner in this API that can safely decide the process is finished with
    // llama.cpp. The OS reclaims it at exit; an unused binding would only
    // invite someone to call it at the wrong moment.
    unsafe extern "C" {
        pub fn nc_llama_log_quiet();
        pub fn nc_llama_backend_load_all();
        pub fn nc_llama_device_count() -> i32;
        pub fn nc_llama_device_info(
            index: i32,
            name_buf: *mut c_char,
            name_len: i32,
            desc_buf: *mut c_char,
            desc_len: i32,
        ) -> i32;
        pub fn nc_llama_backend_init();
        pub fn nc_llama_model_load(path: *const c_char, n_gpu_layers: i32) -> *mut c_void;
        pub fn nc_llama_model_free(model: *mut c_void);
        pub fn nc_llama_n_embd(model: *mut c_void) -> i32;
        pub fn nc_llama_context_new(
            model: *mut c_void,
            n_ctx: i32,
            n_threads: i32,
            n_ubatch: i32,
        ) -> *mut c_void;
        pub fn nc_llama_context_free(ctx: *mut c_void);
        pub fn nc_llama_get_n_threads(ctx: *mut c_void) -> i32;
        pub fn nc_llama_get_n_ubatch(ctx: *mut c_void) -> i32;
        pub fn nc_llama_token_count(model: *mut c_void, text: *const c_char) -> i32;
        pub fn nc_llama_embed(
            model: *mut c_void,
            ctx: *mut c_void,
            text: *const c_char,
            out: *mut f32,
            out_len: i32,
            normalize: i32,
        ) -> i32;
    }
}

/// Process-global llama.cpp initialisation, run exactly once.
///
/// `llama_backend_init` and `ggml_backend_load_all` mutate a process-global
/// backend registry and are NOT safe to call concurrently. Every `Embedder`
/// construction used to call both, and Rust runs tests in parallel threads, so
/// two simultaneous loads raced on first-time registration: the full workspace
/// suite crashed with SIGSEGV or SIGABRT roughly one run in three while every
/// binary passed in isolation.
///
/// `Once` is the whole fix. It does not make `Embedder` `Send` or `Sync` —
/// those are still withheld — it only guarantees the one-time global setup
/// happens once and that later callers block until it has finished.
#[cfg(not(nc_llama_stub))]
fn init_backend_once() {
    static BACKEND_INIT: std::sync::Once = std::sync::Once::new();
    BACKEND_INIT.call_once(|| {
        // Quiet before load_all: backend registration prints device banners to
        // stdout, which would corrupt any caller parsing this crate's output.
        if std::env::var_os("NC_LLAMA_VERBOSE").is_none() {
            // SAFETY: installs a callback that ignores its arguments.
            unsafe { ffi::nc_llama_log_quiet() };
        }
        // SAFETY: called exactly once, before any other llama.cpp entry point.
        unsafe {
            ffi::nc_llama_backend_load_all();
            ffi::nc_llama_backend_init();
        }
    });
}

/// Serialises operations that touch llama.cpp's shared device state.
///
/// MEASURED, NOT PRECAUTIONARY. Creating and destroying contexts concurrently
/// on the Vulkan backend segfaults: `vulkan_agreement` crashed 4 times in 10
/// runs with Rust's default parallel test threads and 0 times in 10 with
/// `--test-threads=1`. Model load, context creation and teardown all mutate
/// per-device state inside ggml-vulkan that is not guarded there.
///
/// This does NOT make `Embedder` `Send` or `Sync`; those are still withheld.
/// It only guarantees that two threads never build or tear down a context at
/// the same moment. `embed` is deliberately left outside the lock — each
/// `Embedder` owns its own context and stays on one thread, so inference does
/// not contend, and serialising it would remove the concurrency the
/// accelerator exists to provide.
///
/// Poisoning is ignored: a panic in one thread's load must not permanently
/// disable loading for every other thread.
#[cfg(not(nc_llama_stub))]
fn device_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A loaded embedding model and its context.
///
/// Owns both, frees both on drop, and is deliberately NOT `Sync`: a
/// `llama_context` carries mutable inference state, so sharing one across
/// threads without external synchronisation would be a data race. `Send` is
/// also withheld — llama.cpp associates backend state with the creating thread
/// in some configurations, and claiming otherwise on the basis of "it seemed to
/// work" is precisely the sort of unevidenced promotion this repo avoids.
pub struct Embedder {
    #[cfg(not(nc_llama_stub))]
    model: *mut std::os::raw::c_void,
    #[cfg(not(nc_llama_stub))]
    ctx: *mut std::os::raw::c_void,
    n_embd: usize,
    /// How many layers were REQUESTED on an accelerator. Not proof any went
    /// there — see `accelerator`.
    requested_gpu_layers: u32,
    /// The accelerator ggml enumerated, if any. `None` means this run is CPU
    /// regardless of what was requested.
    accelerator: Option<Device>,
}

impl Embedder {
    /// Load a GGUF embedding model.
    ///
    /// `n_ctx` of 0 takes the model's own trained context length.
    #[cfg(not(nc_llama_stub))]
    pub fn load(path: &Path, n_ctx: u32) -> Result<Self, LlamaError> {
        Self::load_with(path, n_ctx, 0)
    }

    /// Load with `gpu_layers` offloaded to an accelerator.
    ///
    /// Requesting offload does NOT guarantee it happens: if no accelerator is
    /// enumerated, llama.cpp runs on CPU without complaint. `backend_id()`
    /// reports what actually happened, not what was asked for, because a
    /// support-matrix row that recorded the request rather than the outcome
    /// would be wrong in exactly the case that matters.
    #[cfg(not(nc_llama_stub))]
    pub fn load_with(path: &Path, n_ctx: u32, gpu_layers: u32) -> Result<Self, LlamaError> {
        Self::load_full(path, n_ctx, gpu_layers, 0)
    }

    /// Full control, including CPU thread count.
    ///
    /// `threads` of 0 keeps llama.cpp's own default — a hard-coded 4, marked
    /// "TODO: better default" upstream. That is faithful but a poor baseline:
    /// on a 24-core machine it uses one sixth of the CPU, and any CPU-vs-
    /// accelerator comparison against it measures a handicap rather than a
    /// backend. Callers making performance claims must set it explicitly.
    #[cfg(not(nc_llama_stub))]
    pub fn load_full(
        path: &Path,
        n_ctx: u32,
        gpu_layers: u32,
        threads: u32,
    ) -> Result<Self, LlamaError> {
        Self::load_tuned(path, n_ctx, gpu_layers, threads, 0)
    }

    /// Adds control of `n_ubatch`, the PHYSICAL batch size.
    ///
    /// llama.cpp splits a submitted batch into micro-batches of this size, so
    /// it determines where dispatch boundaries fall. `0` leaves it at whatever
    /// `n_ctx` implies. It is exposed because it is the manipulated variable
    /// when asking whether a latency step is a boundary effect: if a step moves
    /// when this moves, the boundary is the cause.
    #[cfg(not(nc_llama_stub))]
    pub fn load_tuned(
        path: &Path,
        n_ctx: u32,
        gpu_layers: u32,
        threads: u32,
        ubatch: u32,
    ) -> Result<Self, LlamaError> {
        let c_path = CString::new(path.as_os_str().as_encoded_bytes())?;
        // One-time global setup, serialised. Backends ship as separate shared
        // objects and are invisible until loaded, so this must happen before
        // the device probe below or the Vulkan device is simply absent and the
        // model runs on CPU while the caller believes otherwise.
        init_backend_once();
        // Held across model load, device probe and context creation — all three
        // touch shared device state. Released when this scope ends.
        let _device = device_lock();

        // `devices_unlocked`, NOT `devices`: the lock is already held here and
        // `std::sync::Mutex` is not reentrant, so calling the public function
        // would self-deadlock. A first attempt did exactly that and every test
        // run hung instead of crashing.
        let accelerator = devices_unlocked()
            .into_iter()
            .find(|d| d.kind.is_accelerator());

        // SAFETY: `c_path` is a valid NUL-terminated string that outlives the call.
        let model = unsafe { ffi::nc_llama_model_load(c_path.as_ptr(), gpu_layers as i32) };
        if model.is_null() {
            return Err(LlamaError::ModelLoad(path.display().to_string()));
        }

        // SAFETY: `model` is non-null and owned by this function.
        let n_embd = unsafe { ffi::nc_llama_n_embd(model) };
        if n_embd <= 0 {
            unsafe { ffi::nc_llama_model_free(model) };
            return Err(LlamaError::NoEmbedding);
        }

        // SAFETY: `model` is a valid handle.
        let ctx = unsafe {
            ffi::nc_llama_context_new(model, n_ctx as i32, threads as i32, ubatch as i32)
        };
        if ctx.is_null() {
            unsafe { ffi::nc_llama_model_free(model) };
            return Err(LlamaError::ContextCreate);
        }

        Ok(Self {
            model,
            ctx,
            n_embd: n_embd as usize,
            requested_gpu_layers: gpu_layers,
            accelerator: if gpu_layers > 0 { accelerator } else { None },
        })
    }

    #[cfg(nc_llama_stub)]
    pub fn load(_path: &Path, _n_ctx: u32) -> Result<Self, LlamaError> {
        Err(LlamaError::Unavailable)
    }

    #[cfg(nc_llama_stub)]
    pub fn load_with(_path: &Path, _n_ctx: u32, _gpu_layers: u32) -> Result<Self, LlamaError> {
        Err(LlamaError::Unavailable)
    }

    #[cfg(nc_llama_stub)]
    pub fn load_full(
        _path: &Path,
        _n_ctx: u32,
        _gpu_layers: u32,
        _threads: u32,
    ) -> Result<Self, LlamaError> {
        Err(LlamaError::Unavailable)
    }

    #[cfg(nc_llama_stub)]
    pub fn load_tuned(
        _path: &Path,
        _n_ctx: u32,
        _gpu_layers: u32,
        _threads: u32,
        _ubatch: u32,
    ) -> Result<Self, LlamaError> {
        Err(LlamaError::Unavailable)
    }

    /// The accelerator this instance is actually using, if any.
    pub fn accelerator(&self) -> Option<&Device> {
        self.accelerator.as_ref()
    }

    /// Layers requested on an accelerator. Compare with `accelerator()` — a
    /// non-zero request with `None` here is a silent fallback to CPU.
    pub fn requested_gpu_layers(&self) -> u32 {
        self.requested_gpu_layers
    }

    /// The support-matrix Backend ID this run's evidence belongs to.
    ///
    /// Derived from the OUTCOME, never the request. Offload was requested and
    /// no accelerator was found means this is a CPU run and must be filed as
    /// one, however the caller configured it.
    pub fn backend_id(&self) -> &'static str {
        match &self.accelerator {
            Some(_) if self.requested_gpu_layers > 0 => BACKEND_ID_VULKAN,
            _ => BACKEND_ID_CPU,
        }
    }

    /// The thread count llama.cpp actually adopted.
    ///
    /// Read back from the context rather than remembered from the request, so
    /// a setting that silently failed to apply is visible instead of assumed.
    #[cfg(not(nc_llama_stub))]
    pub fn active_threads(&self) -> i32 {
        // SAFETY: `ctx` is valid for the lifetime of `self`.
        unsafe { ffi::nc_llama_get_n_threads(self.ctx) }
    }

    #[cfg(nc_llama_stub)]
    pub fn active_threads(&self) -> i32 {
        0
    }

    /// The physical batch size llama.cpp adopted, read back rather than
    /// remembered — a swept parameter that silently failed to apply produces a
    /// flat result indistinguishable from a refuted hypothesis.
    #[cfg(not(nc_llama_stub))]
    pub fn active_ubatch(&self) -> i32 {
        // SAFETY: `ctx` is valid for the lifetime of `self`.
        unsafe { ffi::nc_llama_get_n_ubatch(self.ctx) }
    }

    #[cfg(nc_llama_stub)]
    pub fn active_ubatch(&self) -> i32 {
        0
    }

    /// How many tokens `text` becomes under this model's vocab, including the
    /// special tokens the embedding path adds.
    #[cfg(not(nc_llama_stub))]
    pub fn token_count(&self, text: &str) -> Option<usize> {
        let c = CString::new(text).ok()?;
        // SAFETY: `model` is valid; `c` is NUL-terminated and outlives the call.
        let n = unsafe { ffi::nc_llama_token_count(self.model, c.as_ptr()) };
        (n >= 0).then_some(n as usize)
    }

    #[cfg(nc_llama_stub)]
    pub fn token_count(&self, _text: &str) -> Option<usize> {
        None
    }

    /// Embedding dimensionality.
    pub fn dimensions(&self) -> usize {
        self.n_embd
    }

    /// Embed `text`, L2-normalised to match `llama-embedding`'s default.
    ///
    /// Normalisation is not cosmetic: the reference binary applies it, and the
    /// agreement test against that binary is the only external oracle this
    /// backend has. Changing the default here would break that test, which is
    /// the point of having it.
    #[cfg(not(nc_llama_stub))]
    pub fn embed(&mut self, text: &str) -> Result<Vec<f32>, LlamaError> {
        self.embed_with(text, true)
    }

    #[cfg(nc_llama_stub)]
    pub fn embed(&mut self, _text: &str) -> Result<Vec<f32>, LlamaError> {
        Err(LlamaError::Unavailable)
    }

    #[cfg(not(nc_llama_stub))]
    pub fn embed_with(&mut self, text: &str, normalize: bool) -> Result<Vec<f32>, LlamaError> {
        if text.is_empty() {
            return Err(LlamaError::EmptyInput);
        }
        let c_text = CString::new(text).map_err(|_| LlamaError::BadPath(text.into()))?;
        let mut out = vec![0.0f32; self.n_embd];
        // SAFETY: model and ctx are valid for the lifetime of `self`; `out` has
        // exactly `n_embd` capacity, which is what the shim checks against.
        let rc = unsafe {
            ffi::nc_llama_embed(
                self.model,
                self.ctx,
                c_text.as_ptr(),
                out.as_mut_ptr(),
                self.n_embd as i32,
                i32::from(normalize),
            )
        };
        if rc < 0 {
            return Err(error_from_code(rc));
        }
        if rc as usize != self.n_embd {
            return Err(LlamaError::BufferTooSmall {
                needed: rc as usize,
                got: self.n_embd,
            });
        }
        Ok(out)
    }

    #[cfg(nc_llama_stub)]
    pub fn embed_with(&mut self, _text: &str, _normalize: bool) -> Result<Vec<f32>, LlamaError> {
        Err(LlamaError::Unavailable)
    }
}

#[cfg(not(nc_llama_stub))]
impl Drop for Embedder {
    fn drop(&mut self) {
        // Teardown races with construction on the same device state, so it
        // takes the same lock. Without this, one thread freeing a context
        // while another builds one is the crash this guards.
        let _device = device_lock();
        // SAFETY: both pointers were produced by the matching constructors and
        // are freed exactly once. Context before model: the context holds a
        // reference to the model's weights.
        unsafe {
            ffi::nc_llama_context_free(self.ctx);
            ffi::nc_llama_model_free(self.model);
        }
    }
}

/// Every device ggml can see, in ggml's own order.
///
/// Empty on a stub build. Calls `ggml_backend_load_all` first, because backends
/// ship as separate shared objects and are invisible until loaded.
#[cfg(not(nc_llama_stub))]
pub fn devices() -> Vec<Device> {
    init_backend_once();
    let _device = device_lock();
    devices_unlocked()
}

/// Enumerate devices assuming the caller already holds the device lock.
#[cfg(not(nc_llama_stub))]
fn devices_unlocked() -> Vec<Device> {
    // SAFETY: reads a count from the registry.
    let n = unsafe { ffi::nc_llama_device_count() };
    if n <= 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let mut name = [0i8; 128];
        let mut desc = [0i8; 256];
        // SAFETY: both buffers are valid for the lengths passed, and the shim
        // uses snprintf, which always NUL-terminates within them.
        let code = unsafe {
            ffi::nc_llama_device_info(
                i,
                name.as_mut_ptr(),
                name.len() as i32,
                desc.as_mut_ptr(),
                desc.len() as i32,
            )
        };
        if code < 0 {
            continue;
        }
        out.push(Device {
            name: c_buf_to_string(&name),
            description: c_buf_to_string(&desc),
            kind: DeviceKind::from_code(code),
        });
    }
    out
}

#[cfg(nc_llama_stub)]
pub fn devices() -> Vec<Device> {
    Vec::new()
}

#[cfg(not(nc_llama_stub))]
fn c_buf_to_string(buf: &[i8]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|c| **c != 0)
        .map(|c| *c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// True when this build can actually run a model.
pub const fn is_available() -> bool {
    !cfg!(nc_llama_stub)
}

/// Cosine similarity between two equal-length vectors.
///
/// Returns `None` on a length mismatch or a zero-norm vector rather than
/// silently producing `NaN`, which would compare `false` against every
/// threshold and read as a quiet failure instead of a loud one — the same
/// reasoning `band_power` applies to non-finite samples.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let mut dot = 0.0f64;
    let mut na = 0.0f64;
    let mut nb = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        dot += f64::from(*x) * f64::from(*y);
        na += f64::from(*x) * f64::from(*x);
        nb += f64::from(*y) * f64::from(*y);
    }
    if na <= 0.0 || nb <= 0.0 {
        return None;
    }
    Some((dot / (na.sqrt() * nb.sqrt())) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_of_a_vector_with_itself_is_one() {
        let v = vec![0.1, -0.4, 0.9, 0.2];
        let c = cosine_similarity(&v, &v).expect("same length");
        assert!((c - 1.0).abs() < 1e-6, "got {c}");
    }

    #[test]
    fn cosine_of_opposites_is_minus_one_and_orthogonals_are_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let c = vec![0.0, 1.0];
        assert!((cosine_similarity(&a, &b).unwrap() + 1.0).abs() < 1e-6);
        assert!(cosine_similarity(&a, &c).unwrap().abs() < 1e-6);
    }

    #[test]
    fn cosine_refuses_rather_than_returning_nan() {
        // Each of these would produce NaN if computed naively, and a NaN
        // compares false against any threshold — a quiet failure.
        assert_eq!(cosine_similarity(&[1.0, 2.0], &[1.0]), None);
        assert_eq!(cosine_similarity(&[], &[]), None);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 1.0]), None);
        assert_eq!(cosine_similarity(&[f32::NAN, 1.0], &[1.0, 1.0]), None);
        assert_eq!(cosine_similarity(&[f32::INFINITY, 1.0], &[1.0, 1.0]), None);
    }

    #[test]
    fn the_backend_identifiers_match_the_support_matrix() {
        // If these drift from docs/support-matrix.md, evidence produced by this
        // crate cannot be attributed to a row.
        assert_eq!(BACKEND_ID, "llama-cpp-cpu");
        assert_eq!(RUNTIME_ABI, "nc-gguf-v1");
    }

    #[test]
    fn a_stub_build_reports_itself_as_unavailable() {
        // The dangerous direction is a stub that looks like a working backend.
        if !is_available() {
            assert_eq!(BACKEND_COMMIT, "stub");
            match Embedder::load(Path::new("/nonexistent.gguf"), 0) {
                Err(e) => assert_eq!(e, LlamaError::Unavailable),
                Ok(_) => panic!("a stub build must never return a working Embedder"),
            }
        } else {
            assert_ne!(BACKEND_COMMIT, "stub");
            assert_eq!(BACKEND_COMMIT.len(), 40, "expected a git sha");
        }
    }
}
