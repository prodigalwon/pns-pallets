use codec::{Encode, FullCodec};
use core::fmt::Debug;
use polkadot_sdk::frame_support::traits::Currency;
use pns_types::DomainHash;

use polkadot_sdk::sp_io::hashing::keccak_256;
use polkadot_sdk::sp_runtime::{
    traits::{AtLeast32BitUnsigned, MaybeSerializeDeserialize},
    DispatchError, DispatchResult,
};
use polkadot_sdk::sp_std::vec::Vec;

pub trait Registrar {
    type Balance;
    type AccountId;
    type Moment;
    fn check_expires_registrable(node: DomainHash) -> DispatchResult;
    fn check_expires_renewable(node: DomainHash) -> DispatchResult;
    fn check_expires_useable(node: DomainHash) -> DispatchResult;
    fn clear_registrar_info(node: DomainHash, owner: &Self::AccountId) -> DispatchResult;
    fn for_redeem_code(
        name: Vec<u8>,
        to: Self::AccountId,
        duration: Self::Moment,
        label: Label,
    ) -> DispatchResult;
    fn basenode() -> DomainHash;
    /// Returns `true` if `account` currently holds a valid (non-expired, within grace period)
    /// canonical name. Used by the registry to guard subdomain acceptance.
    fn has_valid_canonical_name(account: &Self::AccountId) -> bool;
}

/// 登记表
pub trait Registry: NFT<Self::AccountId> {
    type AccountId;

    fn mint_subname(
        node_owner: &Self::AccountId,
        node: DomainHash,
        label_node: DomainHash,
        to: Self::AccountId,
        capacity: u32,
        do_payments: impl FnOnce(Option<&Self::AccountId>) -> DispatchResult,
    ) -> DispatchResult;
    fn available(caller: &Self::AccountId, node: DomainHash) -> DispatchResult;
    fn owner_of(node: DomainHash) -> Option<Self::AccountId>;
    fn transfer(from: &Self::AccountId, to: &Self::AccountId, node: DomainHash) -> DispatchResult;
    fn burn(caller: Self::AccountId, node: DomainHash) -> DispatchResult;

    /// Returns `true` if `account` currently holds at least one active subdomain.
    /// Used by the registrar to block canonical name registration/transfer to accounts
    /// that already hold a subdomain.
    fn has_active_subname(account: &Self::AccountId) -> bool;

    /// Create an offer for a subdomain under `parent`.
    /// Performs depth check, duplicate check, and capacity increment.
    /// `capacity` is the parent's subdomain capacity from RegistrarInfo.
    fn offer_subname(
        parent: DomainHash,
        label_node: DomainHash,
        label_bytes: polkadot_sdk::frame_support::BoundedVec<u8, polkadot_sdk::sp_core::ConstU32<63>>,
        to: Self::AccountId,
        capacity: u32,
    ) -> DispatchResult;

    /// Accept a pending offer. Changes state to Active, updates AccountToSubnames.
    /// Returns the parent DomainHash on success.
    fn accept_subname_offer(
        label_node: pns_types::DomainHash,
        acceptor: &Self::AccountId,
    ) -> Result<pns_types::DomainHash, polkadot_sdk::sp_runtime::DispatchError>;

    /// Reject a pending offer. Changes state from Offered to Rejected.
    /// Removes from OfferedToAccount so it no longer appears as pending.
    fn reject_subname_offer(
        label_node: pns_types::DomainHash,
        caller: &Self::AccountId,
    ) -> DispatchResult;

    /// Revoke a subdomain (offered, rejected, or active) by the parent domain owner.
    /// Deletes the record and decrements the parent's children counter.
    fn revoke_subname(
        parent: DomainHash,
        label_node: DomainHash,
    ) -> DispatchResult;

    /// Release an active subdomain voluntarily by the holder.
    /// Deletes the record and decrements the parent's children counter.
    /// Returns parent DomainHash on success.
    fn release_subname(
        label_node: DomainHash,
        by: &Self::AccountId,
    ) -> Result<pns_types::DomainHash, polkadot_sdk::sp_runtime::DispatchError>;
}

