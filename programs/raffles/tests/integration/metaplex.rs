//! The mainnet fixtures load and execute under LiteSVM's mainnet feature set.
//! Raffle tests build their NFTs, Core assets and collections the same way.

use {
    crate::harness::Test,
    anchor_spl::metadata::mpl_token_metadata::{
        self,
        accounts::{MasterEdition, Metadata},
        instructions::CreateV1Builder,
        types::{PrintSupply, TokenStandard},
    },
    mpl_core::{accounts::BaseAssetV1, instructions::CreateV2Builder},
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const URI: &str = "https://example.com/nft.json";

#[test]
fn token_metadata_creates_an_nft() {
    let mut test = Test::new();
    let payer = test.payer.pubkey();
    let mint = Keypair::new();
    let (metadata, _) = Metadata::find_pda(&mint.pubkey());
    let (master_edition, _) = MasterEdition::find_pda(&mint.pubkey());
    let instruction = CreateV1Builder::new()
        .metadata(metadata)
        .master_edition(Some(master_edition))
        .mint(mint.pubkey(), true)
        .authority(payer)
        .payer(payer)
        .update_authority(payer, true)
        .spl_token_program(Some(anchor_spl::token::ID))
        .name("Test NFT".into())
        .uri(URI.into())
        .seller_fee_basis_points(0)
        .token_standard(TokenStandard::NonFungible)
        .print_supply(PrintSupply::Zero)
        .instruction();

    test.send(&[instruction], &[&mint]);

    let account = test.svm.get_account(&metadata).unwrap();
    assert_eq!(account.owner, mpl_token_metadata::ID);
    assert_eq!(
        Metadata::from_bytes(&account.data).unwrap().mint,
        mint.pubkey()
    );
}

#[test]
fn mpl_core_creates_an_asset() {
    let mut test = Test::new();
    let asset = Keypair::new();
    let instruction = CreateV2Builder::new()
        .asset(asset.pubkey())
        .payer(test.payer.pubkey())
        .name("Test asset".into())
        .uri(URI.into())
        .instruction();

    test.send(&[instruction], &[&asset]);

    let account = test.svm.get_account(&asset.pubkey()).unwrap();
    assert_eq!(account.owner, mpl_core::ID);
    assert_eq!(
        BaseAssetV1::from_bytes(&account.data).unwrap().owner,
        test.payer.pubkey()
    );
}
