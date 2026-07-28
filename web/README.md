# Web (future)

Optional React diagnostics client. Not started.

FFI note: the core's `u64` timestamps do not cross into JavaScript safely
(no integer beyond 2^53). A future wasm façade should expose `f64`
milliseconds; the contract fixtures already contain only f64-safe numbers.
