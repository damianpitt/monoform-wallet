"""Isolated developer/CI check: explicit verified Core bin directory, public fixture only."""
import os
import json
from contextlib import contextmanager
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time


@contextmanager
def isolated_node(bin_dir, network):
    with tempfile.TemporaryDirectory(prefix="monoform-core-" + network + "-") as temporary:
        datadir = str(Path(temporary).resolve())
        # Own the child PID: never send a stop request to a possibly unrelated node.
        node = subprocess.Popen([
            str(bin_dir / "bitcoind"), "-" + network, "-conf=/dev/null", f"-datadir={datadir}",
            "-server=1", "-disablewallet=1", "-blockfilterindex=1", "-networkactive=0", "-listen=0",
            "-dnsseed=0", "-discover=0", "-rpcbind=127.0.0.1", "-rpcallowip=127.0.0.1",
            "-rpcport=48332",
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            # Do not authenticate to another node if this child's port bind fails.
            cookie = Path(datadir) / network / ".cookie"
            deadline = time.monotonic() + 20
            while not cookie.exists():
                if node.poll() is not None or time.monotonic() >= deadline:
                    raise ValueError("isolated Core did not start")
                time.sleep(0.1)
            environment = dict(os.environ, MONOFORM_TEST_CORE_DIR=datadir,
                               MONOFORM_TEST_CORE_BIN=str(bin_dir),
                               PATH=str(bin_dir) + os.pathsep + os.environ.get("PATH", ""))
            environment.pop("MONOFORM_REGTEST_FIXTURE", None)
            cli = [str(bin_dir / "bitcoin-cli"), "-" + network, "-conf=/dev/null",
                   f"-datadir={datadir}", "-rpcconnect=127.0.0.1", "-rpcwait",
                   "-rpcport=48332", "-rpcwaittimeout=15", "-rpcclienttimeout=10"]
            def rpc(*params):
                result = subprocess.run(cli + list(params), check=True, capture_output=True, timeout=20)
                return json.loads(result.stdout)
            rpc("getblockchaininfo")
            if node.poll() is not None:
                raise ValueError("isolated Core exited")
            yield datadir, environment, rpc
        finally:
            node.terminate()
            try:
                node.wait(timeout=15)
            except subprocess.TimeoutExpired:
                node.kill()
                node.wait(timeout=5)


def run(bin_dir):
    if not bin_dir.is_absolute():
        raise ValueError("expected absolute verified Core bin directory")
    cargo = shutil.which("cargo")
    if cargo is None:
        raise ValueError("cargo unavailable")
    # Build first, so node lifetime stays short. Default fixture stays offline.
    subprocess.run([cargo, "build", "--locked", "--example", "bitcoin_core_fixture"], check=True, timeout=600)
    subprocess.run([cargo, "test", "--locked", "--features", "core-rpc",
                    "--test", "bitcoin_core_live", "--no-run"], check=True, timeout=600)
    subprocess.run([cargo, "test", "--locked", "--features", "core-rpc", "--lib",
                    "--no-run"], check=True, timeout=600)
    fixture = subprocess.run([cargo, "run", "--locked", "--quiet", "--example", "bitcoin_core_fixture"],
                             check=True, capture_output=True, timeout=60)
    def native_test(name, environment):
        subprocess.run([cargo, "test", "--locked", "--features", "core-rpc", "--lib", name,
                        "--", "--ignored", "--exact"], check=True, env=environment, timeout=90)
    utxo_test = "core_rpc::utxos::tests::core_utxo_scan_schemas_match_and_nonempty_amounts_are_exact"
    pending_test = "core_rpc::mempool::tests::core_pending_chains_replacement_and_confirmation_match"
    with isolated_node(bin_dir, "testnet4") as (datadir, environment, _):
        subprocess.run([sys.executable, "-B", "scripts/check_bitcoin_core.py", datadir],
                       input=fixture.stdout, check=True, env=environment, timeout=60)
        subprocess.run([cargo, "test", "--locked", "--features", "core-rpc", "--test",
                        "bitcoin_core_live", "--", "--ignored"], check=True, env=environment, timeout=90)
        native_test("core_rpc::discovery::tests::core_genesis_filter_scan_and_activity_schema_match", environment)
        native_test(utxo_test, environment)
        native_test(pending_test, environment)
    with isolated_node(bin_dir, "regtest") as (_, environment, rpc):
        # Coinbases only here; ignored mempool tests sign published-fixture keys on
        # this peer-disabled regtest node. Production exposes no signing/broadcast.
        receive, change = json.loads(fixture.stdout)["descriptors"]
        receive_zero = rpc("deriveaddresses", receive, "[0,0]")[0]
        change_zero = rpc("deriveaddresses", change, "[0,0]")[0]
        outside_scope = rpc("deriveaddresses", receive, "[99,99]")[0]
        rpc("generatetoaddress", "1", receive_zero)
        rpc("generatetoaddress", "100", outside_scope)
        rpc("generatetoaddress", "1", change_zero)
        native_test(utxo_test, dict(environment, MONOFORM_REGTEST_FIXTURE="1"))
        native_test(pending_test, dict(environment, MONOFORM_REGTEST_FIXTURE="1"))


if __name__ == "__main__":
    try:
        if len(sys.argv) != 2:
            raise ValueError("expected absolute verified Core bin directory")
        run(Path(sys.argv[1]))
    except (OSError, ValueError, subprocess.SubprocessError):
        sys.exit("Isolated Core compatibility test failed; no user wallet was modified.")
