use super::{Decode, Document};
use sha3::{Digest, Keccak256};

struct Item {
    offset: usize,
    prefix: usize,
    length: usize,
    list: bool,
}
fn item(bytes: &[u8], offset: usize, end: usize) -> Decode<Item> {
    let tag = *bytes
        .get(offset)
        .filter(|_| offset < end)
        .ok_or("Truncated RLP item.")?;
    let (prefix, length, list) = match tag {
        0..=0x7f => (0, 1, false),
        0x80..=0xb7 => (1, usize::from(tag - 0x80), false),
        0xc0..=0xf7 => (1, usize::from(tag - 0xc0), true),
        _ => {
            let n = if tag < 0xc0 {
                usize::from(tag - 0xb7)
            } else {
                usize::from(tag - 0xf7)
            };
            let size = bytes
                .get(offset + 1..offset + 1 + n)
                .filter(|_| offset + 1 + n <= end)
                .ok_or("Truncated RLP length.")?;
            if size[0] == 0 || n > std::mem::size_of::<usize>() {
                return Err("Non-canonical or excessive RLP length.");
            }
            let mut length = 0usize;
            for b in size {
                length = length
                    .checked_mul(256)
                    .and_then(|v| v.checked_add(*b as usize))
                    .ok_or("RLP length overflow.")?;
            }
            if length < 56 {
                return Err("RLP long form used for a short payload.");
            }
            (1 + n, length, tag >= 0xf8)
        }
    };
    let stop = offset
        .checked_add(prefix)
        .and_then(|v| v.checked_add(length))
        .filter(|v| *v <= end)
        .ok_or("RLP length extends beyond its container.")?;
    if prefix == 1 && !list && length == 1 && bytes[stop - 1] < 0x80 {
        return Err("Non-canonical RLP single-byte string.");
    }
    Ok(Item {
        offset,
        prefix,
        length,
        list,
    })
}
fn children(bytes: &[u8], node: &Item) -> Decode<Vec<Item>> {
    if !node.list {
        return Err("Expected an RLP list.");
    }
    let mut pos = node.offset + node.prefix;
    let end = pos + node.length;
    let mut out = vec![];
    while pos < end {
        if out.len() >= 10_000 {
            return Err("RLP list exceeds the inspector item limit.");
        }
        let n = item(bytes, pos, end)?;
        pos += n.prefix + n.length;
        out.push(n);
    }
    Ok(out)
}
pub fn looks_like(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let offset = if matches!(bytes[0], 1 | 2) { 1 } else { 0 };
    let Ok(top) = item(bytes, offset, bytes.len()) else {
        return false;
    };
    if !top.list || top.offset + top.prefix + top.length != bytes.len() {
        return false;
    }
    let Ok(items) = children(bytes, &top) else {
        return false;
    };
    match offset {
        0 => matches!(items.len(), 6 | 9),
        _ => {
            if bytes[0] == 1 {
                matches!(items.len(), 8 | 11)
            } else {
                matches!(items.len(), 9 | 12)
            }
        }
    }
}
fn walk(
    d: &mut Document,
    bytes: &[u8],
    node: &Item,
    name: &str,
    group: &str,
    help: &str,
    expected: &str,
    depth: usize,
) -> Decode<()> {
    if depth > 12 {
        return Err("RLP nesting exceeds the inspector limit.");
    }
    if node.prefix > 0 {
        d.field(
            node.offset,
            node.prefix,
            format!("{name} · RLP prefix"),
            "metadata",
            hex::encode(&bytes[node.offset..node.offset + node.prefix]),
            "RLP type and length prefix.",
            "Must use the shortest RLP encoding and fit inside its parent.",
        )?;
    }
    if node.list {
        for (i, child) in children(bytes, node)?.iter().enumerate() {
            walk(
                d,
                bytes,
                child,
                &format!("{name} · {}", i + 1),
                group,
                help,
                expected,
                depth + 1,
            )?;
        }
    } else {
        let offset = node.offset + node.prefix;
        let b = &bytes[offset..offset + node.length];
        if name == "Call data" && b.len() >= 4 {
            d.field(offset,4,"Function selector","payload",hex::encode(&b[..4]),"First four bytes of the Keccak-256 function signature; selectors can collide.","a9059cbb commonly denotes transfer(address,uint256). A selector alone does not establish contract behavior.")?;
            for (i, word) in b[4..].chunks(32).enumerate() {
                d.field(offset+4+i*32,word.len(),format!("Call data · ABI word {}",i+1),"payload",hex::encode(word),"A 32-byte ABI argument word or fragment. Dynamic arguments use offsets relative to the argument area.","Verify recipient, amount and offsets against the trusted contract ABI. This inspector does not execute or fully decompile contracts.")?;
            }
        } else {
            let value = if b.len() <= 16 && group != "payload" {
                let mut n = [0u8; 16];
                n[16 - b.len()..].copy_from_slice(b);
                u128::from_be_bytes(n).to_string()
            } else if b.len() <= 128 {
                format!("0x{}", hex::encode(b))
            } else {
                format!("{} bytes", b.len())
            };
            d.field(offset, b.len(), name, group, value, help, expected)?;
        }
    }
    Ok(())
}
pub fn transaction(d: &mut Document, bytes: &[u8]) -> Decode<()> {
    let first = *bytes.first().ok_or("Empty transaction.")?;
    let typed = matches!(first, 1 | 2);
    let offset = if typed { 1 } else { 0 };
    if typed {
        d.field(
            0,
            1,
            "Transaction type",
            "header",
            first.to_string(),
            "EIP-2718 transaction envelope type.",
            "1 is an access-list transaction; 2 is an EIP-1559 fee-market transaction.",
        )?;
    }
    let root = item(bytes, offset, bytes.len())?;
    if root.offset + root.prefix + root.length != bytes.len() {
        return Err("Trailing bytes after EVM transaction.");
    }
    let entries = children(bytes, &root)?;
    let names: &[&str] = if !typed {
        &[
            "Nonce",
            "Gas price",
            "Gas limit",
            "Destination",
            "Value (wei)",
            "Call data",
            "v / chain ID",
            "r",
            "s",
        ]
    } else if first == 1 {
        &[
            "Chain ID",
            "Nonce",
            "Gas price",
            "Gas limit",
            "Destination",
            "Value (wei)",
            "Call data",
            "Access list",
            "y parity",
            "r",
            "s",
        ]
    } else {
        &[
            "Chain ID",
            "Nonce",
            "Priority fee per gas",
            "Maximum fee per gas",
            "Gas limit",
            "Destination",
            "Value (wei)",
            "Call data",
            "Access list",
            "y parity",
            "r",
            "s",
        ]
    };
    let valid = if !typed {
        matches!(entries.len(), 6 | 9)
    } else if first == 1 {
        matches!(entries.len(), 8 | 11)
    } else {
        matches!(entries.len(), 9 | 12)
    };
    if !valid {
        return Err("Unexpected EVM transaction field count or unsupported transaction type.");
    }
    d.field(
        root.offset,
        root.prefix,
        "Transaction RLP list",
        "header",
        entries.len().to_string(),
        "RLP list containing the ordered transaction fields.",
        "Every child must fit within this list and use canonical RLP encoding.",
    )?;
    for (i, node) in entries.iter().enumerate() {
        let name = names[i];
        let data = &bytes[node.offset + node.prefix..node.offset + node.prefix + node.length];
        let (group,help,expected)=match name {
            "Destination"=>("payload","20-byte recipient or contract address; empty means contract creation.","Verify the address on the intended chain. An identical address can exist on multiple chains."),
            "Call data"=>("payload","Contract call arguments or contract creation bytecode.","Empty for a simple native-coin transfer; verify arbitrary calls with a trusted ABI."),
            "Access list"=>("metadata","Addresses and storage keys listed for access by this transaction.","Each entry contains a 20-byte address and a list of 32-byte storage keys."),
            "r"|"s"|"y parity"=>("witness","A component of the transaction's recoverable ECDSA signature.","Canonical secp256k1 signature values; signature recovery is not performed here."),
            _=>("metadata","Unsigned big-endian transaction quantity. Monetary values are in wei; gas fields are in gas or wei per gas.","Minimal integer encoding with no leading zero bytes. Check the intended chain, nonce and fee.")
        };
        if name == "Access list" {
            for entry in children(bytes, node)? {
                let parts = children(bytes, &entry)?;
                if parts.len() != 2 || parts[0].list || parts[0].length != 20 {
                    return Err("Invalid access-list address entry.");
                }
                for key in children(bytes, &parts[1])? {
                    if key.list || key.length != 32 {
                        return Err("Invalid access-list storage key.");
                    }
                }
            }
        } else {
            if node.list {
                return Err("EVM scalar field cannot be an RLP list.");
            }
            if name == "Destination" {
                if !matches!(data.len(), 0 | 20) {
                    return Err("EVM destination must be empty or 20 bytes.");
                }
            } else if name != "Call data" && (data.len() > 32 || data.first() == Some(&0)) {
                return Err("Non-canonical EVM integer or integer exceeds 256 bits.");
            }
            if name == "y parity" && !(data.is_empty() || data == [1]) {
                return Err("EVM y parity must be zero or one.");
            }
        }
        walk(d, bytes, node, name, group, help, expected, 0)?;
    }
    d.check("pass","RLP transaction encoding","Checked canonical RLP, field count, scalar lengths, integer encoding, recipient size and access-list layout.");
    d.check("info","Checksum","EVM transactions have no embedded file checksum. Keccak-256 identifies these encoded bytes; it does not authenticate an intended recipient.");
    d.check("info","Execution and signatures","Signature recovery, contract execution, account nonce, balances and chain acceptance are not checked by this decoder.");
    d.properties
        .push(("Keccak-256".into(), hex::encode(Keccak256::digest(bytes))));
    Ok(())
}
