## Work in progress...

### Current progress

- Completion
  - All language constants
  - All language functions
  - All language data types
  - user defined functions

- Inlay Hints
  - built in function parameters
  - user defined function parameters

### Use as crate:

```rs
use ersa_lsp_core::lsp;

[tokio::main]
async fn main() {
    lsp::LSP::start.await
}
```

### Binary

```sh
cargo build --bin ersa_lsp

.../target/debug/ersa_lsp --stdin
```
