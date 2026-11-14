//! The fixture set: what the cluster holds once `apply` has run. It mirrors
//! mainnet's structure (holder-proof §3 and §7). That means two legacy Token
//! Metadata collections without a certified collection, a Core collection, a
//! Pig Mud stand-in, a look-alike for each way the rule rejects an NFT, a prize
//! of each allowlist kind, and the wallets the tests sign as.

use {crate::keys::Keys, solana_keypair::Keypair, solana_pubkey::Pubkey, solana_signer::Signer};

/// Mainnet's pigs carry this royalty, and so do the stand-ins.
pub const SELLER_FEE_BASIS_POINTS: u16 = 500;
const LAMPORTS_PER_SOL: u64 = 1_000_000_000;

// Real, re-hosted metadata JSON, so the devnet indexer's backfill finds an
// image for every stand-in. The rules read only the on-chain fields.
const SOL_GANG_URI: &str = "https://pub-b1a45763f0a64d8fa271f66f5514a561.r2.dev/old/metadata/CVqmUYDPX4FgVjoTCLASdt4Nz3TUgKEV8KCYoxrwcUuv.json";
const GIRL_GANG_URI: &str = "https://rk2cjjujjgvqwsmy.public.blob.vercel-storage.com/piggy-girl-gang-json/Dk1cJAAYPMa6Y39Qs31Jo8EofTJqJFFE62FXDdtyRQmN.json";
const PIGGY_GANG_URI: &str =
    "https://pub-b1a45763f0a64d8fa271f66f5514a561.r2.dev/metadata/a5fe6350fb7dacd04fc9.json";

/// Keys that sign but hold nothing to test with: the stand-ins for mainnet's
/// authorities. `apply`'s payer pays every rent, so none of them needs SOL.
// The variants are named for mainnet's roles, "update authority" included.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Authority {
    /// Stands in for `Pigv3g…`, the verified creator of both legacy
    /// collections.
    Creator,
    /// Stands in for `7Nsng…`: the update authority of the legacy pigs, of Pig
    /// Mud and of the Core collection.
    UpdateAuthority,
    /// Stands in for `5uJo…`, Pig Mud's verified creator.
    PigMudCreator,
    /// A key no rule names: the wrong update authority, and the authority of a
    /// Core collection that isn't Piggy Gang.
    Stranger,
    /// Update authority of the prize collections, mint authority of the prize
    /// token.
    PrizeAuthority,
}

impl Authority {
    pub const ALL: [Self; 5] = [
        Self::Creator,
        Self::UpdateAuthority,
        Self::PigMudCreator,
        Self::Stranger,
        Self::PrizeAuthority,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Creator => "creator",
            Self::UpdateAuthority => "update-authority",
            Self::PigMudCreator => "pig-mud-creator",
            Self::Stranger => "stranger",
            Self::PrizeAuthority => "prize-authority",
        }
    }

    pub fn keypair(self, keys: &Keys) -> Keypair {
        keys.keypair(&format!("authority/{}", self.label()))
    }

    pub fn address(self, keys: &Keys) -> Pubkey {
        self.keypair(keys).pubkey()
    }
}

/// The funded wallets the tests and the E2E run sign as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wallet {
    SingleHolder,
    FullGangHolder,
    Whale,
    Seller,
    Buyer,
    Creator,
    GmBot,
    Crank,
    Admin,
    Impostor,
    Reserve,
}

