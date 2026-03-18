Run `cargo make coverage-ci` on the workspace. It uses --fail-under-lines 99 and
exits non-zero if coverage is below threshold. If it fails, run `cargo llvm-cov
--workspace` to see the detailed report, find the uncovered paths, and add unit
tests to bring coverage back above 99%.
