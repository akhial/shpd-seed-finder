# Seed-search throughput, 2026-09-05

## Baseline and method

The unmodified core at `6f51732` was built and benchmarked before editing it.
Its executable was saved separately so baseline and candidate could run
alternately without rebuilding between measurements.

- Intel Core i5-10400F, 6 cores / 12 logical processors, Windows 11 Home.
- Rust 1.97.1, `stable-x86_64-pc-windows-gnu`, LLVM 22.1.6.
- Repository release profile: O3, fat LTO, one codegen unit, mimalloc.
- No `target-cpu=native`, PGO, query changes, or lossy search shortcuts.
- Existing CLI `--benchmark`: +3 Wand of Fireblast through depth 24, exact
  planner reducing generation to the Wandmaker window ending at depth 9.
- Every invocation starts at numeric seed 0. Build time is excluded.
- Reported paired measurements run sequentially, without concurrent builds
  or test runs.

The initial 10,000-seed baseline runs returned 179-193 seeds/s with one
worker during environment setup. These preliminary runs are excluded from
the paired results below. Comparisons use the same seed count for both
versions; different ranges and background load can change the absolute rate.

## Measurements

Two alternating runs per version, 5,000 seeds per run, one worker:

| Version | Elapsed seconds | Combined throughput |
| --- | --- | ---: |
| Baseline | 24.975, 24.816 | 200.8 seeds/s |
| Optimized | 23.628, 23.432 | 212.5 seeds/s |

This is a **5.8% throughput increase**. Rates above use total seeds divided
by total elapsed time rather than averaging the CLI's rounded rates.

The final build was checked again after cleanup:

| Workers | Seeds per run | Baseline seconds (seeds/s) | Optimized seconds (seeds/s) | Throughput change |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 5,000 | 24.872 (201.0) | 23.284 (214.7) | +6.8% |
| 12 | 50,000 | 34.576 (1446.1) | 34.211 (1461.5) | +1.1% |

The single-worker improvement replicated across the paired runs. **A stable
multicore improvement was not established.** The earlier 50,000-seed sequence
measured baseline 34.119 s and optimized 35.152 / 35.205 s, while the final
clean pair above favored the optimized build. Background load, temperature,
and shared execution resources may contribute to this variability; these
measurements do not distinguish the causes. Shorter 10,000-seed, 12-worker
runs also varied (baseline 6.709 / 6.725 / 7.717 s; optimized 6.351 / 6.674 /
6.815 s). Do not extrapolate the single-worker percentage to all-core search.

A 36.641 s multicore baseline and a 23.460 s single-worker candidate overlapped
at the transition between experiments and were excluded. Their raw logs are
retained for transparency.

## Changes and SIMD assessment

- `bit_rows.rs` packs/unpacks sixteen boolean cells using baseline x86-64
  SSE2. Terrain smoothing, open-space flags, and maze output share these
  conversions. Remaining cells use scalar code; other architectures use the
  portable implementation. Unaligned loads/stores stay within each complete
  chunk, and stores preserve the required 0/1 boolean representation.
- Maze growth builds four valid-direction bit masks per column. Failed
  attempts reuse those masks; successful growth refreshes them. Every random
  draw and the 2,500-failure stopping rule are preserved.
- The existing ARM64 NEON depth-seed path remains in place. Full floor
  generation has divergent control flow and variable RNG consumption across
  seeds, so the selected SIMD work targets uniform cell conversion instead.
- Patch smoothing and maze geometry already use bit-parallel `u64` operations.
  There is no additional CPU feature requirement or runtime dispatch overhead
  for the new x86-64 path. SIMD instructions were confirmed in the release
  executable's disassembly.

Temporary coarse timing instrumentation on the baseline's first 2,000 seeds
measured approximately 1.06 s in maze growth, 1.16 s in patches (including
0.20 s in initial RNG fill), 0.22 s in flag generation, and 2.17 s in room
placement out of 10.26 s total. These inclusive measurements are diagnostic,
not additive or benchmark scores. The instrumentation was removed.

The intrinsics follow Rust's documented
[`_mm_loadu_si128`](https://doc.rust-lang.org/core/arch/x86_64/fn._mm_loadu_si128.html)
and [`_mm_movemask_epi8`](https://doc.rust-lang.org/core/arch/x86_64/fn._mm_movemask_epi8.html)
semantics. ARM64 performance has not been measured on this machine.

## Reproduction and validation

Build the baseline and changed revisions separately with the same toolchain:

```powershell
cargo +stable-x86_64-pc-windows-gnu build --locked --release -p shpd-seedfinder-cli
target/release/seed-seeker.exe --benchmark 5000 --workers 1
target/release/seed-seeker.exe --benchmark 50000 --workers 12
```

Run each version alternately several times after building, retaining all
elapsed times. The local `target/perf` directory holds the saved executables
and raw measurements; these generated artifacts are not committed.

New differential tests check bit conversions against scalar expectations for
every length 0–64, sixteen slice alignments, all single-bit patterns, and random
patterns, including sentinel cells outside the destination slice. Maze tests
compare masks to the original spatial probes and compare complete mazes and
subsequent RNG state to the original growth loop. The portable conversion
branch was also exercised locally in a temporary test harness.

```powershell
cargo +stable-x86_64-pc-windows-gnu fmt --all -- --check
cargo +stable-x86_64-pc-windows-gnu clippy --locked --workspace --exclude shpd-seedfinder-gtk --all-targets -- -D warnings
cargo +stable-x86_64-pc-windows-gnu test --locked --workspace --exclude shpd-seedfinder-gtk --release
```

**Validation passed:** 478 tests, Clippy with warnings denied, formatting,
and the portable-conversion harness. Four existing opt-in tests were ignored.

The GTK frontend requires Linux system libraries and is excluded here, as in
the repository's common Rust CI job. The opt-in long-running differential/fuzz
sweeps are not part of the default test run.