impl Wallet {
    pub const ALL: [Self; 11] = [
        Self::SingleHolder,
        Self::FullGangHolder,
        Self::Whale,
        Self::Seller,
        Self::Buyer,
        Self::Creator,
        Self::GmBot,
        Self::Crank,
        Self::Admin,
        Self::Impostor,
        Self::Reserve,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::SingleHolder => "single-holder",
            Self::FullGangHolder => "full-gang-holder",
            Self::Whale => "whale",
            Self::Seller => "seller",
            Self::Buyer => "buyer",
            Self::Creator => "creator",
            Self::GmBot => "gm-bot",
            Self::Crank => "crank",
            Self::Admin => "admin",
            Self::Impostor => "impostor",
            Self::Reserve => "reserve",
        }
    }

    pub fn purpose(self) -> &'static str {
        match self {
            Self::SingleHolder => "One SOL Gang pig: the plainest claim.",
            Self::FullGangHolder => "Pigs of all three collections: the Full Gang bonus (E-E7).",
            Self::Whale => {
                "30 eligible pigs, three times the cap, and two Pig Mud (E-E1, E-E14, E-E16)."
            }
            Self::Seller => "Enters pigs, then sells one to the buyer (transfer-and-reclaim, E-E9).",
            Self::Buyer => "Buys an entered pig from the seller; holds one SOL Gang pig of its own.",
            Self::Creator => {
                "An approved partner with prizes, and a pig it can't enter in its own raffle (E-E12)."
            }
            Self::GmBot => "The GM bot: GM prizes and prize tokens.",
            Self::Crank => "Pays for the draw and the closes.",
            Self::Admin => {
                "The devnet config authority and DAO admin: Core prizes, DressMe vouchers, prize \
                 tokens."
            }
            Self::Impostor => "Every look-alike, next to one real SOL Gang pig.",
            Self::Reserve => "The pigs nobody else holds, for ad-hoc tests.",
        }
    }

    /// The balance `apply` tops the wallet up to.
    pub fn funding(self) -> u64 {
        match self {
            Self::Creator | Self::GmBot | Self::Admin => LAMPORTS_PER_SOL,
            Self::Crank => LAMPORTS_PER_SOL / 2,
            Self::Whale => LAMPORTS_PER_SOL / 4,
            Self::Reserve => LAMPORTS_PER_SOL / 20,
            Self::SingleHolder
            | Self::FullGangHolder
            | Self::Seller
            | Self::Buyer
            | Self::Impostor => LAMPORTS_PER_SOL / 10,
        }
    }

    pub fn keypair(self, keys: &Keys) -> Keypair {
        keys.keypair(&format!("wallet/{}", self.label()))
    }

    pub fn address(self, keys: &Keys) -> Pubkey {
        self.keypair(keys).pubkey()
    }
}

/// The stand-in collections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pigs {
    SolGang,
    GirlGang,
    PiggyGang,
    /// Not eligible, as on mainnet: it shares the update authority but not
    /// the creator.
    PigMud,
}

impl Pigs {
    pub const ALL: [Self; 4] = [Self::SolGang, Self::GirlGang, Self::PiggyGang, Self::PigMud];

    pub fn slug(self) -> &'static str {
        match self {
            Self::SolGang => "test-sol-gang",
            Self::GirlGang => "test-girl-gang",
            Self::PiggyGang => "test-piggy-gang",
            Self::PigMud => "test-pig-mud",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::SolGang => "Test SOL Gang",
            Self::GirlGang => "Test Girl Gang",
            Self::PiggyGang => "Test Piggy Gang",
            Self::PigMud => "Test Pig Mud",
        }
    }

    pub fn count(self) -> u32 {
        match self {
            Self::SolGang | Self::PiggyGang => 60,
            Self::GirlGang => 40,
            Self::PigMud => 10,
        }
    }

    /// The Token Metadata symbol; the Core collection has none.
    pub fn symbol(self) -> Option<&'static str> {
        match self {
            Self::SolGang => Some("PSG"),
            Self::GirlGang => Some("PGG"),
            Self::PiggyGang => None,
            Self::PigMud => Some("PMUD"),
        }
    }

    /// Its slot in the membership rules. Pig Mud has none, as on mainnet.
    pub fn rule_slot(self) -> Option<u8> {
        match self {
            Self::SolGang => Some(0),
            Self::GirlGang => Some(1),
            Self::PiggyGang => Some(2),
            Self::PigMud => None,
        }
    }

    pub fn uri(self) -> &'static str {
        match self {
            Self::SolGang | Self::PigMud => SOL_GANG_URI,
            Self::GirlGang => GIRL_GANG_URI,
            Self::PiggyGang => PIGGY_GANG_URI,
        }
    }

    /// The Token Metadata verified creator.
    pub fn creator(self) -> Authority {
        match self {
            Self::PigMud => Authority::PigMudCreator,
            _ => Authority::Creator,
        }
    }

    /// The key label of its `index`-th item (from 1): a mint or a Core asset.
    pub fn label(self, index: u32) -> String {
        let kind = if self == Self::PiggyGang {
            "asset"
        } else {
            "mint"
        };
        format!("{kind}/{}/{index}", self.slug())
    }

    fn column(self) -> usize {
        match self {
            Self::SolGang => 0,
            Self::GirlGang => 1,
            Self::PiggyGang => 2,
            Self::PigMud => 3,
        }
    }
}

