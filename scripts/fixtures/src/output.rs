//! What a run leaves in the repo. For each cluster it writes the manifest the
//! tests and the E2E run read, plus the indexer's collections config and mint
//! lists. `dump` copies accounts for the claim spike's LiteSVM tests.

use {
    crate::{
        build,
        chain::Chain,
        keys::Keys,
        spec::{self, Authority, Lookalike, Pigs, Spec, Wallet, CORE_PRIZES, GM_PRIZES, VOUCHERS},
        Cluster,
    },
    anyhow::{bail, Context, Result},
    base64::{engine::general_purpose::STANDARD as BASE64, Engine},
    serde_json::{json, Value},
    solana_pubkey::Pubkey,
    std::{fmt::Write as _, path::Path},
};

/// Writes `manifest.json`, `collections.toml` and `seeds/*.mints.json` into
/// `dir`.
pub fn write(dir: &Path, cluster: Cluster, keys: &Keys, spec: &Spec) -> Result<()> {
    std::fs::create_dir_all(dir.join("seeds"))?;
    let manifest = manifest(cluster, keys, spec);
    std::fs::write(dir.join("manifest.json"), format!("{:#}\n", manifest))?;
    std::fs::write(
        dir.join("collections.toml"),
        collections_toml(cluster, keys),
    )?;
    for pigs in Pigs::ALL
        .into_iter()
        .filter(|pigs| *pigs != Pigs::PiggyGang)
    {
        let mints: Vec<String> = (1..=pigs.count())
            .map(|index| keys.address(&pigs.label(index)).to_string())
            .collect();
        std::fs::write(
            dir.join(format!("seeds/{}.mints.json", pigs.slug())),
            format!("{:#}\n", json!(mints)),
        )?;
    }
    Ok(())
}

/// The root a manifest in `dir` was written with, if there is one.
pub fn manifest_root(dir: &Path) -> Result<Option<Pubkey>> {
    let path = dir.join("manifest.json");
    if !path.exists() {
        return Ok(None);
    }
    let manifest: Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    let root = manifest["root"]
        .as_str()
        .with_context(|| format!("{} has no root", path.display()))?;
    Ok(Some(root.parse()?))
}