/// Interface for the marketplace pallet to interact with name ownership.
pub trait NameRegistry {
    type AccountId;
    /// Returns the canonical name hash registered to `account`, if any.
    fn canonical_name(account: &Self::AccountId) -> Option<DomainHash>;
    /// Returns the current NFT owner of `node`, if any.
    fn owner_of(node: DomainHash) -> Option<Self::AccountId>;
    /// Transfers `node` from `from` to `to`, clearing the seller's canonical name entry.
    fn transfer_name(from: &Self::AccountId, to: &Self::AccountId, node: DomainHash) -> DispatchResult;
}

// 客户
pub trait Customer<AccountId> {
    // 客户使用的货币
    type Currency: Currency<AccountId>;
}

pub trait PriceOracle {
    type Moment;
    type Balance;
    /// Returns the price to register or renew a name.
    /// * `name`: The name being registered or renewed.
    /// * `expires`: When the name presently expires (0 if this is a new registration).
    /// * `duration`: How long the name is being registered or extended for, in seconds.
    /// return The price of this renewal or registration, in wei.
    fn renew_fee(name_len: usize, duration: Self::Moment) -> Option<Self::Balance>;
    fn register_fee(name_len: usize, duration: Self::Moment) -> Option<Self::Balance>;
    fn registration_fee(name_len: usize) -> Option<Self::Balance>;
}

/// Abstraction over a non-fungible token system.
#[allow(clippy::upper_case_acronyms)]
pub trait NFT<AccountId> {
    /// The NFT class identifier.
    type ClassId: Default + Copy;

    /// The NFT token identifier.
    type TokenId: Default + Copy;

    /// The balance of account.
    type Balance: AtLeast32BitUnsigned
        + FullCodec
        + Copy
        + MaybeSerializeDeserialize
        + Debug
        + Default;

    /// The number of NFTs assigned to `who`.
    fn balance(who: &AccountId) -> Self::Balance;

    /// The owner of the given token ID. Returns `None` if the token does not
    /// exist.
    fn owner(token: (Self::ClassId, Self::TokenId)) -> Option<AccountId>;

    /// Transfer the given token ID from one account to another.
    fn transfer(
        from: &AccountId,
        to: &AccountId,
        token: (Self::ClassId, Self::TokenId),
    ) -> DispatchResult;
}

pub struct Label {
    pub node: DomainHash,
}
pub const LABEL_MAX_LEN: usize = 63;
pub const LABEL_MIN_LEN: usize = 1;
pub const MIN_REGISTRABLE_LEN: usize = 1;

impl Label {
    pub fn new(data: &[u8]) -> Option<Self> {
        check_label(data)?;

        let normalized = core::str::from_utf8(data).ok()?.to_ascii_lowercase();
        let node = DomainHash::from(keccak_256(normalized.as_bytes()));
        Some(Self { node })
    }
    pub fn new_basenode(data: &[u8]) -> Option<Self> {
        check_label(data)?;

        let normalized = core::str::from_utf8(data).ok()?.to_ascii_lowercase();
        let node = DomainHash::from(keccak_256(normalized.as_bytes()));

        let encoded = &(DomainHash::default(), node).encode();
        let hash_encoded = keccak_256(encoded);

        Some(Self {
            node: DomainHash::from(hash_encoded),
        })
    }

    pub fn encode_with_name(&self, data: &[u8]) -> Option<Self> {
        let node = Self::new(data)?;
        Some(Label {
            node: self.encode_with_node(&node.node),
        })
    }

    pub fn encode_with_basename(&self, data: &[u8]) -> Option<Self> {
        let node = Self::new(data)?;
        Some(Label {
            node: self.encode_with_baselabel(&node.node),
        })
    }
    pub fn new_with_len(data: &[u8]) -> Option<(Self, usize)> {
        check_label(data)?;

        let normalized = core::str::from_utf8(data).ok()?.to_ascii_lowercase();
        let node = DomainHash::from(keccak_256(normalized.as_bytes()));
        Some((Self { node }, normalized.len()))
    }

