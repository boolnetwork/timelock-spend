use clap::{App, Arg, Parser};
use std::str::FromStr;
use bitcoin::secp256k1::XOnlyPublicKey;

use bitcoin::hashes::Hash;
use bitcoin::key::{Keypair, TapTweak, TweakedKeypair, UntweakedPublicKey};
use bitcoin::locktime::absolute;
use bitcoin::secp256k1::{rand, Message, Secp256k1, SecretKey, Signing, Verification};
use bitcoin::sighash::{Prevouts, SighashCache, TapSighashType};
use bitcoin::{transaction, Address, Amount, Network, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid, Witness, script, TapNodeHash};
use bitcoin::consensus::encode;
use bitcoin::opcodes::all::{OP_CHECKSIG, OP_CLTV, OP_DROP};
use bitcoin::taproot::{ControlBlock, LeafVersion, TaprootBuilder};
use bitcoin::TapLeafHash;

use hex;

const DUMMY_UTXO_AMOUNT: Amount = Amount::from_sat( 100_000_000 );
const SPEND_AMOUNT: Amount = Amount::from_sat( 50_000_000);
const CHANGE_AMOUNT: Amount = Amount::from_sat(100_000_000- 50_000_000 - 1_000); // 1000 sat fee.

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, value_parser)]
    secret: String,

    #[clap(short, long, value_parser)]
    commitee: String,

    #[clap(short, long, value_parser)]
    time: u64,

    #[clap(short, long, value_parser)]
    receiver: String,

    #[clap(long, value_parser)]
    amount: u64,

    #[clap(short, long, value_parser, default_value = "0.00001")]
    fee_rate: f64,

    #[clap(short, long, value_parser)]
    utxo: String,

    #[clap(short, long, value_parser)]
    index_utxo: u64,

    #[clap(short, long, value_parser)]
    network: u64,
}


fn main() {
    let args = Args::parse();
    println!("secret {}!", args.secret);
    let private_key_bytes = hex::decode(args.secret).unwrap();
    let private_key_u8: Vec<u8> = private_key_bytes.iter().map(|&x| x as u8).collect();
    println!("private_key_u8 {:?}!", private_key_u8);

    println!("commitee {}!", args.commitee);
    println!("unlock time {}!", args.time);
    println!("amount {}! fee_rate {}!", args.amount, args.fee_rate);

    let network = match  args.network {
        0 => Network::Bitcoin,
        1 => Network::Testnet,
        2 => Network::Regtest,
        _ => Network::Testnet
    };
    create_tx(&private_key_u8, args.time, args.commitee,
              args.amount, args.receiver, args.fee_rate, args.utxo, args.index_utxo, network);
}

fn create_tx(secret: &[u8], time: u64, commitee: String, amount: u64, receiver:String ,fee_rate: f64,
             utxo: String, index_utxo: u64, network: Network) {
    let secp = Secp256k1::new();

    let keypair = Keypair::from_seckey_slice(&secp, secret).unwrap();

    let (internal_key, _parity) = keypair.x_only_public_key();

    let reveal_script  = script::Builder::new()
        .push_int(time as i64)
        .push_opcode(OP_CLTV)
        .push_opcode(OP_DROP)
        .push_x_only_key(&internal_key)
        .push_opcode(OP_CHECKSIG)
        .into_script();
    println!("{:?}",reveal_script);

    let commitee_internal_key = XOnlyPublicKey::from_str(&commitee).unwrap();

    let taproot_spend_info = TaprootBuilder::new()
        .add_leaf(0, reveal_script.clone())
        .expect("adding leaf should work")
        .finalize(&secp, commitee_internal_key)
        .expect("finalizing taproot builder should work");

    let control_block = taproot_spend_info
        .control_block(&(reveal_script.clone(), LeafVersion::TapScript))
        .expect("should compute control block");

    let script_pubkey = ScriptBuf::new_p2tr(
        &secp,
        taproot_spend_info.internal_key(),
        taproot_spend_info.merkle_root(),
    );

    let merkle_root = taproot_spend_info.merkle_root();
    let address = Address::p2tr(&secp, commitee_internal_key, merkle_root, network);
    println!("taproot address {}",address);
    let out_point = OutPoint {
        txid: Txid::from_str(&utxo).unwrap(), // Obviously invalid.
        vout: index_utxo as u32,
    };
    let utxo = TxOut { value: Amount::from_sat( amount), script_pubkey };

    let address = receivers_address(&receiver, network);
    // The input for the transaction we are constructing.
    let input = TxIn {
        previous_output: out_point, // The dummy output we are spending.
        script_sig: ScriptBuf::default(), // For a p2tr script_sig is empty.
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
        witness: Witness::default(), // Filled in after signing.
    };

    let spend = TxOut { value: Amount::from_sat(amount), script_pubkey: address.script_pubkey() };

    // The transaction we want to sign and broadcast.
    let mut unsigned_tx = Transaction {
        version: transaction::Version::TWO,  // Post BIP-68.
        lock_time: absolute::LockTime::from_time((time + 1) as u32).unwrap(), // Ignore the locktime.
        input: vec![input],                  // Input goes into index 0.
        output: vec![spend],         // Outputs, order does not matter.
    };

    let mut unsigned_tx = fee_tx(unsigned_tx.clone(), fee_rate);

    let mut sighasher = SighashCache::new(&mut unsigned_tx);
    let sighash = sighasher
        .taproot_script_spend_signature_hash(
            0,
            &Prevouts::All(&vec![utxo]),
            TapLeafHash::from_script(&reveal_script, LeafVersion::TapScript),
            TapSighashType::Default,
        )
        .expect("failed to construct sighash");

    let signature = secp.sign_schnorr(&Message::from(sighash), &keypair);
    println!("signateure {}",signature);
    //1a069fec473e1c251498b981eb6f7e2746996ed6ce76c25479c20b61aee5ba4f7068ae59ad1db775541dc03f31b5e042d2307041a743e5f20e51033db122dc0c
    let witness = sighasher
        .witness_mut(0)
        .expect("getting mutable witness reference should work");

    witness.push(
        bitcoin::taproot::Signature { sig: signature, hash_ty: TapSighashType::Default, }.to_vec(),
    );
    witness.push(reveal_script);
    witness.push(&control_block.serialize());
    let tx = sighasher.into_transaction();
    let tx_hex = encode::serialize_hex(&tx);

    println!("{}", tx_hex);
}

fn receivers_address(receiver: &str, network: Network) -> Address {
    Address::from_str(receiver)
        .expect("a valid address")
        .require_network(network)
        .expect("valid address for mainnet")
}

pub fn fee_tx(tx: Transaction, fee_rate: f64) -> Transaction{
    let mut tx_fee = tx.clone();
    tx_fee.input[0].witness = Witness::from(vec![vec![88u8;14];15]);
    let fee = calculate_fee(tx_fee.vsize(), fee_rate, 1.0);
    println!("actual fee {:?}",fee);
    tx_fee.input[0].witness.clear();
    tx_fee.output[0].value = tx_fee.output[0].value - Amount::from_sat(fee);
    println!("{:?}", tx_fee);
    return tx_fee;
}

pub fn calculate_fee(virtual_size: usize, rate: f64, multiplier: f64) -> u64 {
    let kilo_bytes = virtual_size as f64 / 1000_f64;
    let rate = bitcoin::Amount::from_btc(rate).unwrap().to_sat() as f64;
    ((kilo_bytes * rate) * multiplier).round() as u64
}