use bitcoin::Network;
use reqwest;
use serde::{Serialize,Deserialize};
use crate::taproot::convert_all_inputs_to_sighashs;

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
pub struct Utxo {
    pub txid: String,
    pub vout: u64,
    pub address: String,
    #[serde(rename = "scriptPubKey")]
    pub script_pub_key: String,
    pub amount: f64,
    pub confirmations: i64,
    pub spendable: bool,
    pub solvable: bool,
    pub desc: String,
    pub parent_descs: Vec<String>,
    pub safe: bool
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct RpcRequest<T: Serialize> {
    pub jsonrpc: String,
    pub method: String,
    pub id: u32,
    pub params: T,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct SpvRpcRequestResult<T> {
    pub result: T,
    pub error: Option<String>,
    pub id: Option<u32>,
}



#[test]
pub fn test_uxto(){
    let mut utxos = get_utxos(
        "bcrt1p5cj85luz7uhaugxpusgtk3xpyp5wmje6ks5fcl3njzegfdxexuws45r4l6",
        "http://127.0.0.1:8332","r1","r123").unwrap();
    utxos.retain(|utxo| {
        utxo.spendable && utxo.amount >= 0.00001000 && utxo.confirmations > 0
    });
}

#[test]
pub fn test_create_tx(){
    let mut utxos = get_utxos(
        "bcrt1p0gx7rktgnlq23z9lsfdpkew22znr9e4frrelkqdcq3dum9z8hnrsd93q6u",
        "http://127.0.0.1:8332","r1","r123").unwrap();
    utxos.retain(|utxo| {
        utxo.spendable && utxo.amount >= 0.00001000 && utxo.confirmations > 0
    });
    const private_key: [u8; 32] = [118, 152, 169, 184, 186, 69, 131, 143, 153, 108, 249, 217, 150, 161, 172, 79, 240, 71, 47, 7, 207, 82, 111, 64, 225, 103, 194, 124, 87, 97, 50, 225];

    convert_all_inputs_to_sighashs(135, utxos,
                                   "bcrt1p5cj85luz7uhaugxpusgtk3xpyp5wmje6ks5fcl3njzegfdxexuws45r4l6",
                                   &private_key,
                                   "378b4d0ff06bd8e58eee92b213b81bc3c8fd7af562d1695409a442e1506ee6d8",
                                   Network::Regtest);
    // deposit_address_with_timelock_leaf(string)
    // rpc_url(string) rpc_username(string) rpc_password(string)
    // private_key(vec<u8>) time(u64) receiver_address(string)
    // pubkey_of_comitee(string) network(u64)
}

pub fn get_utxos(address: &str, rpc_url: &str, username: &str, password: &str) -> Result<Vec<Utxo>,String>{
    let client = reqwest::blocking::Client::new();

    let request_utxo = RpcRequest::<(u32, u32, Vec<&str>)> {
        jsonrpc: "1.0".to_string(),
        method: "listunspent".to_string(),
        id: 0,
        params: (1, 9999999, vec![address.clone()]),
    };

    let resp = client
        .post(rpc_url)
        .json(&request_utxo)
        .basic_auth(username, Some(password))
        .send().unwrap()
        .json::<SpvRpcRequestResult<Vec<Utxo>>>().unwrap();

    if let Some(e) = resp.error {
        return Err("err".to_string());
    } else {
        let utxos = resp.result;
        if utxos.is_empty() {
            return Err("err".to_string());
        }
        return Ok(utxos);
    }
}