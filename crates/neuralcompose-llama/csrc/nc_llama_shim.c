// A deliberately tiny, stable C ABI over llama.cpp's embedding path.
//
// WHY A SHIM AND NOT DIRECT FFI
//
// `llama_model_params` and `llama_context_params` are large structs that change
// between llama.cpp releases — fields are added, reordered, and retyped. Hand
// declaring them in Rust means hand-maintaining a byte-exact mirror of a moving
// target, and a mismatch is not a compile error: it is silent memory
// corruption, because the caller and callee disagree about where a field lives.
// `bindgen` would solve that, but it is a build dependency this repo has not
// taken.
//
// So the version-sensitive part stays in C, compiled against the real
// `llama.h`, where the compiler checks it. Rust sees only opaque pointers,
// `int32_t`, `float*` and `const char*` — types whose layout is fixed by the
// platform ABI and cannot drift. Everything below is the entire surface Rust
// is allowed to touch.
//
// If llama.cpp changes its API, THIS FILE fails to compile, loudly, at build
// time. That is the property being bought.

#include "llama.h"
#include "ggml-backend.h"

#include <math.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>

// Error codes. Negative, distinct, and stable — Rust maps them to an error
// enum, so renumbering them is a breaking change.
#define NC_OK                 0
#define NC_ERR_MODEL_LOAD    -1
#define NC_ERR_CONTEXT       -2
#define NC_ERR_TOKENIZE      -3
#define NC_ERR_ENCODE        -4
#define NC_ERR_NO_EMBEDDING  -5
#define NC_ERR_BUFFER_SMALL  -6
#define NC_ERR_NULL_ARG      -7
#define NC_ERR_EMPTY_INPUT   -8
#define NC_ERR_BAD_INDEX     -9

// Device type codes, mirroring ggml_backend_dev_type. Kept as our own numbers
// rather than passing ggml's enum through, so a reordering upstream cannot
// silently change what a caller thinks a device is.
#define NC_DEV_CPU   0
#define NC_DEV_GPU   1
#define NC_DEV_IGPU  2
#define NC_DEV_ACCEL 3
#define NC_DEV_OTHER 4

// llama.cpp logs ~200 lines of loader detail to stderr per model load. That is
// right for a debugging tool and wrong for a program a user runs to check their
// headband, so the default is silence and the detail is opt-in. Silencing is a
// separate call rather than automatic in backend_init: a caller diagnosing a
// load failure needs those lines, and a library that makes them unreachable is
// worse than one that prints too much.
static void nc_log_silent(enum ggml_log_level level, const char * text, void * user_data) {
    (void) level; (void) text; (void) user_data;
}

void nc_llama_log_quiet(void) { llama_log_set(nc_log_silent, NULL); }

void nc_llama_backend_init(void) { llama_backend_init(); }

// Backends built as separate shared objects (ggml-vulkan.so and friends) are
// discovered at runtime, not link time. Without this the Vulkan device is
// simply absent and the model runs on CPU while the caller believes otherwise
// — which would make a `llama-cpp-vulkan` row's evidence describe a CPU run.
void nc_llama_backend_load_all(void) { ggml_backend_load_all(); }

int32_t nc_llama_device_count(void) { return (int32_t) ggml_backend_dev_count(); }

// Writes the device's name and description, returns its NC_DEV_* type.
//
// This exists so a backend claim can be CHECKED rather than assumed: asking for
// GPU offload does not prove a GPU was found, and a silent fallback to CPU is
// exactly the failure that would make a support-matrix row wrong.
int32_t nc_llama_device_info(
    int32_t index,
    char *  name_buf, int32_t name_len,
    char *  desc_buf, int32_t desc_len
) {
    if (index < 0 || (size_t) index >= ggml_backend_dev_count()) { return NC_ERR_BAD_INDEX; }
    ggml_backend_dev_t dev = ggml_backend_dev_get((size_t) index);
    if (dev == NULL) { return NC_ERR_BAD_INDEX; }

    if (name_buf != NULL && name_len > 0) {
        const char * n = ggml_backend_dev_name(dev);
        snprintf(name_buf, (size_t) name_len, "%s", (n != NULL) ? n : "");
    }
    if (desc_buf != NULL && desc_len > 0) {
        const char * d = ggml_backend_dev_description(dev);
        snprintf(desc_buf, (size_t) desc_len, "%s", (d != NULL) ? d : "");
    }
    switch (ggml_backend_dev_type(dev)) {
        case GGML_BACKEND_DEVICE_TYPE_CPU:   return NC_DEV_CPU;
        case GGML_BACKEND_DEVICE_TYPE_GPU:   return NC_DEV_GPU;
        case GGML_BACKEND_DEVICE_TYPE_IGPU:  return NC_DEV_IGPU;
        case GGML_BACKEND_DEVICE_TYPE_ACCEL: return NC_DEV_ACCEL;
        default:                             return NC_DEV_OTHER;
    }
}
void nc_llama_backend_free(void) { llama_backend_free(); }

