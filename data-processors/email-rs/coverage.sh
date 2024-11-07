#!/bin/sh

cargo clean
rm -f *.profraw *.profdata
RUSTFLAGS="-C instrument-coverage" cargo t
llvm-profdata merge -o email_rs.profdata -sparse *.profraw
rm -f *.profraw
llvm-cov report --ignore-filename-regex='/.cargo/registry' \
  --instr-profile=email_rs.profdata \
  $(for f in `ls -1 --ignore='*.*' target/debug/deps/`; do echo --object target/debug/deps/"$f"; done)
