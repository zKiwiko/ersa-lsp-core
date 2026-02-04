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