    pub fn encode_with_baselabel(&self, baselabel: &DomainHash) -> DomainHash {
        let basenode = Self::basenode(baselabel);
        let encoded_again = &(basenode, &self.node).encode();

        DomainHash::from(keccak_256(encoded_again))
    }

    pub fn basenode(baselabel: &DomainHash) -> DomainHash {
        let encoded = &(DomainHash::default(), baselabel).encode();
        let hash_encoded = keccak_256(encoded);
        DomainHash::from(hash_encoded)
    }

    pub fn to_basenode(&self) -> DomainHash {
        Self::basenode(&self.node)
    }

    pub fn encode_with_node(&self, node: &DomainHash) -> DomainHash {
        let encoded = &(node, &self.node).encode();

        DomainHash::from(keccak_256(encoded))
    }
}
// TODO: (暂不支持中文域名)
// 域名不区分大小写和简繁体。
// 域名的合法长度为1~63个字符（域名主体，不包括后缀）。
// 英文域名合法字符为a-z、0-9、短划线（-）。
// （ 说明 短划线（-）不能出现在开头和结尾以及在第三和第四字符位置。）
// 中文域名除英文域名合法字符外，必须含有至少一个汉字（简体或繁体），计算中文域名字符长度以转换后的punycode码为准。
// 不支持xn—开头的请求参数（punycode码），请以中文域名作为请求参数。
pub fn check_label(label: &[u8]) -> Option<()> {
    let label = core::str::from_utf8(label)
        .map(|label| label.to_ascii_lowercase())
        .ok()?;

    if !(LABEL_MIN_LEN..=LABEL_MAX_LEN).contains(&label.len()) {
        return None;
    }

    if !label.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }

    Some(())
}
pub trait Available {
    fn is_anctionable(&self) -> bool;
    fn is_registrable(&self) -> bool;
}

impl Available for usize {
    fn is_anctionable(&self) -> bool {
        *self >= 1 && *self < MIN_REGISTRABLE_LEN
    }

    fn is_registrable(&self) -> bool {
        *self >= MIN_REGISTRABLE_LEN
    }
}

/// Writes the SS58 (owner account) record for a domain node.
///
/// Implemented in the runtime using `pns_resolvers::resolvers::Pallet`.
/// Called by registrar (on register), registry (on transfer), and marketplace (on buy).
pub trait Ss58Updater {
    type AccountId;
    fn update_ss58(node: DomainHash, owner: &Self::AccountId) -> DispatchResult;
}

/// Writes the ORIGIN record (block hash of registration block) for a domain node.
///
/// Implemented in the runtime using `pns_resolvers::resolvers::Pallet`.
/// Called only by the registrar on initial registration — never on renew/transfer.
pub trait OriginRecorder {
    fn record_origin(node: DomainHash, block_hash: [u8; 32]) -> DispatchResult;
}

/// Clears all DNS records for a domain node except the SS58 record.
///
/// Called on ownership transfers (registry transfer, marketplace buy) to prevent
/// a new owner from inheriting the previous owner's DNS records.
pub trait RecordCleaner {
    fn clear_records_except_ss58(node: DomainHash);
    /// Clears ALL DNS records for a domain node (Records, Accounts, Texts).
    ///
    /// Called when a domain is completely removed (e.g. a subname that is
    /// auto-cleared when its parent is transferred, released, or sold).
    fn clear_all_records(node: DomainHash);
}

pub trait ExchangeRate {
    type Balance;
    /// 1 USD to balance
    fn get_exchange_rate() -> Self::Balance;
}

pub trait Official {
    type AccountId;

    fn get_official_account() -> Result<Self::AccountId, DispatchError>;
}

pub trait IsRegistrarOpen {
    fn is_open() -> bool;
}
