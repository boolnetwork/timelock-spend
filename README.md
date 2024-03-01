it can spend utxo sent to this kind of tapleaf in taproot address:
```
OP_PUSHBYTES_2 
<expire time>
OP_CLTV 
OP_DROP 
OP_PUSHBYTES_32 
<internal key of the beneficiary> 
OP_CHECKSIG
```


example:
```
./forced-withdraw.exe --secret 7698a9b8ba45838f996cf9d996a1ac4ff0472f07cf526f40e167c27c576132e1 \
--commitee df96a7f0809b69deb50936b91626ffad8f07f79518bd3f37067eff2e04bb6ed1 --time 200 \
--receiver bcrt1p0gx7rktgnlq23z9lsfdpkew22znr9e4frrelkqdcq3dum9z8hnrsd93q6u --amount 100000000  \
--utxo 980430593fb868eb995d287c9ca5cc68dcf979d9c08ec9de9fd1f1e205b329d1 \
--index-utxo 1 -n 2
```

`--secret` is the hex format of The private key of the beneficiary of the time-lock script in Taproot.

`--commitee` is the internal key of the DHC, x only publickey

`--receiver` decide who receive the btc

`--time` expire time of the time-lock script in Taproot.

`--utxo` `--index-utxo` `--amount`  The three together constitute the core data of the UTXO that will be spent in this transaction.

`--fee-rate` is default set to 0.00001

```
-n 0    = Network::Bitcoin
-n 1    = Network::Testnet
-n 2    = Network::Regtest
```