/// Who starts with which pigs: SOL Gang, Girl Gang, Piggy Gang, Pig Mud. The
/// reserve holds the rest.
const HOLDINGS: [(Wallet, [u32; 4]); 7] = [
    (Wallet::SingleHolder, [1, 0, 0, 0]),
    (Wallet::FullGangHolder, [2, 1, 1, 0]),
    (Wallet::Whale, [12, 8, 10, 2]),
    (Wallet::Seller, [2, 2, 2, 0]),
    (Wallet::Buyer, [1, 0, 0, 0]),
    (Wallet::Creator, [1, 0, 0, 0]),
    (Wallet::Impostor, [1, 0, 0, 1]),
];

/// The holder of the `index`-th pig (from 1) of `pigs`.
pub fn holder(pigs: Pigs, index: u32) -> Wallet {
    let mut first = 1;
    for (wallet, counts) in HOLDINGS {
        let count = counts[pigs.column()];
        if index < first + count {
            return wallet;
        }
        first += count;
    }
    Wallet::Reserve
}

/// The NFTs that pass for pigs but fail the rule, one per rejection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lookalike {
    UnverifiedCreator,
    WrongSymbol,
    WrongUpdateAuthority,
    Honorary,
    PigMud,
    OtherCollection,
    AddressUpdateAuthority,
}

impl Lookalike {
    pub const ALL: [Self; 7] = [
        Self::UnverifiedCreator,
        Self::WrongSymbol,
        Self::WrongUpdateAuthority,
        Self::Honorary,
        Self::PigMud,
        Self::OtherCollection,
        Self::AddressUpdateAuthority,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Self::UnverifiedCreator => "unverified-creator",
            Self::WrongSymbol => "wrong-symbol",
            Self::WrongUpdateAuthority => "wrong-update-authority",
            Self::Honorary => "honorary",
            Self::PigMud => "pig-mud",
            Self::OtherCollection => "other-collection",
            Self::AddressUpdateAuthority => "address-update-authority",
        }
    }

    /// The collection it passes itself off as: the rule it's claimed under.
    pub fn imitates(self) -> Pigs {
        match self {
            Self::OtherCollection | Self::AddressUpdateAuthority => Pigs::PiggyGang,
            _ => Pigs::SolGang,
        }
    }

    /// Why the rule rejects it.
    pub fn why(self) -> &'static str {
        match self {
            Self::UnverifiedCreator => {
                "The creator is present but unverified, like mainnet's #293."
            }
            Self::WrongSymbol => "The right creator and update authority, but symbol PSGX.",
            Self::WrongUpdateAuthority => {
                "The creator verified and symbol PSG, but another update authority."
            }
            Self::Honorary => {
                "The creator verified, but as its own update authority and with symbol HPSG, like \
                 Honorary Piggy 1."
            }
            Self::PigMud => {
                "A Test Pig Mud pig: the same update authority, another verified creator."
            }
            Self::OtherCollection => "A Core asset in another collection.",
            Self::AddressUpdateAuthority => "A Core asset whose update authority is an address.",
        }
    }

    /// The key label of its mint or asset. Pig Mud's is the impostor's Test Pig
    /// Mud pig.
    pub fn label(self) -> String {
        match self {
            Self::PigMud => Pigs::PigMud.label(first_held(Pigs::PigMud, Wallet::Impostor)),
            Self::OtherCollection | Self::AddressUpdateAuthority => {
                format!("asset/lookalike/{}", self.slug())
            }
            _ => format!("mint/lookalike/{}", self.slug()),
        }
    }
}

