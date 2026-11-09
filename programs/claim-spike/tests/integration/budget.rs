//! The measurements behind docs/spikes/pig-133-claim-entries-budget.md. To
//! print the tables:
//!
//!     cargo test -p claim-spike --test integration budget -- --nocapture --test-threads 1
//!
//! The assertions pin the findings, so a change that moves a limit fails here.

use {
    crate::harness::{
        account_count, size, Test, CU_LIMIT, GIRL_GANG, MAX_ACCOUNT_LOCKS, MAX_TX_BYTES, SOL_GANG,
    },
    anchor_lang::{prelude::Pubkey, system_program},
    claim_spike::{
        client::{self, Claim, Pig},
        state::EntryReceipt,
    },
    litesvm::types::TransactionMetadata,
    solana_message::AddressLookupTableAccount,
};

/// Address seeds per batch. Each puts the position and the receipts at other
/// addresses, and so varies their bump searches.
const SEEDS: u64 = 8;
/// Batch sizes probed, past every limit.
const MAX_BATCH: usize = 14;
/// The limit to request without simulating: every batch that fits stays under
/// it, bump-search tail included (see `budget_receipt_bump_search`).
const FALLBACK_CU_LIMIT: u64 = 150_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mix {
    /// SOL Gang and Girl Gang, alternating.
    TokenMetadata,
    Core,
    /// SOL Gang, Girl Gang, Piggy Gang, in turn.
    Mixed,
}

impl Mix {
    const ALL: [Self; 3] = [Self::TokenMetadata, Self::Core, Self::Mixed];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Table {
    None,
    /// What every claim names: the config and the system program. The
    /// programs a transaction invokes can't come from a table, and neither
    /// can its signer.
    Shared,
    /// Plus the raffle: a table per raffle.
    PerRaffle,
}

fn pigs(test: &mut Test, mix: Mix, n: usize) -> Vec<Pig> {
    (0..n)
        .map(|i| match (mix, i % 3) {
            (Mix::Core, _) | (Mix::Mixed, 2) => test.core_pig(),
            (Mix::TokenMetadata, _) if i % 2 == 1 => test.tm_pig(GIRL_GANG),
            (Mix::Mixed, 1) => test.tm_pig(GIRL_GANG),
            _ => test.tm_pig(SOL_GANG),
        })
        .collect()
}

fn lookup_table(test: &mut Test, table: Table) -> Option<AddressLookupTableAccount> {
    let shared = vec![client::config(), system_program::ID];
    match table {
        Table::None => None,
        Table::Shared => Some(test.lookup_table(shared)),
        Table::PerRaffle => {
            let raffle = test.raffle;
            Some(test.lookup_table([shared, vec![raffle]].concat()))
        }
    }
}

#[derive(Clone, Copy)]
struct Shape {
    bytes: usize,
    accounts: usize,
}

impl Shape {
    fn fits(self) -> bool {
        self.bytes <= MAX_TX_BYTES && self.accounts <= MAX_ACCOUNT_LOCKS
    }

    fn mark(self) -> &'static str {
        if self.fits() {
            ""
        } else {
            " ✗"
        }
    }
}

fn shape(test: &mut Test, claim: Claim, mix: Mix, n: usize, table: Table) -> Shape {
    let pigs = pigs(test, mix, n);
    let table = lookup_table(test, table);
    let transaction = test.claim_transaction(claim, &pigs, table.as_ref());
    Shape {
        bytes: size(&transaction),
        accounts: account_count(&transaction),
    }
}

/// The largest batch that fits one transaction.
fn max_batch(test: &mut Test, claim: Claim, mix: Mix, table: Table) -> usize {
    (1..=MAX_BATCH)
        .take_while(|&n| shape(test, claim, mix, n, table).fits())
        .last()
        .unwrap_or(0)
}