// `n_gpu_layers` is the caller's, and 0 means strictly CPU.
//
// It is a parameter rather than a constant because the support matrix has two
// separate linux/x86_64 rows — `llama-cpp-cpu` and `llama-cpp-vulkan` — and
// they are distinct claims. A row claiming CPU must not silently offload, and a
// row claiming Vulkan must not silently fall back; the caller states which it
// wants and `nc_llama_device_info` lets it verify what it got.
void * nc_llama_model_load(const char * path, int32_t n_gpu_layers) {
    if (path == NULL) { return NULL; }
    struct llama_model_params mparams = llama_model_default_params();
    mparams.n_gpu_layers = n_gpu_layers;
    return (void *) llama_model_load_from_file(path, mparams);
}

void nc_llama_model_free(void * model) {
    if (model != NULL) { llama_model_free((struct llama_model *) model); }
}

int32_t nc_llama_n_embd(void * model) {
    if (model == NULL) { return NC_ERR_NULL_ARG; }
    return llama_model_n_embd((const struct llama_model *) model);
}

void * nc_llama_context_new(void * model, int32_t n_ctx) {
    if (model == NULL) { return NULL; }
    struct llama_context_params cparams = llama_context_default_params();
    cparams.embeddings = true;
    // UNSPECIFIED means "use the pooling the model's own metadata declares".
    // Forcing MEAN or CLS here would silently disagree with the reference
    // `llama-embedding` binary for any model whose metadata says otherwise,
    // and the agreement test against that binary is the only oracle this has.
    cparams.pooling_type = LLAMA_POOLING_TYPE_UNSPECIFIED;
    if (n_ctx > 0) {
        cparams.n_ctx = (uint32_t) n_ctx;
        // n_batch must be able to hold the whole sequence: embeddings are
        // computed in one encode call, not incrementally.
        cparams.n_batch = (uint32_t) n_ctx;
        cparams.n_ubatch = (uint32_t) n_ctx;
    }
    return (void *) llama_init_from_model((struct llama_model *) model, cparams);
}

void nc_llama_context_free(void * ctx) {
    if (ctx != NULL) { llama_free((struct llama_context *) ctx); }
}

// Embed `text` into `out`.
//
// Returns the number of floats written, or a negative NC_ERR_*. `normalize`
// applies L2 normalisation, matching `llama-embedding`'s default
// (`--embd-normalize 2`); without it the values are not comparable to the
// reference output.
int32_t nc_llama_embed(
    void       * model,
    void       * ctx,
    const char * text,
    float      * out,
    int32_t      out_len,
    int32_t      normalize
) {
    if (model == NULL || ctx == NULL || text == NULL || out == NULL) {
        return NC_ERR_NULL_ARG;
    }
    const size_t text_len = strlen(text);
    if (text_len == 0) { return NC_ERR_EMPTY_INPUT; }

    struct llama_model   * m = (struct llama_model *) model;
    struct llama_context * c = (struct llama_context *) ctx;
    const struct llama_vocab * vocab = llama_model_get_vocab(m);

    const int32_t n_embd = llama_model_n_embd(m);
    if (n_embd <= 0)      { return NC_ERR_NO_EMBEDDING; }
    if (out_len < n_embd) { return NC_ERR_BUFFER_SMALL; }

    // Two-pass tokenisation: a negative return is minus the required capacity.
    int32_t n_tok = llama_tokenize(vocab, text, (int32_t) text_len, NULL, 0, true, false);
    if (n_tok < 0) { n_tok = -n_tok; }
    if (n_tok <= 0) { return NC_ERR_TOKENIZE; }

    llama_token * tokens = (llama_token *) malloc((size_t) n_tok * sizeof(llama_token));
    if (tokens == NULL) { return NC_ERR_TOKENIZE; }

    const int32_t got = llama_tokenize(vocab, text, (int32_t) text_len, tokens, n_tok, true, false);
    if (got <= 0) { free(tokens); return NC_ERR_TOKENIZE; }

    struct llama_batch batch = llama_batch_init(got, 0, 1);
    for (int32_t i = 0; i < got; i++) {
        batch.token[i]     = tokens[i];
        batch.pos[i]       = i;
        batch.n_seq_id[i]  = 1;
        batch.seq_id[i][0] = 0;
        // Every token is marked as an output token: pooled embeddings are
        // computed across the sequence, so the pooling layer needs them all.
        batch.logits[i]    = 1;
    }
    batch.n_tokens = got;

    const int32_t rc = llama_encode(c, batch);
    llama_batch_free(batch);
    free(tokens);
    if (rc != 0) { return NC_ERR_ENCODE; }

    const float * emb = llama_get_embeddings_seq(c, 0);
    if (emb == NULL) { return NC_ERR_NO_EMBEDDING; }

    if (normalize) {
        double sum = 0.0;
        for (int32_t i = 0; i < n_embd; i++) { sum += (double) emb[i] * (double) emb[i]; }
        // A zero-norm vector cannot be normalised; copy it through rather than
        // dividing by zero and emitting NaNs that would compare as "not equal"
        // to everything and read as a quiet failure downstream.
        const double norm = sqrt(sum);
        const double scale = (norm > 0.0) ? (1.0 / norm) : 1.0;
        for (int32_t i = 0; i < n_embd; i++) { out[i] = (float) ((double) emb[i] * scale); }
    } else {
        memcpy(out, emb, (size_t) n_embd * sizeof(float));
    }
    return n_embd;
}
