mod query_file;

#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::env;
use std::fs;
use std::io::{self, Write as _};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use shpd_seedfinder_core::SHPD_VERSION;
use shpd_seedfinder_core::catalog::{ItemId, ItemKind};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::feasibility::QueryPlan;
use shpd_seedfinder_core::main_world::CanonicalMainWorldGenerator;
use shpd_seedfinder_core::query::{Requirement, SearchQuery, TierRequirement, UpgradeRequirement};
use shpd_seedfinder_core::search::{SearchOptions, SearchProgress, search_parallel};
use shpd_seedfinder_core::seed::TOTAL_SEEDS;

const DEFAULT_BENCHMARK_SEEDS: u64 = 10_000;
const SEARCH_CHUNK_SIZE: usize = 4;
const SEARCH_WINDOW_SEEDS: u64 = 4_096;

#[derive(Clone, Debug, Eq, PartialEq)]
struct BenchmarkOptions {
    seeds: u64,
    workers: Option<NonZeroUsize>,
    items: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Command {
    Benchmark(BenchmarkOptions),
    Search {
        items: PathBuf,
        workers: Option<NonZeroUsize>,
    },
    Help,
    Version,
}

fn main() -> ExitCode {
    match parse_args(env::args().skip(1)) {
        Ok(Command::Benchmark(options)) => match benchmark_command(&options) {
            Ok(report) => {
                println!("{report}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("seed-seeker: benchmark failed: {error}");
                ExitCode::FAILURE
            }
        },
        Ok(Command::Search { items, workers }) => match search_command(&items, workers) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("seed-seeker: search failed: {error}");
                ExitCode::FAILURE
            }
        },
        Ok(Command::Help) => {
            print!("{}", help());
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!(
                "seed-seeker {} (Shattered Pixel Dungeon {SHPD_VERSION})",
                env!("CARGO_PKG_VERSION")
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("seed-seeker: {error}\n\n{}", help());
            ExitCode::from(2)
        }
    }
}

fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    if arguments.is_empty() {
        return Ok(Command::Help);
    }
    if arguments.len() == 1 {
        match arguments[0].as_str() {
            "--help" | "-h" => return Ok(Command::Help),
            "--version" | "-V" => return Ok(Command::Version),
            _ => {}
        }
    }

    let mut benchmark_seeds = None;
    let mut workers = None;
    let mut items = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--benchmark" | "-b" => {
                if benchmark_seeds.is_some() {
                    return Err("benchmark may only be specified once".to_owned());
                }
                let mut seeds = DEFAULT_BENCHMARK_SEEDS;
                if arguments
                    .get(index + 1)
                    .is_some_and(|argument| !argument.starts_with('-'))
                {
                    index += 1;
                    seeds = parse_seed_count(&arguments[index])?;
                }
                benchmark_seeds = Some(seeds);
            }
            "--workers" => {
                if workers.is_some() {
                    return Err("--workers may only be specified once".to_owned());
                }
                index += 1;
                let value = arguments
                    .get(index)
                    .ok_or_else(|| "--workers requires a positive integer".to_owned())?;
                workers = Some(parse_worker_count(value)?);
            }
            "--items" | "-i" => {
                if items.is_some() {
                    return Err("--items may only be specified once".to_owned());
                }
                index += 1;
                let value = arguments
                    .get(index)
                    .ok_or_else(|| "--items requires a JSON file path".to_owned())?;
                items = Some(PathBuf::from(value));
            }
            "--help" | "-h" | "--version" | "-V" => {
                return Err("help and version cannot be combined with other options".to_owned());
            }
            argument => return Err(format!("unknown option '{argument}'")),
        }
        index += 1;
    }

    if let Some(seeds) = benchmark_seeds {
        return Ok(Command::Benchmark(BenchmarkOptions {
            seeds,
            workers,
            items,
        }));
    }
    if let Some(items) = items {
        return Ok(Command::Search { items, workers });
    }
    Err("--workers requires --benchmark or --items".to_owned())
}

fn parse_seed_count(value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|seeds| (1..=TOTAL_SEEDS).contains(seeds))
        .ok_or_else(|| format!("benchmark seed count must be between 1 and {TOTAL_SEEDS}"))
}