/// Bytes and accounts: what decides how many NFTs a transaction carries.
#[test]
fn budget_transaction_size() {
    let mut test = Test::new();
    println!("\n### Bytes · accounts per claim transaction (✗: over 1,232 bytes or 64 accounts)\n");
    println!("| NFTs | Token Metadata | Core | Mixed | Token Metadata with mints |");
    println!("| ---: | ---: | ---: | ---: | ---: |");
    for n in 1..=MAX_BATCH {
        let cells: Vec<String> = [
            (Claim::Canonical, Mix::TokenMetadata),
            (Claim::Canonical, Mix::Core),
            (Claim::Canonical, Mix::Mixed),
            (Claim::WithMints, Mix::TokenMetadata),
        ]
        .into_iter()
        .map(|(claim, mix)| {
            let shape = shape(&mut test, claim, mix, n, Table::None);
            format!(
                "{} · {}{}",
                thousands(shape.bytes as u64),
                shape.accounts,
                shape.mark()
            )
        })
        .collect();
        println!("| {n} | {} |", cells.join(" | "));
    }

    println!("\n### Bytes with a lookup table (accounts as above)\n");
    println!(
        "| NFTs | Token Metadata, shared | Token Metadata, per raffle | Core, shared | Core, per \
         raffle | Mixed, shared | Mixed, per raffle |"
    );
    println!("| ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    for n in 1..=MAX_BATCH {
        let cells: Vec<String> = Mix::ALL
            .into_iter()
            .flat_map(|mix| [(mix, Table::Shared), (mix, Table::PerRaffle)])
            .map(|(mix, table)| {
                let shape = shape(&mut test, Claim::Canonical, mix, n, table);
                format!("{}{}", thousands(shape.bytes as u64), shape.mark())
            })
            .collect();
        println!("| {n} | {} |", cells.join(" | "));
    }

    // Each NFT's bytes: the step from one batch size to the next.
    let step = |test: &mut Test, claim, mix, table| {
        let one = shape(test, claim, mix, 1, table).bytes;
        (shape(test, claim, mix, 2, table).bytes - one, one)
    };
    let (tm_step, tm_one) = step(&mut test, Claim::Canonical, Mix::TokenMetadata, Table::None);
    let (core_step, _) = step(&mut test, Claim::Canonical, Mix::Core, Table::None);
    let (mint_step, _) = step(&mut test, Claim::WithMints, Mix::TokenMetadata, Table::None);
    let (_, shared_one) = step(
        &mut test,
        Claim::Canonical,
        Mix::TokenMetadata,
        Table::Shared,
    );
    let (_, per_raffle_one) = step(
        &mut test,
        Claim::Canonical,
        Mix::TokenMetadata,
        Table::PerRaffle,
    );
    println!(
        "\nPer NFT: Token Metadata {tm_step} B, Core {core_step} B, Token Metadata with its mint \
         {mint_step} B. Fixed: {} B. A shared table saves {} B, a per-raffle table {} B.",
        tm_one - tm_step,
        tm_one - shared_one,
        tm_one - per_raffle_one,
    );

    println!("\n### Largest batch per transaction\n");
    println!("| | Token Metadata | Core | Mixed |");
    println!("| --- | ---: | ---: | ---: |");
    let rows = [
        ("Legacy", Claim::Canonical, Table::None),
        (
            "Lookup table: config, system program",
            Claim::Canonical,
            Table::Shared,
        ),
        (
            "Lookup table: config, system program, raffle",
            Claim::Canonical,
            Table::PerRaffle,
        ),
        ("Legacy, with each mint", Claim::WithMints, Table::None),
    ];
    let mut limits = Vec::new();
    for (name, claim, table) in rows {
        let row: Vec<usize> = Mix::ALL
            .into_iter()
            .map(|mix| max_batch(&mut test, claim, mix, table))
            .collect();
        println!("| {name} | {} | {} | {} |", row[0], row[1], row[2]);
        limits.push(row);
    }

    assert_eq!(
        (tm_one - tm_step, tm_step, core_step, mint_step),
        (366, 100, 67, 133)
    );
    assert_eq!((tm_one - shared_one, tm_one - per_raffle_one), (26, 57));
    assert_eq!(limits[0], [8, 12, 9], "legacy");
    assert_eq!(limits[1], [8, 13, 9], "shared table");
    assert_eq!(limits[2], [9, 13, 10], "per-raffle table");
    assert_eq!(limits[3], [6, 12, 7], "with mints");
}

/// Compute units of a wallet's first claim in a raffle, which opens its
/// position: the dearest case. The raffle's cap is lifted so batches past 10
/// run; the cap check costs the same either way.
fn units(claim: Claim, mix: Mix, n: usize, table: Table) -> Vec<u64> {
    (0..SEEDS)
        .map(|seed| {
            let mut test = Test::with(seed, u8::MAX);
            let pigs = pigs(&mut test, mix, n);
            let table = lookup_table(&mut test, table);
            send(&mut test, claim, &pigs, table.as_ref()).compute_units_consumed
        })
        .collect()
}

fn send(
    test: &mut Test,
    claim: Claim,
    pigs: &[Pig],
    table: Option<&AddressLookupTableAccount>,
) -> TransactionMetadata {
    let transaction = test.claim_transaction(claim, pigs, table);
    test.svm
        .send_transaction(transaction)
        .unwrap_or_else(|failed| panic!("{}\n{}", failed.err, failed.meta.pretty_logs()))
}