/// The first pig of `pigs` that `wallet` holds.
fn first_held(pigs: Pigs, wallet: Wallet) -> u32 {
    (1..=pigs.count())
        .find(|&index| holder(pigs, index) == wallet)
        .unwrap_or_else(|| unreachable!("HOLDINGS gives {wallet:?} a {pigs:?} pig"))
}

/// What holds an item: a wallet, or an authority holding its collection's NFT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holder {
    Wallet(Wallet),
    Authority(Authority),
}

impl Holder {
    pub fn keypair(self, keys: &Keys) -> Keypair {
        match self {
            Self::Wallet(wallet) => wallet.keypair(keys),
            Self::Authority(authority) => authority.keypair(keys),
        }
    }

    pub fn address(self, keys: &Keys) -> Pubkey {
        self.keypair(keys).pubkey()
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Wallet(wallet) => wallet.label(),
            Self::Authority(authority) => authority.label(),
        }
    }
}

/// A Token Metadata NFT: a pig, a look-alike, a prize or a collection's NFT.
pub struct TmNft {
    /// Its mint's key label.
    pub label: String,
    pub name: String,
    pub symbol: &'static str,
    pub uri: &'static str,
    pub update_authority: Authority,
    /// Address, verified, share. They start unverified, and each one the spec
    /// verifies signs a `Verify`, as on mainnet.
    pub creators: Vec<(Authority, bool, u8)>,
    /// The certified collection it belongs to: the collection NFT's label and
    /// update authority.
    pub collection: Option<(String, Authority)>,
    /// Whether it is a certified collection's NFT.
    pub is_collection: bool,
    pub holder: Holder,
}

pub struct CoreCollection {
    pub label: String,
    pub name: &'static str,
    pub uri: &'static str,
    pub update_authority: Authority,
}

pub struct CoreAsset {
    pub label: String,
    pub name: String,
    pub uri: &'static str,
    /// The collection's label, or `None` for an asset whose update authority
    /// is an address.
    pub collection: Option<String>,
    /// The collection's update authority, which signs the create, or the
    /// asset's own update authority.
    pub authority: Authority,
    pub holder: Wallet,
}

/// A fungible SPL token with Token Metadata, the D11 prize.
pub struct Token {
    pub label: String,
    pub name: &'static str,
    pub symbol: &'static str,
    pub uri: &'static str,
    pub decimals: u8,
    pub mint_authority: Authority,
    /// Whole tokens each wallet holds.
    pub balances: Vec<(Wallet, u64)>,
}

/// A certified Token Metadata collection of prizes: kind 0 on the allowlist,
/// and kind 1 for any single member.
pub struct PrizeCollection {
    pub slug: &'static str,
    pub name: &'static str,
    pub member_name: &'static str,
    pub symbol: &'static str,
    pub uri: &'static str,
    pub members: &'static [(Wallet, u32)],
}

pub const GM_PRIZES: PrizeCollection = PrizeCollection {
    slug: "test-gm-prizes",
    name: "Test GM Prizes",
    member_name: "Test GM Prize",
    symbol: "TGMP",
    uri: GIRL_GANG_URI,
    members: &[(Wallet::GmBot, 6), (Wallet::Creator, 4)],
};

/// The DressMe crossover's voucher NFTs (spec D8).
pub const VOUCHERS: PrizeCollection = PrizeCollection {
    slug: "test-dressme-vouchers",
    name: "Test DressMe Vouchers",
    member_name: "Test DressMe Voucher",
    symbol: "TDMV",
    uri: SOL_GANG_URI,
    members: &[(Wallet::Admin, 3)],
};

/// Core prizes: kind 2 on the allowlist (spec D22).
pub const CORE_PRIZES: (&str, &str, &[(Wallet, u32)]) = (
    "test-core-prizes",
    "Test Core Prizes",
    &[(Wallet::Admin, 2), (Wallet::Creator, 2)],
);

/// A Core collection that isn't Piggy Gang, for a look-alike.
pub const NOT_PIGGY: (&str, &str) = ("test-not-piggy", "Test Not Piggy");

/// Everything `apply` creates, in the order it creates it.
pub struct Spec {
    pub core_collections: Vec<CoreCollection>,
    /// Collection NFTs first: members are verified into them.
    pub tm_nfts: Vec<TmNft>,
    pub core_assets: Vec<CoreAsset>,
    pub token: Token,
}