fn parse_worker_count(value: &str) -> Result<NonZeroUsize, String> {
    value
        .parse::<usize>()
        .ok()
        .and_then(NonZeroUsize::new)
        .ok_or_else(|| "--workers requires a positive integer".to_owned())
}

fn help() -> &'static str {
    concat!(
        "Seed Seeker command-line tools\n\n",
        "Usage:\n",
        "  seed-seeker --items FILE [--workers WORKERS]\n",
        "  seed-seeker [--items FILE] --benchmark [SEEDS] [--workers WORKERS]\n\n",
        "Options:\n",
        "  -b, --benchmark [SEEDS]  Benchmark a seed search\n",
        "                            [default: 10000]\n",
        "  -i, --items FILE          Read search requirements from a JSON file\n",
        "      --workers WORKERS     Number of search workers [default: available CPUs]\n",
        "  -h, --help                Print help\n",
        "  -V, --version             Print version\n",
    )
}

fn benchmark_command(benchmark: &BenchmarkOptions) -> Result<String, String> {
    let query = benchmark
        .items
        .as_deref()
        .map(load_query)
        .transpose()?
        .unwrap_or_else(benchmark_query);
    let workers = benchmark
        .workers
        .unwrap_or_else(SearchOptions::available_parallelism);
    let options = SearchOptions {
        start_seed: 0,
        end_seed_exclusive: benchmark.seeds,
        workers,
        chunk_size: NonZeroUsize::new(SEARCH_CHUNK_SIZE).expect("chunk size is non-zero"),
        max_results: NonZeroUsize::MAX,
    };
    let outcome = search_parallel(
        &CanonicalMainWorldGenerator::with_challenges(query.challenges),
        &query,
        options,
        &SearchProgress::default(),
    )
    .map_err(|error| format!("{error:?}"))?;

    Ok(format!(
        concat!(
            "Seed Seeker benchmark (Shattered Pixel Dungeon {shpd_version})\n",
            "Workers: {workers}\n",
            "Seeds tested: {tested}\n",
            "Elapsed: {elapsed:.3} s\n",
            "Throughput: {throughput:.0} seeds/s",
        ),
        shpd_version = SHPD_VERSION,
        workers = workers,
        tested = outcome.tested,
        elapsed = outcome.elapsed.as_secs_f64(),
        throughput = outcome.seeds_per_second(),
    ))
}

fn search_command(items: &Path, workers: Option<NonZeroUsize>) -> Result<(), String> {
    let query = load_query(items)?;
    if QueryPlan::analyze(&query).is_unsatisfiable() {
        eprintln!(
            "seed-seeker: no seed can satisfy this query within depth {}; nothing to search",
            query.max_depth
        );
        return Ok(());
    }
    let workers = workers.unwrap_or_else(SearchOptions::available_parallelism);
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    let mut start_seed = 0;
    while start_seed < TOTAL_SEEDS {
        let end_seed_exclusive = start_seed
            .saturating_add(SEARCH_WINDOW_SEEDS)
            .min(TOTAL_SEEDS);
        let outcome = search_parallel(
            &CanonicalMainWorldGenerator::with_challenges(query.challenges),
            &query,
            SearchOptions {
                start_seed,
                end_seed_exclusive,
                workers,
                chunk_size: NonZeroUsize::new(SEARCH_CHUNK_SIZE).expect("chunk size is non-zero"),
                max_results: NonZeroUsize::MAX,
            },
            &SearchProgress::default(),
        )
        .map_err(|error| format!("{error:?}"))?;
        for world in outcome.worlds {
            writeln!(output, "{}", world.seed)
                .map_err(|error| format!("could not write matching seed: {error}"))?;
        }
        output
            .flush()
            .map_err(|error| format!("could not flush matching seeds: {error}"))?;
        start_seed = end_seed_exclusive;
    }
    Ok(())
}

fn load_query(path: &Path) -> Result<SearchQuery, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("could not read '{}': {error}", path.display()))?;
    query_file::decode(&contents)
        .map_err(|error| format!("could not parse '{}': {error}", path.display()))
}

