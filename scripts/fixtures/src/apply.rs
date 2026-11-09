//! `apply` and `check`. Each run reads what exists, creates only what is
//! missing, verifies what is unverified, and returns items that wandered to
//! another fixture key to their holder. A rerun after a complete run sends
//! nothing.

use {
    crate::{
        build::{self, Step},
        chain::Chain,
        keys::Keys,
        spec::{Spec, TmNft, Wallet},
    },
    anyhow::{bail, Result},
    mpl_core::{
        accounts::{BaseAssetV1, BaseCollectionV1},
        types::UpdateAuthority,
    },
    mpl_token_metadata::accounts::Metadata,
    solana_account::Account,
    solana_instruction::Instruction,
    solana_keypair::Keypair,
    solana_program_pack::Pack,
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    std::collections::HashMap,
};

/// `Verify` instructions per transaction: each adds a metadata account.
const VERIFIES_PER_TX: usize = 8;
/// Core assets per transaction: each adds a signature and its name and URI.
const ASSETS_PER_TX: usize = 2;
/// SOL transfers per transaction.
const TRANSFERS_PER_TX: usize = 8;

/// Brings the cluster to the spec. Returns what it couldn't fix.
pub async fn apply(chain: &Chain, keys: &Keys, spec: &Spec) -> Result<Vec<String>> {
    let payer = chain.payer().pubkey();
    let mut lost = Vec::new();
    let mut state = State::read(chain, keys, spec).await?;

    // SOL for the wallets.
    let mut transfers = Vec::new();
    for wallet in Wallet::ALL {
        let balance = state.lamports(&wallet.address(keys));
        if balance < wallet.funding() / 2 {
            transfers.push(solana_system_interface::instruction::transfer(
                &payer,
                &wallet.address(keys),
                wallet.funding() - balance,
            ));
        }
    }
    for chunk in transfers.chunks(TRANSFERS_PER_TX) {
        run(
            chain,
            Step {
                what: format!("fund {} wallets", chunk.len()),
                instructions: chunk.to_vec(),
                signers: Vec::new(),
            },
        )
        .await?;
    }

    for collection in &spec.core_collections {
        if !state.exists(&keys.address(&collection.label)) {
            run(
                chain,
                build::create_core_collection(keys, collection, &payer),
            )
            .await?;
        }
    }

    let mut created = Vec::new();
    for nft in &spec.tm_nfts {
        if !state.exists(&build::metadata(&keys.address(&nft.label))) {
            run(chain, build::create_tm_nft(keys, nft, &payer)).await?;
            created.push(nft.label.as_str());
        }
    }
    // Reread: what was just created is unverified.
    if !created.is_empty() {
        state = State::read(chain, keys, spec).await?;
    }

    let mut creator_verifies = Vec::new();
    let mut collection_verifies = Vec::new();
    for nft in &spec.tm_nfts {
        let Some(metadata) = state.metadata(keys, nft)? else {
            bail!("{} is still missing after its create", nft.name);
        };
        for &(creator, verified, _) in &nft.creators {
            let address = creator.address(keys);
            let on_chain = metadata
                .creators
                .iter()
                .flatten()
                .find(|c| c.address == address);
            if verified && on_chain.is_some_and(|c| !c.verified) {
                creator_verifies.push(build::verify_creator(keys, nft, creator));
            }
        }
        if nft.collection.is_some() && !metadata.collection.as_ref().is_some_and(|c| c.verified) {
            collection_verifies.extend(build::verify_collection(keys, nft));
        }
    }
    for (what, verifies) in [
        ("creators", creator_verifies),
        ("collections", collection_verifies),
    ] {
        for chunk in verifies.chunks(VERIFIES_PER_TX) {
            let (instructions, signers): (Vec<Instruction>, Vec<Keypair>) = chunk
                .iter()
                .map(|(instruction, signer)| (instruction.clone(), signer.insecure_clone()))
                .unzip();
            run(
                chain,
                Step {
                    what: format!("verify {} {what}", chunk.len()),
                    instructions,
                    signers,
                },
            )
            .await?;
        }
    }

    let missing: Vec<Step> = spec
        .core_assets
        .iter()
        .filter(|asset| !state.exists(&keys.address(&asset.label)))
        .map(|asset| build::create_core_asset(keys, asset, &payer))
        .collect();
    for chunk in missing.chunks(ASSETS_PER_TX) {
        let mut step = Step {
            what: chunk
                .iter()
                .map(|step| step.what.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            instructions: Vec::new(),
            signers: Vec::new(),
        };
        for part in chunk {
            step.instructions.extend(part.instructions.iter().cloned());
            step.signers
                .extend(part.signers.iter().map(Keypair::insecure_clone));
        }
        run(chain, step).await?;
    }

    let token = &spec.token;
    let mint = keys.address(&token.label);
    if !state.exists(&mint) {
        run(chain, build::create_token(keys, token, &payer)).await?;
    }
    for &(wallet, whole) in &token.balances {
        let target = whole * 10u64.pow(u32::from(token.decimals));
        let held = state.token_amount(&build::token_account(&wallet.address(keys), &mint));
        if held < target {
            run(
                chain,
                build::mint_token(keys, token, &wallet.address(keys), target - held, &payer),
            )
            .await?;
        }
    }

    // Return items that moved to another fixture key; report the rest.
    let holders = build::fixture_holders(keys);
    let fixture_key = |owner: &Pubkey| {
        holders
            .iter()
            .find(|(address, _)| address == owner)
            .map(|(_, key)| key)
    };
    for nft in &spec.tm_nfts {
        let mint = keys.address(&nft.label);
        if created.contains(&nft.label.as_str())
            || state.token_amount(&build::token_account(&nft.holder.address(keys), &mint)) == 1
        {
            continue;
        }
        match chain.token_holder(&mint).await? {
            Some((_, owner)) => match fixture_key(&owner) {
                Some(key) => run(chain, build::return_tm_nft(keys, nft, key, &payer)).await?,
                None => lost.push(format!(
                    "{} is held by {owner}, outside the fixtures",
                    nft.name
                )),
            },
            None => lost.push(format!("{} has no holder: burned?", nft.name)),
        }
    }
    let state = State::read(chain, keys, spec).await?;
    for asset in &spec.core_assets {
        let Some(on_chain) = state.asset(keys, &asset.label)? else {
            continue;
        };
        if on_chain.owner != asset.holder.address(keys) {
            match fixture_key(&on_chain.owner) {
                Some(key) => run(chain, build::return_core_asset(keys, asset, key, &payer)).await?,
                None => lost.push(format!(
                    "{} is held by {}, outside the fixtures",
                    asset.name, on_chain.owner
                )),
            }
        }
    }
    Ok(lost)
}

/// Compares the cluster with the spec and returns every difference.
pub async fn check(chain: &Chain, keys: &Keys, spec: &Spec) -> Result<Vec<String>> {
    let state = State::read(chain, keys, spec).await?;
    let mut problems = Vec::new();
    for wallet in Wallet::ALL {
        if state.lamports(&wallet.address(keys)) < wallet.funding() / 2 {
            problems.push(format!("wallet {} is low on SOL", wallet.label()));
        }
    }
    for collection in &spec.core_collections {
        match state.collection(keys, &collection.label)? {
            None => problems.push(format!("collection {} is missing", collection.name)),
            Some(on_chain)
                if on_chain.update_authority != collection.update_authority.address(keys) =>
            {
                problems.push(format!(
                    "collection {} has another update authority",
                    collection.name
                ))
            }
            Some(_) => {}
        }
    }
    for nft in &spec.tm_nfts {
        let Some(metadata) = state.metadata(keys, nft)? else {
            problems.push(format!("{} is missing", nft.name));
            continue;
        };
        let creators: Vec<(Pubkey, bool, u8)> = metadata
            .creators
            .iter()
            .flatten()
            .map(|c| (c.address, c.verified, c.share))
            .collect();
        let expected: Vec<(Pubkey, bool, u8)> = nft
            .creators
            .iter()
            .map(|&(creator, verified, share)| (creator.address(keys), verified, share))
            .collect();
        if creators != expected {
            problems.push(format!(
                "{}: creators {creators:?}, expected {expected:?}",
                nft.name
            ));
        }
        if metadata.update_authority != nft.update_authority.address(keys)
            || metadata.symbol.trim_end_matches('\0') != nft.symbol
        {
            problems.push(format!("{}: wrong update authority or symbol", nft.name));
        }
        let collection = metadata.collection.as_ref().map(|c| (c.key, c.verified));
        let expected = nft
            .collection
            .as_ref()
            .map(|(label, _)| (keys.address(label), true));
        if collection != expected {
            problems.push(format!(
                "{}: collection {collection:?}, expected {expected:?}",
                nft.name
            ));
        }
        let mint = keys.address(&nft.label);
        if state.token_amount(&build::token_account(&nft.holder.address(keys), &mint)) != 1 {
            problems.push(format!("{} isn't with {}", nft.name, nft.holder.label()));
        }
    }
    for asset in &spec.core_assets {
        let Some(on_chain) = state.asset(keys, &asset.label)? else {
            problems.push(format!("{} is missing", asset.name));
            continue;
        };
        let expected = match &asset.collection {
            Some(collection) => UpdateAuthority::Collection(keys.address(collection)),
            None => UpdateAuthority::Address(asset.authority.address(keys)),
        };
        if on_chain.update_authority != expected {
            problems.push(format!(
                "{}: update authority {:?}",
                asset.name, on_chain.update_authority
            ));
        }
        if on_chain.owner != asset.holder.address(keys) {
            problems.push(format!(
                "{} isn't with {}",
                asset.name,
                asset.holder.label()
            ));
        }
    }
    let mint = keys.address(&spec.token.label);
    for &(wallet, whole) in &spec.token.balances {
        let target = whole * 10u64.pow(u32::from(spec.token.decimals));
        if state.token_amount(&build::token_account(&wallet.address(keys), &mint)) < target {
            problems.push(format!(
                "{} holds less than {whole} {}",
                wallet.label(),
                spec.token.symbol
            ));
        }
    }
    Ok(problems)
}

async fn run(chain: &Chain, step: Step) -> Result<()> {
    let signature = chain.send(&step.instructions, &step.signers).await?;
    println!("{}  {signature}", step.what);
    Ok(())
}

/// Every account the spec names, read in one sweep.
struct State(HashMap<Pubkey, Account>);

impl State {
    async fn read(chain: &Chain, keys: &Keys, spec: &Spec) -> Result<Self> {
        let mut addresses: Vec<Pubkey> = Wallet::ALL
            .iter()
            .map(|wallet| wallet.address(keys))
            .collect();
        for collection in &spec.core_collections {
            addresses.push(keys.address(&collection.label));
        }
        for nft in &spec.tm_nfts {
            let mint = keys.address(&nft.label);
            addresses.push(build::metadata(&mint));
            addresses.push(build::token_account(&nft.holder.address(keys), &mint));
        }
        for asset in &spec.core_assets {
            addresses.push(keys.address(&asset.label));
        }
        let mint = keys.address(&spec.token.label);
        addresses.push(mint);
        for &(wallet, _) in &spec.token.balances {
            addresses.push(build::token_account(&wallet.address(keys), &mint));
        }
        Ok(Self(chain.accounts(&addresses).await?))
    }

    fn exists(&self, address: &Pubkey) -> bool {
        self.0.contains_key(address)
    }

    fn lamports(&self, address: &Pubkey) -> u64 {
        self.0.get(address).map_or(0, |account| account.lamports)
    }

    fn token_amount(&self, address: &Pubkey) -> u64 {
        self.0
            .get(address)
            .and_then(|account| spl_token_interface::state::Account::unpack(&account.data).ok())
            .map_or(0, |token| token.amount)
    }

    fn metadata(&self, keys: &Keys, nft: &TmNft) -> Result<Option<Metadata>> {
        self.0
            .get(&build::metadata(&keys.address(&nft.label)))
            .map(|account| Ok(Metadata::from_bytes(&account.data)?))
            .transpose()
    }

    fn asset(&self, keys: &Keys, label: &str) -> Result<Option<BaseAssetV1>> {
        self.0
            .get(&keys.address(label))
            .map(|account| Ok(BaseAssetV1::from_bytes(&account.data)?))
            .transpose()
    }

    fn collection(&self, keys: &Keys, label: &str) -> Result<Option<BaseCollectionV1>> {
        self.0
            .get(&keys.address(label))
            .map(|account| Ok(BaseCollectionV1::from_bytes(&account.data)?))
            .transpose()
    }
}
