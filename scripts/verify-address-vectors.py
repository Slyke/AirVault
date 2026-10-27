"""Public, reproducible address vectors. Verification dependency: bip_utils==2.12.2."""
import json
from pathlib import Path
from bip_utils import *
phrase="abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
seed=Bip39SeedGenerator(phrase).Generate()
vectors={}
for name,coin in [("ethereum",Bip44Coins.ETHEREUM),("litecoin",Bip44Coins.LITECOIN),("dogecoin",Bip44Coins.DOGECOIN),("bitcoin_cash",Bip44Coins.BITCOIN_CASH),("cosmos",Bip44Coins.COSMOS),("xrp",Bip44Coins.RIPPLE),("tron",Bip44Coins.TRON)]:
    vectors[name]=Bip44.FromSeed(seed,coin).Purpose().Coin().Account(0).Change(Bip44Changes.CHAIN_EXT).AddressIndex(0).PublicKey().ToAddress()
sol=Bip32Slip10Ed25519.FromSeed(seed).DerivePath("m/44'/501'/0'/0'")
vectors["solana"]=SolAddrEncoder.EncodeKey(sol.PublicKey().RawCompressed().ToBytes()[1:])
dot=Substrate.FromSeed(SubstrateBip39SeedGenerator(phrase).Generate(),SubstrateCoins.POLKADOT)
vectors["polkadot"]=dot.PublicKey().ToAddress()
vectors["polkadot//1"]=dot.DerivePath("//1").PublicKey().ToAddress()
for name,coin in [("cardano",Cip1852Coins.CARDANO_ICARUS),("cardano_preprod",Cip1852Coins.CARDANO_ICARUS_TESTNET)]:
    ada=Cip1852.FromSeed(CardanoIcarusSeedGenerator(phrase).Generate(),coin).Purpose().Coin().Account(0)
    vectors[name]=CardanoShelley.FromCip1852Object(ada).Change(Bip44Changes.CHAIN_EXT).AddressIndex(0).PublicKeys().ToAddress()
Path("crates/airvault-core/tests/fixtures/addresses.json").write_text(json.dumps(vectors,indent=2)+"\n")
print(json.dumps(vectors,indent=2))
