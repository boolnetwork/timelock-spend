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
    beneficiary: String,

    #[clap(short, long, value_parser)]
    time: u64,

    #[clap(short, long, value_parser)]
    receiver: String,

    #[clap(long, value_parser)]
    amount: u64,

    #[clap(short, long, value_parser)]
    fee: u64,

    #[clap(short, long, value_parser)]
    utxo: String,

    #[clap(short, long, value_parser)]
    index_utxo: u64,
}


fn main() {
    //let sec = "e8b6b07943b6d7b966a05d10b8a80aabf784f0947993b3b38622dec72e279558";

    let args = Args::parse();

    println!("secret {}!", args.secret);
    let private_key_bytes = hex::decode(args.secret).unwrap();
    let private_key_u8: Vec<u8> = private_key_bytes.iter().map(|&x| x as u8).collect();
    println!("private_key_u8 {:?}!", private_key_u8);

    println!("secret {}!", args.beneficiary);
    let beneficiary_bytes = hex::decode(args.beneficiary).unwrap();
    let beneficiary_u8: Vec<u8> = beneficiary_bytes.iter().map(|&x| x as u8).collect();
    println!("beneficiary_u8 {:?}!", beneficiary_u8);
    let mut beneficiary_u8_32 = [0u8;32];
    beneficiary_u8_32.copy_from_slice(&beneficiary_u8);



    println!("unlock time {}!", args.time);
    println!("amount fee {}! {}!", args.amount, args.fee);

    create_tx(&private_key_u8, args.time, beneficiary_u8_32,
              args.amount, args.receiver, args.fee, args.utxo, args.index_utxo );
}

fn create_tx(secret: &[u8], time: u64, beneficiary: [u8;32], amount: u64, receiver:String ,fee: u64,
             utxo: String, index_utxo: u64) {
    let secp = Secp256k1::new();

    //let keypair = Keypair::from_seckey_slice(&secp, &private_key).unwrap();
    let keypair = Keypair::from_seckey_slice(&secp, secret).unwrap();

    let (internal_key, _parity) = keypair.x_only_public_key();


    // 这里build叶子的script
    let reveal_script  = script::Builder::new()
        //.push_int(135u32 as i64)
        .push_int(time as i64)
        .push_opcode(OP_CLTV)
        .push_opcode(OP_DROP)
        //.push_x_only_key(&internal_key)
        //.push_slice(internal_key.serialize())
        .push_slice(beneficiary)
        .push_opcode(OP_CHECKSIG)
        .into_script();
    println!("{:?}",reveal_script);

    let taproot_spend_info = TaprootBuilder::new()
        .add_leaf(0, reveal_script.clone())
        .expect("adding leaf should work")
        .finalize(&secp, internal_key)
        .expect("finalizing taproot builder should work");

    let control_block = taproot_spend_info
        .control_block(&(reveal_script.clone(), LeafVersion::TapScript))
        .expect("should compute control block");

    let script_pubkey = ScriptBuf::new_p2tr(
        &secp,
        taproot_spend_info.internal_key(), // 这个和上面的internal_key是一样的
        taproot_spend_info.merkle_root(),
    );

    let merkle_root = taproot_spend_info.merkle_root();
    let address = Address::p2tr(&secp, internal_key, merkle_root,Network::Regtest);
    // 构建完毕后 往这个taproot address 转账，接着后面生成的代码可以花费这个taproot的叶子
    println!("taproot address {}",address);
    // 填写这个转账的信息
    let out_point = OutPoint {
        txid: Txid::from_str(&utxo).unwrap(), // Obviously invalid.
        vout: index_utxo as u32,
    };
    let utxo = TxOut { value: Amount::from_sat( amount), script_pubkey };

    let address = receivers_address(&receiver);
    // The input for the transaction we are constructing.
    let input = TxIn {
        previous_output: out_point, // The dummy output we are spending.
        script_sig: ScriptBuf::default(), // For a p2tr script_sig is empty.
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
        witness: Witness::default(), // Filled in after signing.
    };

    let spend = TxOut { value: SPEND_AMOUNT, script_pubkey: address.script_pubkey() };
    let change = TxOut {
        value: Amount::from_sat(amount - fee),
        script_pubkey: ScriptBuf::new_p2tr(&secp, internal_key, None)
    };

    // The transaction we want to sign and broadcast.
    let mut unsigned_tx = Transaction {
        version: transaction::Version::TWO,  // Post BIP-68.
        lock_time: absolute::LockTime::from_height(time as u32).unwrap(), // Ignore the locktime.
        input: vec![input],                  // Input goes into index 0.
        output: vec![spend, change],         // Outputs, order does not matter.
    };

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
    //分开签名

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

fn receivers_address(receiver: &str) -> Address {
    Address::from_str(receiver)
        .expect("a valid address")
        .require_network(Network::Regtest)
        .expect("valid address for mainnet")
}