use rustc_serialize::base64::{self, ToBase64};
use time::now_utc;

use crypto::{HashDigest, PublicKey, SecretKey, gen_keypair, hash, hash_message,
             sign_message};
use error::{SimplesError, SimplesResult};
use simples_pb::{Block, BlockPatch, HashedBlock, SignedBlock, Transaction};
use tx::{TransactionBuilder, TransactionExt};

fn create_genesis_block(tx: Transaction) -> SimplesResult<HashedBlock> {
    if tx.get_commit().get_bounty() != 0 || tx.get_commit().has_bounty_pk() {
        return Err(SimplesError::new(
            "Transactions must not have a bounty set in a genesis block."));
    }
    try!(tx.verify_signatures());
    let mut genesis = HashedBlock::new();
    genesis.mut_signed_block().mut_block().mut_transactions().push(tx);
    genesis.mut_signed_block().mut_block().set_previous(
        HashDigest::from_u64(0).0.to_vec());
    genesis.mut_signed_block().mut_block().set_timestamp(
        now_utc().to_timespec().sec);
    genesis.compute_hash();
    Ok(genesis)
}

pub struct GenesisBuilder {
    transfers: Vec<(PublicKey, u64)>
}

impl GenesisBuilder {
    pub fn new() -> GenesisBuilder {
        GenesisBuilder {
            transfers: vec![]
        }
    }

    pub fn add_transfer(&mut self, destination: PublicKey, tokens: u64) {
        self.transfers.push((destination, tokens));
    }

    pub fn build(self) -> HashedBlock {
        let (public_key, secret_key) = gen_keypair();
        let mut tx_builder = TransactionBuilder::new();
        let mut op_num = 0u32;
        for (destination, tokens) in self.transfers.into_iter() {
            tx_builder.add_transfer(
                &secret_key, &public_key, &destination, tokens, op_num);
            op_num += 1;
        }
        let genesis_tx = tx_builder.build().unwrap();
        assert!(genesis_tx.verify_signatures().is_ok());
        create_genesis_block(genesis_tx).unwrap()
    }
}

pub trait HashedBlockExt {
    fn compute_hash(&mut self) -> HashDigest;
    fn decode_hash(&self) -> SimplesResult<HashDigest>;
    fn decode_previous(&self) -> SimplesResult<HashDigest>;
    fn get_block<'a>(&'a self) -> &'a Block;
    fn set_previous_block(&mut self, block_hash: &HashDigest);
    fn verify_hash(&self) -> SimplesResult<()>;
    fn verify(&self) -> SimplesResult<()>;
}

impl HashedBlockExt for HashedBlock {
    fn compute_hash(&mut self) -> HashDigest {
        let hash_digest = hash_message(self.get_signed_block());
        self.set_hash(hash_digest.0.to_vec());
        hash_digest
    }

    fn decode_hash(&self) -> SimplesResult<HashDigest> {
        HashDigest::from_bytes(self.get_hash())
    }

    fn decode_previous(&self) -> SimplesResult<HashDigest> {
        HashDigest::from_bytes(self.get_block().get_previous())
    }

    fn get_block<'a>(&'a self) -> &'a Block {
        self.get_signed_block().get_block()
    }

    fn set_previous_block(&mut self, block_hash: &HashDigest) {
        self.mut_signed_block().mut_block().set_previous(block_hash.0.to_vec())
    }

    fn verify_hash(&self) -> SimplesResult<()> {
        let block_hash = try!(HashDigest::from_bytes(&self.get_hash()[]));
        try!(HashDigest::from_bytes(self.get_block().get_previous()));

        let computed_hash = hash_message(self.get_signed_block());
        if computed_hash == block_hash { Ok(()) }
        else { Err(SimplesError::new(&format!(
            "Block has invalid hash: {} != {} (actual)",
            block_hash, computed_hash)[]))
        }
    }

    fn verify(&self) -> SimplesResult<()> {
        try!(self.verify_hash());
        try!(self.get_signed_block().verify_signature());
        let txes = self.get_block().get_transactions();
        for tx in txes { try!(tx.verify_signatures()); }
        Ok(())
    }
}

pub trait SignedBlockExt {
    fn sign(&mut self, secret_key: &SecretKey);
    fn verify_signature(&self) -> SimplesResult<()>;
}

impl SignedBlockExt for SignedBlock {
    fn sign(&mut self, secret_key: &SecretKey) {
        let signature = sign_message(secret_key, self.get_block());
        self.set_signature(signature.0.to_vec());
    }

    fn verify_signature(&self) -> SimplesResult<()> {
        Ok(())
    }
}

pub trait BlockPatchExt {
    fn decode_previous(&self) -> SimplesResult<HashDigest>;
    fn encode_previous(&mut self, previous: &HashDigest);
}

impl BlockPatchExt for BlockPatch {
    fn decode_previous(&self) -> SimplesResult<HashDigest> {
        HashDigest::from_bytes(self.get_previous())
    }

    fn encode_previous(&mut self, previous: &HashDigest) {
        self.set_previous(previous.0.to_vec());
    }
}

#[test]
fn test_create_genesis_empty() {
    let tx = Transaction::new();
    let maybe_genesis = create_genesis_block(tx);
    assert!(maybe_genesis.is_ok());
}

#[test]
fn test_create_genesis_with_invalid_tx() {
    let (pk1, sk1) = gen_keypair();
    let (pk2, sk2) = gen_keypair();

    let mut tx_builder = TransactionBuilder::new();
    tx_builder.add_transfer(&sk1, &pk1, &pk2, 10, 0);
    let maybe_tx = tx_builder.build();
    assert!(maybe_tx.is_ok());
    let mut tx = maybe_tx.unwrap();
    assert!(create_genesis_block(tx.clone()).is_ok());
    tx.clear_signatures();
    assert!(create_genesis_block(tx).is_err());
}

#[test]
fn test_hashed_block_get_block() {
    let mut hashed_block = HashedBlock::new();
    hashed_block.mut_signed_block().mut_block()
        .set_previous(hash(b"test1").0.to_vec());
    assert!(hashed_block.get_signed_block().get_block() == hashed_block.get_block());
}

#[test]
fn test_hashed_block_hash_integrity() {
    let mut hashed_block = HashedBlock::new();
    hashed_block.mut_signed_block().mut_block()
        .set_previous(hash(b"test123").0.to_vec());
    assert!(hashed_block.verify_hash().is_err());
    hashed_block.compute_hash();
    assert!(hashed_block.verify_hash().is_ok());

    hashed_block.mut_signed_block().mut_block()
        .set_previous(hash(b"test123.").0.to_vec());
    assert!(hashed_block.verify_hash().is_err());
    hashed_block.compute_hash();
    assert!(hashed_block.verify_hash().is_ok());
}

// #[test]
// fn test_hashed_block_sign_integrity() {
//     let mut hashed_block = HashedBlock::new();
//     hashed_block.mut_signed_block().mut_mut_block().set_hash(hash("test1"));

//     assert!(hashed_block.verify_hash().is_err());
//     hashed_block.compute_hash();
//     assert!(hashed_block.verify_hash().is_ok());
//     println!("Block hash: {}",
//             &hashed_block.get_hash()[].to_base64(base64::STANDARD));
// }