fn manifest(cluster: Cluster, keys: &Keys, spec: &Spec) -> Value {
    let address = |label: &str| keys.address(label).to_string();
    let authorities: serde_json::Map<String, Value> = Authority::ALL
        .into_iter()
        .map(|authority| {
            (
                authority.label().to_owned(),
                json!(authority.address(keys).to_string()),
            )
        })
        .collect();

    let mut wallets = serde_json::Map::new();
    for wallet in Wallet::ALL {
        let mut pigs = serde_json::Map::new();
        for collection in Pigs::ALL {
            let held: Vec<String> = (1..=collection.count())
                .filter(|&index| spec::holder(collection, index) == wallet)
                .map(|index| address(&collection.label(index)))
                .collect();
            if !held.is_empty() {
                pigs.insert(collection.slug().to_owned(), json!(held));
            }
        }
        wallets.insert(
            wallet.label().to_owned(),
            json!({
                "address": wallet.address(keys).to_string(),
                "purpose": wallet.purpose(),
                "lamports": wallet.funding(),
                "pigs": pigs,
            }),
        );
    }

    let mut collections = serde_json::Map::new();
    for pigs in Pigs::ALL {
        let items: Vec<Value> = (1..=pigs.count())
            .map(|index| {
                let item = address(&pigs.label(index));
                let holder = spec::holder(pigs, index);
                if pigs == Pigs::PiggyGang {
                    json!({ "index": index, "asset": item, "holder": holder.label() })
                } else {
                    let mint = keys.address(&pigs.label(index));
                    json!({
                        "index": index,
                        "mint": item,
                        "metadata": build::metadata(&mint).to_string(),
                        "token_account": build::token_account(&holder.address(keys), &mint).to_string(),
                        "holder": holder.label(),
                    })
                }
            })
            .collect();
        let mut collection = json!({
            "name": pigs.name(),
            "rule_slot": pigs.rule_slot(),
            "pigs": items,
        });
        match pigs.symbol() {
            Some(symbol) => {
                collection["standard"] = json!("token_metadata");
                collection["symbol"] = json!(symbol);
                collection["verified_creator"] = json!(pigs.creator().address(keys).to_string());
                collection["update_authority"] =
                    json!(Authority::UpdateAuthority.address(keys).to_string());
            }
            None => {
                collection["standard"] = json!("core");
                collection["address"] = json!(address(&format!("collection/{}", pigs.slug())));
            }
        }
        collections.insert(pigs.slug().to_owned(), collection);
    }

    let lookalikes: Vec<Value> = Lookalike::ALL
        .into_iter()
        .map(|lookalike| {
            let item = keys.address(&lookalike.label());
            let mut entry = json!({
                "slug": lookalike.slug(),
                "imitates": lookalike.imitates().slug(),
                "rule_slot": lookalike.imitates().rule_slot(),
                "why": lookalike.why(),
                "holder": Wallet::Impostor.label(),
            });
            if lookalike.imitates() == Pigs::PiggyGang {
                entry["asset"] = json!(item.to_string());
            } else {
                entry["mint"] = json!(item.to_string());
                entry["metadata"] = json!(build::metadata(&item).to_string());
                entry["token_account"] =
                    json!(build::token_account(&Wallet::Impostor.address(keys), &item).to_string());
            }
            entry
        })
        .collect();

    let mut prizes = serde_json::Map::new();
    for collection in [GM_PRIZES, VOUCHERS] {
        let nfts: Vec<Value> = spec
            .tm_nfts
            .iter()
            .filter(|nft| {
                nft.collection
                    .as_ref()
                    .is_some_and(|(label, _)| label == &format!("mint/{}", collection.slug))
            })
            .map(|nft| json!({ "mint": address(&nft.label), "holder": nft.holder.label() }))
            .collect();
        prizes.insert(
            collection.slug.to_owned(),
            json!({
                "name": collection.name,
                "allowlist_kind": "Collection",
                "collection_mint": address(&format!("mint/{}", collection.slug)),
                "nfts": nfts,
            }),
        );
    }
    let core_label = format!("collection/{}", CORE_PRIZES.0);
    let assets: Vec<Value> = spec
        .core_assets
        .iter()
        .filter(|asset| asset.collection.as_deref() == Some(core_label.as_str()))
        .map(|asset| json!({ "asset": address(&asset.label), "holder": asset.holder.label() }))
        .collect();
    prizes.insert(
        CORE_PRIZES.0.to_owned(),
        json!({
            "name": CORE_PRIZES.1,
            "allowlist_kind": "CoreCollection",
            "collection": address(&core_label),
            "assets": assets,
        }),
    );
    let token = &spec.token;
    let balances: serde_json::Map<String, Value> = token
        .balances
        .iter()
        .map(|&(wallet, whole)| (wallet.label().to_owned(), json!(whole)))
        .collect();
    prizes.insert(
        "test-prize-token".to_owned(),
        json!({
            "name": token.name,
            "symbol": token.symbol,
            "allowlist_kind": "Mint",
            "mint": address(&token.label),
            "decimals": token.decimals,
            "balances": balances,
        }),
    );

    let rules: Vec<Value> = [Pigs::SolGang, Pigs::GirlGang, Pigs::PiggyGang]
        .into_iter()
        .map(|pigs| match pigs.symbol() {
            Some(symbol) => json!({
                "slot": pigs.rule_slot(),
                "collection": pigs.slug(),
                "kind": 0,
                "address": pigs.creator().address(keys).to_string(),
                "update_authority": Authority::UpdateAuthority.address(keys).to_string(),
                "symbol": symbol,
            }),
            None => json!({
                "slot": pigs.rule_slot(),
                "collection": pigs.slug(),
                "kind": 1,
                "address": address(&format!("collection/{}", pigs.slug())),
                "update_authority": null,
                "symbol": null,
            }),
        })
        .collect();

    json!({
        "cluster": cluster.name(),
        "written_by": "scripts/fixtures",
        "root": keys.root().to_string(),
        "programs": {
            "token_metadata": mpl_token_metadata::ID.to_string(),
            "mpl_core": mpl_core::ID.to_string(),
        },
        "membership_rules": rules,
        "authorities": authorities,
        "wallets": wallets,
        "collections": collections,
        "lookalikes": lookalikes,
        "prizes": prizes,
    })
}