fn median(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

/// Compute units: well inside the limits, so they never decide the batch.
#[test]
fn budget_compute_units() {
    println!("\n### Compute units of a first claim (median · max over {SEEDS} address seeds)\n");
    println!("| NFTs | Token Metadata | Core | Mixed |");
    println!("| ---: | ---: | ---: | ---: |");
    let mut test = Test::new();
    let mut table = Vec::new();
    let mut worst_fitting = 0;
    for n in 1..=MAX_BATCH {
        let row: Vec<Vec<u64>> = Mix::ALL
            .into_iter()
            .map(|mix| units(Claim::Canonical, mix, n, Table::None))
            .collect();
        let cells: Vec<String> = Mix::ALL
            .into_iter()
            .zip(&row)
            .map(|(mix, units)| {
                let max = *units.iter().max().unwrap();
                let shape = shape(&mut test, Claim::Canonical, mix, n, Table::None);
                if shape.fits() {
                    worst_fitting = worst_fitting.max(max);
                }
                format!(
                    "{} · {}{}",
                    thousands(median(units)),
                    thousands(max),
                    shape.mark()
                )
            })
            .collect();
        println!("| {n} | {} |", cells.join(" | "));
        table.push(row);
    }

    let per_nft = |mix: usize| {
        (median(&table[MAX_BATCH - 1][mix]) - median(&table[0][mix])) / (MAX_BATCH as u64 - 1)
    };
    println!(
        "\nPer NFT (median): Token Metadata {}, Core {}. Fixed: {}. The most any batch that fits \
         took: {}.",
        thousands(per_nft(0)),
        thousands(per_nft(1)),
        thousands(median(&table[0][0]) - per_nft(0)),
        thousands(worst_fitting),
    );
    assert!(
        worst_fitting < FALLBACK_CU_LIMIT * 3 / 4,
        "a batch that fits took {worst_fitting}: recheck the fallback limit"
    );
}

/// Where a claim's units go, from the traced variant's log of the units left.
#[test]
fn budget_where_the_units_go() {
    println!("\n### Where the units go (seed 0)\n");
    println!("| Step | 8 Token Metadata | 12 Core |");
    println!("| --- | ---: | ---: |");
    let mut columns = Vec::new();
    for (mix, n) in [(Mix::TokenMetadata, 8), (Mix::Core, 12)] {
        let mut test = Test::with(0, u8::MAX);
        let pigs = pigs(&mut test, mix, n);
        let traced = send(&mut test, Claim::Traced, &pigs, None);
        let left: Vec<u64> = traced
            .logs
            .iter()
            .filter_map(|line| line.strip_prefix("Program consumption: "))
            .map(|rest| rest.trim_end_matches(" units remaining").parse().unwrap())
            .collect();
        assert_eq!(left.len(), 2 + 2 * n);
        // left[0] before the first NFT, then for NFT i left[1 + 2i] after its
        // proof and left[2 + 2i] after its receipt, and last the end. Each
        // step's units include one trace call's.
        let (start, end) = (left[0], left[2 * n + 1]);
        let proof: u64 = (0..n).map(|i| left[2 * i] - left[1 + 2 * i]).sum();
        let receipt: u64 = (0..n).map(|i| left[1 + 2 * i] - left[2 + 2 * i]).sum();
        let total = traced.compute_units_consumed;
        let budget = u64::from(CU_LIMIT);
        let (before, tail, after) = (budget - start, left[2 * n] - end, total - (budget - end));
        columns.push([
            before,
            proof / n as u64,
            receipt / n as u64,
            tail,
            after,
            total,
        ]);
    }
    let names = [
        "Before the NFTs: compute budget, entry, accounts, position opened",
        "Proof, per NFT",
        "Receipt, per NFT: bump search, create, write",
        "Counting the entries",
        "After: accounts written back",
        "Total",
    ];
    for (i, name) in names.iter().enumerate() {
        println!(
            "| {name} | {} | {} |",
            thousands(columns[0][i]),
            thousands(columns[1][i])
        );
    }
}

/// The variants against the canonical claim, and the cases around it. Each
/// pair runs the same pigs at the same addresses, so the difference is exact.
#[test]
fn budget_variants() {
    println!("\n### Variants and cases (median over {SEEDS} address seeds)\n");
    println!("| Case | Canonical | Variant | Difference |");
    println!("| --- | ---: | ---: | ---: |");
    let row = |name: &str, base: &[u64], variant: &[u64], per: Option<u64>| {
        let differences: Vec<i128> = base
            .iter()
            .zip(variant)
            .map(|(&base, &variant)| i128::from(variant) - i128::from(base))
            .collect();
        let mut sorted = differences.clone();
        sorted.sort_unstable();
        let difference = sorted[sorted.len() / 2];
        let per = per.map_or(String::new(), |n| {
            format!(" ({} per NFT)", signed(difference / i128::from(n)))
        });
        println!(
            "| {name} | {} | {} | {}{per} |",
            thousands(median(base)),
            thousands(median(variant)),
            signed(difference)
        );
        differences
    };

    let tm8 = units(Claim::Canonical, Mix::TokenMetadata, 8, Table::None);
    let full = units(Claim::FullDecode, Mix::TokenMetadata, 8, Table::None);
    row(
        "8 Token Metadata, mpl-token-metadata's full decode",
        &tm8,
        &full,
        Some(8),
    );
    let tm6 = units(Claim::Canonical, Mix::TokenMetadata, 6, Table::None);
    let mints = units(Claim::WithMints, Mix::TokenMetadata, 6, Table::None);
    row(
        "6 Token Metadata, each with its mint",
        &tm6,
        &mints,
        Some(6),
    );
    let table = units(Claim::Canonical, Mix::TokenMetadata, 8, Table::PerRaffle);
    let loaded = row(
        "8 Token Metadata, v0 with the per-raffle table",
        &tm8,
        &table,
        None,
    );
    assert!(
        loaded.iter().all(|&difference| difference == 0),
        "{loaded:?}"
    );

    let mut open = Vec::new();
    let mut prefunded = Vec::new();
    for seed in 0..SEEDS {
        // An earlier claim opened the position.
        let mut test = Test::with(seed, u8::MAX);
        let batch = pigs(&mut test, Mix::TokenMetadata, 8);
        let opener = test.core_pig();
        send(&mut test, Claim::Canonical, &[opener], None);
        open.push(send(&mut test, Claim::Canonical, &batch, None).compute_units_consumed);

        // Someone funded every receipt address first, with the least the
        // runtime takes: an empty account's rent-exempt minimum.
        let mut test = Test::with(seed, u8::MAX);
        let batch = pigs(&mut test, Mix::TokenMetadata, 8);
        let least = test.svm.minimum_balance_for_rent_exemption(0);
        for pig in &batch {
            let receipt = client::receipt(&test.raffle, &pig.nft());
            test.svm.airdrop(&receipt, least).unwrap();
        }
        prefunded.push(send(&mut test, Claim::Canonical, &batch, None).compute_units_consumed);
    }
    row("8 Token Metadata, position already open", &tm8, &open, None);
    let prefunded = row(
        "8 Token Metadata, every receipt address prefunded",
        &tm8,
        &prefunded,
        None,
    );
    assert!(
        prefunded.iter().all(|&difference| difference == 0),
        "{prefunded:?}"
    );
}

/// How far `find_program_address` searches for a receipt's canonical bump:
/// 1,500 units per try.
#[test]
fn budget_receipt_bump_search() {
    const SAMPLES: u64 = 20_000;
    let raffle = Pubkey::new_from_array([7; 32]);
    let mut at_least = [0u64; 12];
    let mut test = Test::new();
    for _ in 0..SAMPLES {
        let nft = test.key();
        let (_, bump) = Pubkey::find_program_address(
            &[EntryReceipt::SEED, raffle.as_ref(), nft.as_ref()],
            &claim_spike::ID,
        );
        let tries = 256 - usize::from(bump);
        for count in at_least.iter_mut().take(tries.min(12)) {
            *count += 1;
        }
    }
    println!(
        "\n### Tries to find a receipt's bump ({} NFTs)\n",
        thousands(SAMPLES)
    );
    println!("| Tries | Share of NFTs needing at least that many | Extra units |");
    println!("| ---: | ---: | ---: |");
    for (i, count) in at_least.iter().enumerate().skip(1) {
        let thousandths = count * 100_000 / SAMPLES;
        println!(
            "| {} | {}.{:03}% | {} |",
            i + 1,
            thousandths / 1_000,
            thousandths % 1_000,
            thousands(1_500 * i as u64)
        );
    }
}

/// `-1234` as `−1,234`, and `1234` as `+1,234`.
fn signed(value: i128) -> String {
    let sign = if value < 0 { "−" } else { "+" };
    format!(
        "{sign}{}",
        thousands(u64::try_from(value.unsigned_abs()).unwrap())
    )
}

/// `1234567` as `1,234,567`.
fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}
