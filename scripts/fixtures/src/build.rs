//! The instructions for each item, made the way mainnet's were. Token Metadata
//! NFTs come from `CreateV1` and `MintV1`, then a `Verify` signed by each
//! verified creator. Core items come from mpl-core's `CreateCollectionV2` and
//! `CreateV2`.

use {
    crate::{
        keys::Keys,
        spec::{
            Authority, CoreAsset, CoreCollection, Holder, TmNft, Token, SELLER_FEE_BASIS_POINTS,
        },
    },
    mpl_core::{
        instructions::{CreateCollectionV2Builder, CreateV2Builder, TransferV1Builder},
        types::{Creator as RoyaltyCreator, Plugin, PluginAuthorityPair, Royalties, RuleSet},
    },
    mpl_token_metadata::{
        accounts::{MasterEdition, Metadata},
        instructions::{
            CreateV1Builder, MintV1Builder, VerifyCollectionV1Builder, VerifyCreatorV1Builder,
        },
        types::{Collection, CollectionDetails, Creator, PrintSupply, TokenStandard},
    },
    solana_instruction::Instruction,
    solana_keypair::Keypair,
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    spl_associated_token_account_interface::{
        address::get_associated_token_address,
        instruction::create_associated_token_account_idempotent,
    },
};

/// Instructions for one transaction, with the keys that sign them besides
/// the payer.
pub struct Step {
    pub what: String,
    pub instructions: Vec<Instruction>,
    pub signers: Vec<Keypair>,
}

pub fn metadata(mint: &Pubkey) -> Pubkey {
    Metadata::find_pda(mint).0
}

pub fn master_edition(mint: &Pubkey) -> Pubkey {
    MasterEdition::find_pda(mint).0
}

/// The owner's associated token account for `mint`, where `MintV1` puts it.
pub fn token_account(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    get_associated_token_address(owner, mint)
}

/// Creates the NFT and mints it to its holder. Every creator starts
/// unverified; `verify_creator` follows for the verified ones.
pub fn create_tm_nft(keys: &Keys, nft: &TmNft, payer: &Pubkey) -> Step {
    let mint = keys.keypair(&nft.label);
    let authority = nft.update_authority.keypair(keys);
    let holder = nft.holder.address(keys);
    let (metadata, master_edition) = (metadata(&mint.pubkey()), master_edition(&mint.pubkey()));
    let creators = nft
        .creators
        .iter()
        .map(|&(creator, _, share)| Creator {
            address: creator.address(keys),
            verified: false,
            share,
        })
        .collect();
    let mut create = CreateV1Builder::new();
    create
        .metadata(metadata)
        .master_edition(Some(master_edition))
        .mint(mint.pubkey(), true)
        .authority(authority.pubkey())
        .payer(*payer)
        .update_authority(authority.pubkey(), true)
        .spl_token_program(Some(spl_token_interface::ID))
        .name(nft.name.clone())
        .symbol(nft.symbol.into())
        .uri(nft.uri.into())
        .seller_fee_basis_points(SELLER_FEE_BASIS_POINTS)
        .creators(creators)
        .is_mutable(true)
        .token_standard(TokenStandard::NonFungible)
        .print_supply(PrintSupply::Zero);
    if let Some((collection, _)) = &nft.collection {
        create.collection(Collection {
            verified: false,
            key: keys.address(collection),
        });
    }
    if nft.is_collection {
        create.collection_details(CollectionDetails::V1 { size: 0 });
    }
    let mint_to = MintV1Builder::new()
        .token(token_account(&holder, &mint.pubkey()))
        .token_owner(Some(holder))
        .metadata(metadata)
        .master_edition(Some(master_edition))
        .mint(mint.pubkey())
        .authority(authority.pubkey())
        .payer(*payer)
        .spl_token_program(spl_token_interface::ID)
        .spl_ata_program(spl_associated_token_account_interface::program::ID)
        .amount(1)
        .instruction();
    Step {
        what: format!("create {}", nft.name),
        instructions: vec![create.instruction(), mint_to],
        signers: vec![mint, authority],
    }
}

/// `creator` verifies itself on the NFT, the only signature that makes a
/// creator verified.
pub fn verify_creator(keys: &Keys, nft: &TmNft, creator: Authority) -> (Instruction, Keypair) {
    let instruction = VerifyCreatorV1Builder::new()
        .authority(creator.address(keys))
        .metadata(metadata(&keys.address(&nft.label)))
        .instruction();
    (instruction, creator.keypair(keys))
}

/// The collection's update authority verifies the NFT into it.
pub fn verify_collection(keys: &Keys, nft: &TmNft) -> Option<(Instruction, Keypair)> {
    let (collection, authority) = nft.collection.as_ref()?;
    let collection = keys.address(collection);
    let instruction = VerifyCollectionV1Builder::new()
        .authority(authority.address(keys))
        .metadata(metadata(&keys.address(&nft.label)))
        .collection_mint(collection)
        .collection_metadata(Some(metadata(&collection)))
        .collection_master_edition(Some(master_edition(&collection)))
        .instruction();
    Some((instruction, authority.keypair(keys)))
}