impl Spec {
    pub fn new() -> Self {
        let mut spec = Self {
            core_collections: vec![
                core_collection(
                    Pigs::PiggyGang.slug(),
                    Pigs::PiggyGang.name(),
                    Authority::UpdateAuthority,
                ),
                core_collection(NOT_PIGGY.0, NOT_PIGGY.1, Authority::Stranger),
                core_collection(CORE_PRIZES.0, CORE_PRIZES.1, Authority::PrizeAuthority),
            ],
            tm_nfts: Vec::new(),
            core_assets: Vec::new(),
            token: Token {
                label: "mint/test-prize-token".into(),
                name: "Test Prize Token",
                symbol: "TPRZ",
                uri: SOL_GANG_URI,
                decimals: 6,
                mint_authority: Authority::PrizeAuthority,
                balances: vec![
                    (Wallet::Creator, 1_000),
                    (Wallet::GmBot, 1_000),
                    (Wallet::Admin, 1_000),
                ],
            },
        };

        for prizes in [GM_PRIZES, VOUCHERS] {
            spec.tm_nfts.push(TmNft {
                label: format!("mint/{}", prizes.slug),
                name: prizes.name.into(),
                symbol: prizes.symbol,
                uri: prizes.uri,
                update_authority: Authority::PrizeAuthority,
                creators: vec![(Authority::PrizeAuthority, true, 100)],
                collection: None,
                is_collection: true,
                holder: Holder::Authority(Authority::PrizeAuthority),
            });
        }

        for pigs in [Pigs::SolGang, Pigs::GirlGang, Pigs::PigMud] {
            for index in 1..=pigs.count() {
                spec.tm_nfts.push(TmNft {
                    label: pigs.label(index),
                    name: format!("{} #{index}", pigs.name()),
                    symbol: pigs.symbol().unwrap_or_default(),
                    uri: pigs.uri(),
                    update_authority: Authority::UpdateAuthority,
                    // Mainnet's layout: the creator verified with share 0,
                    // the update authority unverified with share 100.
                    creators: vec![
                        (pigs.creator(), true, 0),
                        (Authority::UpdateAuthority, false, 100),
                    ],
                    collection: None,
                    is_collection: false,
                    holder: Holder::Wallet(holder(pigs, index)),
                });
            }
        }
        for index in 1..=Pigs::PiggyGang.count() {
            spec.core_assets.push(CoreAsset {
                label: Pigs::PiggyGang.label(index),
                name: format!("{} #{index}", Pigs::PiggyGang.name()),
                uri: Pigs::PiggyGang.uri(),
                collection: Some(format!("collection/{}", Pigs::PiggyGang.slug())),
                authority: Authority::UpdateAuthority,
                holder: holder(Pigs::PiggyGang, index),
            });
        }

        for lookalike in Lookalike::ALL {
            let impostor = Holder::Wallet(Wallet::Impostor);
            let tm = |name: &str, symbol, update_authority, creators| TmNft {
                label: lookalike.label(),
                name: name.into(),
                symbol,
                uri: SOL_GANG_URI,
                update_authority,
                creators,
                collection: None,
                is_collection: false,
                holder: impostor,
            };
            let core = |name: &str, collection: Option<String>, authority| CoreAsset {
                label: lookalike.label(),
                name: name.into(),
                uri: PIGGY_GANG_URI,
                collection,
                authority,
                holder: Wallet::Impostor,
            };
            use Authority::{Creator, Stranger, UpdateAuthority};
            match lookalike {
                Lookalike::UnverifiedCreator => spec.tm_nfts.push(tm(
                    "Look-alike: unverified creator",
                    "PSG",
                    UpdateAuthority,
                    vec![(Creator, false, 0), (UpdateAuthority, false, 100)],
                )),
                Lookalike::WrongSymbol => spec.tm_nfts.push(tm(
                    "Look-alike: wrong symbol",
                    "PSGX",
                    UpdateAuthority,
                    vec![(Creator, true, 0), (UpdateAuthority, false, 100)],
                )),
                Lookalike::WrongUpdateAuthority => spec.tm_nfts.push(tm(
                    "Look-alike: wrong authority",
                    "PSG",
                    Stranger,
                    vec![(Creator, true, 0), (Stranger, false, 100)],
                )),
                Lookalike::Honorary => spec.tm_nfts.push(tm(
                    "Look-alike: honorary",
                    "HPSG",
                    Creator,
                    vec![(Creator, true, 100)],
                )),
                // A Test Pig Mud pig, already in the set.
                Lookalike::PigMud => {}
                Lookalike::OtherCollection => spec.core_assets.push(core(
                    "Look-alike: other collection",
                    Some(format!("collection/{}", NOT_PIGGY.0)),
                    Stranger,
                )),
                Lookalike::AddressUpdateAuthority => {
                    spec.core_assets
                        .push(core("Look-alike: no collection", None, UpdateAuthority))
                }
            }
        }

        for prizes in [GM_PRIZES, VOUCHERS] {
            let mut index = 0;
            for &(wallet, count) in prizes.members {
                for _ in 0..count {
                    index += 1;
                    spec.tm_nfts.push(TmNft {
                        label: format!("mint/{}/{index}", prizes.slug),
                        name: format!("{} #{index}", prizes.member_name),
                        symbol: prizes.symbol,
                        uri: prizes.uri,
                        update_authority: Authority::PrizeAuthority,
                        creators: vec![(Authority::PrizeAuthority, true, 100)],
                        collection: Some((
                            format!("mint/{}", prizes.slug),
                            Authority::PrizeAuthority,
                        )),
                        is_collection: false,
                        holder: Holder::Wallet(wallet),
                    });
                }
            }
        }
        let mut index = 0;
        for &(wallet, count) in CORE_PRIZES.2 {
            for _ in 0..count {
                index += 1;
                spec.core_assets.push(CoreAsset {
                    label: format!("asset/{}/{index}", CORE_PRIZES.0),
                    name: format!("Test Core Prize #{index}"),
                    uri: PIGGY_GANG_URI,
                    collection: Some(format!("collection/{}", CORE_PRIZES.0)),
                    authority: Authority::PrizeAuthority,
                    holder: wallet,
                });
            }
        }
        spec
    }

