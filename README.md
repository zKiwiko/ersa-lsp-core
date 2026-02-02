## Work in progress...

### Current progress

- Completion
  - All language constants
  - All language functions
  - All language data types
  - For User Defined Functions
  - For User Defined Variables
  - **Specs**
    - 300ms Debounce time before updating on change

- Inlay Hints
  - Built in function parameters
  - User defined function parameters

- Goto Definition
  - For User Defined Variables
  - For User Defined Functions

- Symbol Rename
  - Functions
  - Variables

- Hover
  - Functions
  - Variables

- Signature Help
- Document Symbols
- Document Highlight
- Folding ranges
- Code Documents (comments above definitions)

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