/// The indexer's seed format (its `config/collections.toml`), for the
/// cluster's registry (PIG-134, PIG-139).
fn collections_toml(cluster: Cluster, keys: &Keys) -> String {
    let mut toml = format!(
        "# {} stand-ins for the Piggy collections, written by scripts/fixtures (PIG-115).\n\
         # The indexer's seed format: copy into its {} registry with the seeds/ beside it.\n\
         version = 1\n",
        cluster.name(),
        cluster.name()
    );
    for pigs in Pigs::ALL {
        let _ = write!(
            toml,
            "\n[[collections]]\nslug = \"{}\"\nname = \"{}\"\n",
            pigs.slug(),
            pigs.name()
        );
        match pigs.symbol() {
            Some(symbol) => {
                let _ = write!(
                    toml,
                    "standard = \"token_metadata\"\nenabled = true\nverified_creator = \"{}\"\n\
                     update_authority = \"{}\"\nsymbol = \"{symbol}\"\n\
                     mints = {{ file = \"seeds/{}.mints.json\", count = {} }}\n",
                    pigs.creator().address(keys),
                    Authority::UpdateAuthority.address(keys),
                    pigs.slug(),
                    pigs.count()
                );
            }
            None => {
                let _ = write!(
                    toml,
                    "standard = \"core\"\nenabled = true\naddress = \"{}\"\n",
                    keys.address(&format!("collection/{}", pigs.slug()))
                );
            }
        }
    }
    toml
}

/// Copies the first pig of each eligible collection and every look-alike
/// into `dir`, in the `solana account --output json` format, with
/// `cases.json`: the membership rules and which cases the rule must accept.
pub async fn dump(chain: &Chain, dir: &Path, keys: &Keys) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let item = |label: &str, core: bool| {
        let address = keys.address(label);
        if core {
            address
        } else {
            build::metadata(&address)
        }
    };
    let mut cases = Vec::new();
    let mut files = Vec::new();
    for pigs in [Pigs::SolGang, Pigs::GirlGang, Pigs::PiggyGang] {
        let file = format!("{}-1", pigs.slug());
        files.push((file.clone(), item(&pigs.label(1), pigs == Pigs::PiggyGang)));
        cases.push(json!({
            "name": format!("{} #1", pigs.name()),
            "file": file,
            "rule_slot": pigs.rule_slot(),
            "accepted": true,
        }));
    }
    cases.push(json!({
        "name": "Test SOL Gang #1 under the Girl Gang rule",
        "file": format!("{}-1", Pigs::SolGang.slug()),
        "rule_slot": Pigs::GirlGang.rule_slot(),
        "accepted": false,
    }));
    for lookalike in Lookalike::ALL {
        let file = format!("lookalike-{}", lookalike.slug());
        files.push((
            file.clone(),
            item(&lookalike.label(), lookalike.imitates() == Pigs::PiggyGang),
        ));
        cases.push(json!({
            "name": format!("look-alike {}: {}", lookalike.slug(), lookalike.why()),
            "file": file,
            "rule_slot": lookalike.imitates().rule_slot(),
            "accepted": false,
        }));
    }

    let addresses: Vec<Pubkey> = files.iter().map(|(_, address)| *address).collect();
    let accounts = chain.accounts(&addresses).await?;
    for (file, address) in &files {
        let Some(account) = accounts.get(address) else {
            bail!("{address} ({file}) doesn't exist; run apply first");
        };
        let json = json!({
            "pubkey": address.to_string(),
            "account": {
                "lamports": account.lamports,
                "data": [BASE64.encode(&account.data), "base64"],
                "owner": account.owner.to_string(),
                "executable": account.executable,
                "rentEpoch": account.rent_epoch,
                "space": account.data.len(),
            },
        });
        std::fs::write(dir.join(format!("{file}.json")), format!("{:#}\n", json))?;
    }
    let rules = manifest(Cluster::Localnet, keys, &Spec::new())["membership_rules"].clone();
    let cases = json!({
        "written_by": "scripts/fixtures dump",
        "root": keys.root().to_string(),
        "membership_rules": rules,
        "cases": cases,
    });
    std::fs::write(dir.join("cases.json"), format!("{:#}\n", cases))?;
    std::fs::write(
        dir.join("README.md"),
        format!(
            "# Stand-ins\n\n\
             The devnet fixture set's structure, as `scripts/fixtures/localnet.sh` builds it on a \
             local validator (seed root `{}`), in the `solana account --output json` format. \
             Don't edit by hand.\n\n\
             `cases.json` names the membership rules and, for each account, the rule it is \
             claimed under and whether the rule accepts it: the first pig of each eligible \
             collection passes, and every look-alike fails.\n",
            keys.root()
        ),
    )?;
    Ok(())
}