    /// The lamports a fresh run moves to the wallets.
    pub fn funding(&self) -> u64 {
        Wallet::ALL.iter().map(|wallet| wallet.funding()).sum()
    }
}

fn core_collection(slug: &str, name: &'static str, update_authority: Authority) -> CoreCollection {
    CoreCollection {
        label: format!("collection/{slug}"),
        name,
        uri: PIGGY_GANG_URI,
        update_authority,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holdings_fit_the_collections() {
        for pigs in Pigs::ALL {
            let assigned: u32 = HOLDINGS
                .iter()
                .map(|(_, counts)| counts[pigs.column()])
                .sum();
            assert!(assigned <= pigs.count(), "{pigs:?}");
        }
        let whale = |pigs| {
            (1..=Pigs::count(pigs))
                .filter(|&i| holder(pigs, i) == Wallet::Whale)
                .count()
        };
        let eligible = whale(Pigs::SolGang) + whale(Pigs::GirlGang) + whale(Pigs::PiggyGang);
        assert!(eligible >= 25, "the whale holds {eligible} eligible pigs");
    }

    #[test]
    fn labels_are_unique() {
        let spec = Spec::new();
        let mut labels: Vec<&str> = spec
            .tm_nfts
            .iter()
            .map(|nft| nft.label.as_str())
            .chain(spec.core_assets.iter().map(|asset| asset.label.as_str()))
            .chain(
                spec.core_collections
                    .iter()
                    .map(|collection| collection.label.as_str()),
            )
            .chain([spec.token.label.as_str()])
            .collect();
        let total = labels.len();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), total);
    }

    #[test]
    fn names_and_symbols_fit_token_metadata() {
        for nft in Spec::new().tm_nfts {
            assert!(nft.name.len() <= 32, "{}", nft.name);
            assert!(nft.symbol.len() <= 10, "{}", nft.symbol);
            assert!(nft.uri.len() <= 200, "{}", nft.uri);
        }
    }
}
