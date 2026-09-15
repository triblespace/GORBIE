//! GORBIE companion renderer for `trible pile net dashboard --gui`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use triblespace::core::blob::encodings::simplearchive::SimpleArchive;
use triblespace::core::collection::{
    AdmissionPolicy, Collection, CollectionPolicy, CollectionStoreExt,
};
use triblespace::core::repo::memoryrepo::MemoryRepo;
use triblespace::core::repo::pile::Pile;
use triblespace::core::repo::{BlobStoreList, SnapshotSource, StoreSnapshot};
use triblespace_net::dashboard::DashboardReport;
use triblespace_net::health_record;

use GORBIE::NotebookConfig;
use GORBIE::widgets::triblespace::cluster_health_notebook;

struct Args {
    pile: PathBuf,
    key: Option<PathBuf>,
    max_age: u64,
    sample: usize,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("gorbie-cluster-health: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let Some(args) = parse_args()? else {
        return Ok(());
    };
    let report = Arc::new(load_report(&args)?);
    NotebookConfig::new("TribleSpace cluster health")
        .run(move |nb| cluster_health_notebook(nb, Arc::clone(&report)))
        .map_err(|error| format!("launch notebook: {error}"))
}

fn load_report(args: &Args) -> Result<DashboardReport, String> {
    let key_path =
        triblespace::core::signing_key_file::resolve_path(args.key.as_deref(), &args.pile);
    let signer = triblespace::core::signing_key_file::load_existing(&key_path)
        .map_err(|error| format!("load signing key {}: {error}", key_path.display()))?;
    let authority = signer.verifying_key();
    let policy = CollectionPolicy::new(
        AdmissionPolicy::direct(authority),
        AdmissionPolicy::direct(authority),
    );
    let mut descriptors = MemoryRepo::default();
    let health: Collection<SimpleArchive> = descriptors
        .collection(health_record::COLLECTION_NAME, policy)
        .map_err(|error| format!("resolve health collection: {error}"))?;

    let mut pile = Pile::open(&args.pile)
        .map_err(|error| format!("open pile {}: {error:?}", args.pile.display()))?;
    pile.refresh()
        .map_err(|error| format!("refresh pile {}: {error}", args.pile.display()))?;
    let snapshot = pile
        .snapshot()
        .map_err(|error| format!("freeze pile {}: {error}", args.pile.display()))?;
    let health_facts = if snapshot
        .contains_blob(health.handle())
        .map_err(|error| format!("inspect health collection residency: {error}"))?
    {
        Some(
            health
                .read(&snapshot)
                .map_err(|error| format!("read observer reports: {error}"))?,
        )
    } else {
        None
    };
    let report = triblespace_net::dashboard::inspect(
        &snapshot,
        health_facts.as_ref(),
        snapshot.instant().to_tai_duration().total_nanoseconds(),
        Duration::from_secs(args.max_age),
        args.sample,
    )
    .map_err(|error| format!("inspect dashboard: {error}"))?;
    drop(snapshot);
    pile.close()
        .map_err(|error| format!("close pile {}: {error}", args.pile.display()))?;
    Ok(report)
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut pile = None;
    let mut key = None;
    let mut max_age = health_record::DEFAULT_MAX_AGE.as_secs();
    let mut sample = triblespace_net::dashboard::DEFAULT_SAMPLE_LIMIT;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pile" => pile = Some(PathBuf::from(next_value(&mut args, "--pile")?)),
            "--key" => key = Some(PathBuf::from(next_value(&mut args, "--key")?)),
            "--max-age" => {
                max_age = next_value(&mut args, "--max-age")?
                    .parse()
                    .map_err(|_| "--max-age expects seconds as an unsigned integer".to_owned())?;
            }
            "--sample" => {
                sample = next_value(&mut args, "--sample")?
                    .parse()
                    .map_err(|_| "--sample expects an unsigned integer".to_owned())?;
            }
            "-h" | "--help" => {
                print_help();
                return Ok(None);
            }
            positional if !positional.starts_with('-') && pile.is_none() => {
                pile = Some(PathBuf::from(positional));
            }
            unknown => return Err(format!("unknown argument {unknown:?}; use --help")),
        }
    }
    let pile = pile.ok_or_else(|| "missing --pile PATH; use --help".to_owned())?;
    Ok(Some(Args {
        pile,
        key,
        max_age,
        sample,
    }))
}

fn next_value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{option} expects a value"))
}

fn print_help() {
    println!(
        "GORBIE cluster-health dashboard\n\n\
         Usage: gorbie-cluster-health --pile PATH [--key PATH] [--max-age SECONDS] [--sample COUNT]\n\n\
         Reads one immutable pile snapshot and opens the same DashboardReport used by\n\
         `trible pile net dashboard`. It performs no fetch, maintenance, or write."
    );
}
