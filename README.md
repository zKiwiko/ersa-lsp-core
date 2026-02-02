# Overview

Core feature crate for the actual binary : https://github.com/zkiwiko/ersa-lsp

### Current features

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
- Semantic Token Highlighting
- Folding ranges
- Code Documents
- Redefinition Errors
- Hints on enum members not using UPPER_SNAKE_CASE

### Use as crate:

```rs
use ersa_lsp_core::lsp;

[tokio::main]
async fn main() {
    lsp::LSP::start.await
}
```

### development testing binary

```sh
cargo build --bin ersa_lsp

.../target/debug/ersa_lsp --stdin
```
