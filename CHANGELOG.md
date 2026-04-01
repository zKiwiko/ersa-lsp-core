## 0.3.1 - April 1 2026

### Fixed

- Semantic higlighting for variables.

### Changed

- Code Lens to an optional toggle.
  - Toggle with: `--features code_lens` or `--features all`

## 0.3.1 - Mar 30 2026

### Fixed

- Function calls in expressions to error and require an `!`, even when they're not macros.
- Semantic highlighting token sorting.
- Semantic highlighting function token positions.

### Added

- Macro Semantic Highlights -- When enabled.
- Basic formatting logic
  - 4 space indentation
  - space after `,`
  - space before `{`
  - space after control keywords - but before `{`
  - collapse multiple spaces
  - trim empty lines
  - strip trailing whitespace

## 0.3.0 - Feb 13 2026

### Added

- Experimental language features. Visit the [documentation site](https://zkiwiko.github.io/ersa/) for more info.
  - File imports/inclusions with `import [path]`
  - Rust-like macro definitions and uses.

## 0.2.0 - Feb 2 2026

### Fixed

- Fixed grammar to support optional `int` typing in function parameters.
  - `function add(int a, int b)...`
- Fixed grammar to support `identifier`s in array sizes and enum assignment.
  - `enum { VAR = CONSTANT, ...}`, `int Arr[CONSTANT];`
- Warn when a function parameter is being overshadowed by a declared variable.

### Added

- Errors when trying to access a variable you havent defined yet.
- Errors when trying to access a function you havent defined yet.
- Errors when trying to set a value to an immutable variable.
- Warnings on unused variables and functions.
- Code Lens
  - Lists the amount of references above function definitions

## 0.1.0 - Feb 2 2026

initial release
