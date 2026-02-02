cargo clean
tree-sitter generate src/tree-sitter/grammar.js -o src/tree-sitter/src
cargo build