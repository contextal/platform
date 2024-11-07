# backend-utils: Common facilities and data structures for backend development ##

This crate provides helpers for reducing the boilerplate required to write
a backend.

It additionally provides shared utility functions and structs.

## Modules ##
- *crate*: defines the `work_loop!` macro which handles all communication with
  the frontend (you'll almost certainly want to use this)
- *tcpserver*: low level communication with the frontend (you generally don't need this)
- *objects*: interchange objects definitions (request, result, etc)
- *io*: shared I/O utilities

## Notes ##
This crates uses the tracing crate for logging.

Ensure that your backend includes a dependency on `tracing_subscriber` with the
"env-filter" feature enabled: `cargo add tracing_subscriber -F env-filter`

Then add subscriber initialization as the first thing in `main()`.

See the crate-level documentation (`cargo doc --no-deps --open`) for a minimal but complete
example of a `main()` fn.