fn benchmark_query() -> SearchQuery {
    SearchQuery {
        requirements: vec![Requirement {
            kind: ItemKind::Ring,
            item: Some(ItemId::RingTenacity),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Exact(4),
            effect: None,
            source: None,
            identity_group: None,
            max_depth: None,
        }],
        max_depth: 24,
        challenges: Challenges::NONE,
        require_blacksmith: false,
        exclude_blacksmith_rewards: false,
        fast_mode: false,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::path::PathBuf;

    use shpd_seedfinder_core::seed::TOTAL_SEEDS;

    use super::{BenchmarkOptions, Command, help, parse_args};

    fn benchmark(seeds: u64, workers: Option<usize>) -> Command {
        Command::Benchmark(BenchmarkOptions {
            seeds,
            workers: workers.and_then(NonZeroUsize::new),
            items: None,
        })
    }

    #[test]
    fn accepts_both_benchmark_flags_with_an_optional_seed_count() {
        assert_eq!(
            parse_args(["--benchmark".to_owned()]),
            Ok(benchmark(10_000, None))
        );
        assert_eq!(
            parse_args(["-b".to_owned(), "1000".to_owned()]),
            Ok(benchmark(1_000, None))
        );
    }

    #[test]
    fn accepts_a_worker_count_before_or_after_the_benchmark() {
        assert_eq!(
            parse_args([
                "--benchmark".to_owned(),
                "1000".to_owned(),
                "--workers".to_owned(),
                "4".to_owned(),
            ]),
            Ok(benchmark(1_000, Some(4)))
        );
        assert_eq!(
            parse_args(["--workers".to_owned(), "2".to_owned(), "-b".to_owned(),]),
            Ok(benchmark(10_000, Some(2)))
        );
    }

    #[test]
    fn defaults_to_help_and_accepts_standard_information_flags() {
        assert_eq!(parse_args([]), Ok(Command::Help));
        assert_eq!(parse_args(["--help".to_owned()]), Ok(Command::Help));
        assert_eq!(parse_args(["-h".to_owned()]), Ok(Command::Help));
        assert_eq!(parse_args(["--version".to_owned()]), Ok(Command::Version));
        assert_eq!(parse_args(["-V".to_owned()]), Ok(Command::Version));
    }

    #[test]
    fn items_file_starts_a_search_without_the_benchmark_flag() {
        assert_eq!(
            parse_args([
                "--items".to_owned(),
                "requirements.json".to_owned(),
                "--workers".to_owned(),
                "3".to_owned(),
            ]),
            Ok(Command::Search {
                items: PathBuf::from("requirements.json"),
                workers: NonZeroUsize::new(3),
            })
        );
        assert_eq!(
            parse_args(["-i".to_owned(), "requirements.json".to_owned()]),
            Ok(Command::Search {
                items: PathBuf::from("requirements.json"),
                workers: None,
            })
        );
    }

    #[test]
    fn items_file_can_customize_a_benchmark_query() {
        assert_eq!(
            parse_args([
                "-i".to_owned(),
                "requirements.json".to_owned(),
                "-b".to_owned(),
                "1000".to_owned(),
            ]),
            Ok(Command::Benchmark(BenchmarkOptions {
                seeds: 1_000,
                workers: None,
                items: Some(PathBuf::from("requirements.json")),
            }))
        );
    }

    #[test]
    fn rejects_unknown_or_conflicting_arguments() {
        assert_eq!(
            parse_args(["--unknown".to_owned()]),
            Err("unknown option '--unknown'".to_owned())
        );
        assert_eq!(
            parse_args(["-b".to_owned(), "--help".to_owned()]),
            Err("help and version cannot be combined with other options".to_owned())
        );
    }

    #[test]
    fn rejects_invalid_seed_and_worker_counts() {
        for value in ["0", "many", &(TOTAL_SEEDS + 1).to_string()] {
            assert!(parse_args(["-b".to_owned(), value.to_owned()]).is_err());
        }
        for value in ["0", "many"] {
            assert!(
                parse_args(["-b".to_owned(), "--workers".to_owned(), value.to_owned(),]).is_err()
            );
        }
        assert!(parse_args(["--workers".to_owned(), "4".to_owned()]).is_err());
    }

    #[test]
    fn help_lists_benchmark_seed_and_worker_options() {
        assert!(help().contains("-b, --benchmark"));
        assert!(help().contains("-i, --items FILE"));
        assert!(help().contains("--workers WORKERS"));
    }
}
