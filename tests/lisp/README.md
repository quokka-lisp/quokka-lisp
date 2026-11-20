# Differential tests

Each file holds Lisp forms with their expected printed results. The same file
runs on Quokka native, Quokka in WASM and SBCL; all three must agree. SBCL is
the oracle, not a build dependency — if it is not installed, the SBCL leg is
skipped with a warning rather than failing the build.

Empty until the reader and interpreter exist.
