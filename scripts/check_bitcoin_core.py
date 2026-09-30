"""Opt-in, read-only Core check using only the published fixture emitted by Rust."""
import json
from pathlib import Path
import subprocess
import sys


def rpc(datadir, method, *params):
    # Ignore config: fixed loopback/testnet4 defaults, local cookie, no remote endpoint.
    result = subprocess.run(
        ["bitcoin-cli", "-testnet4", "-conf=/dev/null", f"-datadir={datadir}",
         "-rpcconnect=127.0.0.1", "-rpcclienttimeout=10", method, *params],
        capture_output=True, text=True, check=True, timeout=15,
    )
    return json.loads(result.stdout)


def check(datadir, fixture):
    if (len(fixture["descriptors"]) != 2 or len(fixture["addresses"]) != 2
            or any(len(addresses) != 2 for addresses in fixture["addresses"])):
        raise ValueError("expected two branches with two addresses each")
    if rpc(datadir, "getblockchaininfo")["chain"] != "testnet4":
        raise ValueError("wrong chain")
    # Counts were checked above; plain zip also supports macOS's bundled Python 3.9.
    for descriptor, expected in zip(fixture["descriptors"], fixture["addresses"]):
        info = rpc(datadir, "getdescriptorinfo", descriptor)
        if (info["hasprivatekeys"] is not False or info["isrange"] is not True
                or info["issolvable"] is not True
                or info["checksum"] != descriptor.rsplit("#", 1)[1]):
            raise ValueError("descriptor mismatch")
        if rpc(datadir, "deriveaddresses", descriptor, "[0,1]") != expected:
            raise ValueError("address mismatch")


if __name__ == "__main__":
    try:
        if len(sys.argv) != 2 or not Path(sys.argv[1]).is_absolute():
            raise ValueError("usage: check_bitcoin_core.py /absolute/bitcoin-data")
        check(sys.argv[1], json.load(sys.stdin))
    except (OSError, subprocess.SubprocessError, ValueError, KeyError, IndexError, TypeError):
        # Do not echo RPC errors, config, credentials, or supplied wallet material.
        sys.exit("Core compatibility check failed; verify fixture and local testnet4 node.")
    print("Core testnet4: both descriptors and address ranges match the public fixture.")