/// A Core collection with Piggy Gang's one plugin: Royalties without a rule
/// set (prize escrow §3).
pub fn create_core_collection(keys: &Keys, collection: &CoreCollection, payer: &Pubkey) -> Step {
    let key = keys.keypair(&collection.label);
    let royalties = Plugin::Royalties(Royalties {
        basis_points: SELLER_FEE_BASIS_POINTS,
        creators: vec![RoyaltyCreator {
            address: collection.update_authority.address(keys),
            percentage: 100,
        }],
        rule_set: RuleSet::None,
    });
    let instruction = CreateCollectionV2Builder::new()
        .collection(key.pubkey())
        .update_authority(Some(collection.update_authority.address(keys)))
        .payer(*payer)
        .name(collection.name.into())
        .uri(collection.uri.into())
        .plugins(vec![PluginAuthorityPair {
            plugin: royalties,
            authority: None,
        }])
        .instruction();
    Step {
        what: format!("create the {} collection", collection.name),
        instructions: vec![instruction],
        signers: vec![key],
    }
}

/// A Core asset owned by its holder: in its collection, under the
/// collection's update authority, or with that authority as its own.
pub fn create_core_asset(keys: &Keys, asset: &CoreAsset, payer: &Pubkey) -> Step {
    let key = keys.keypair(&asset.label);
    let mut create = CreateV2Builder::new();
    create
        .asset(key.pubkey())
        .payer(*payer)
        .owner(Some(asset.holder.address(keys)))
        .name(asset.name.clone())
        .uri(asset.uri.into());
    let mut signers = vec![key];
    match &asset.collection {
        Some(collection) => {
            create
                .collection(Some(keys.address(collection)))
                .authority(Some(asset.authority.address(keys)));
            signers.push(asset.authority.keypair(keys));
        }
        None => {
            create.update_authority(Some(asset.authority.address(keys)));
        }
    }
    Step {
        what: format!("create {}", asset.name),
        instructions: vec![create.instruction()],
        signers,
    }
}

/// The prize token's mint and metadata.
pub fn create_token(keys: &Keys, token: &Token, payer: &Pubkey) -> Step {
    let mint = keys.keypair(&token.label);
    let authority = token.mint_authority.keypair(keys);
    let instruction = CreateV1Builder::new()
        .metadata(metadata(&mint.pubkey()))
        .mint(mint.pubkey(), true)
        .authority(authority.pubkey())
        .payer(*payer)
        .update_authority(authority.pubkey(), true)
        .spl_token_program(Some(spl_token_interface::ID))
        .name(token.name.into())
        .symbol(token.symbol.into())
        .uri(token.uri.into())
        .seller_fee_basis_points(0)
        .is_mutable(true)
        .token_standard(TokenStandard::Fungible)
        .decimals(token.decimals)
        .instruction();
    Step {
        what: format!("create {}", token.name),
        instructions: vec![instruction],
        signers: vec![mint, authority],
    }
}

/// Mints `amount` base units of the prize token to `owner`.
pub fn mint_token(keys: &Keys, token: &Token, owner: &Pubkey, amount: u64, payer: &Pubkey) -> Step {
    let mint = keys.address(&token.label);
    let authority = token.mint_authority.keypair(keys);
    let instruction = MintV1Builder::new()
        .token(token_account(owner, &mint))
        .token_owner(Some(*owner))
        .metadata(metadata(&mint))
        .mint(mint)
        .authority(authority.pubkey())
        .payer(*payer)
        .spl_token_program(spl_token_interface::ID)
        .spl_ata_program(spl_associated_token_account_interface::program::ID)
        .amount(amount)
        .instruction();
    Step {
        what: format!("mint {} to {owner}", token.symbol),
        instructions: vec![instruction],
        signers: vec![authority],
    }
}

/// Moves a Token Metadata NFT from a fixture key back to its holder.
pub fn return_tm_nft(keys: &Keys, nft: &TmNft, from: &Keypair, payer: &Pubkey) -> Step {
    let mint = keys.address(&nft.label);
    let holder = nft.holder.address(keys);
    let instructions = vec![
        create_associated_token_account_idempotent(payer, &holder, &mint, &spl_token_interface::ID),
        spl_token_interface::instruction::transfer_checked(
            &spl_token_interface::ID,
            &token_account(&from.pubkey(), &mint),
            &mint,
            &token_account(&holder, &mint),
            &from.pubkey(),
            &[],
            1,
            0,
        )
        .unwrap_or_else(|err| unreachable!("a valid transfer: {err}")),
    ];
    Step {
        what: format!("return {} to {}", nft.name, nft.holder.label()),
        instructions,
        signers: vec![from.insecure_clone()],
    }
}

/// Moves a Core asset from a fixture key back to its holder.
pub fn return_core_asset(keys: &Keys, asset: &CoreAsset, from: &Keypair, payer: &Pubkey) -> Step {
    let instruction = TransferV1Builder::new()
        .asset(keys.address(&asset.label))
        .collection(
            asset
                .collection
                .as_ref()
                .map(|collection| keys.address(collection)),
        )
        .payer(*payer)
        .authority(Some(from.pubkey()))
        .new_owner(asset.holder.address(keys))
        .instruction();
    Step {
        what: format!("return {} to {}", asset.name, asset.holder.label()),
        instructions: vec![instruction],
        signers: vec![from.insecure_clone()],
    }
}

/// Every key the spec can hold an item with, for returning strays.
pub fn fixture_holders(keys: &Keys) -> Vec<(Pubkey, Keypair)> {
    crate::spec::Wallet::ALL
        .into_iter()
        .map(Holder::Wallet)
        .chain(Authority::ALL.into_iter().map(Holder::Authority))
        .map(|holder| (holder.address(keys), holder.keypair(keys)))
        .collect()
}
