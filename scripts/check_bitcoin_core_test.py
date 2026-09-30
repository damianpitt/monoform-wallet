"""No running node required: prove the optional checker fails closed."""
import copy
import unittest
from unittest.mock import patch

import check_bitcoin_core as checker


class CoreCheckerTests(unittest.TestCase):
    # Synthetic public strings exercise orchestration, not descriptor cryptography.
    fixture = {"descriptors": ["receive#one", "change#two"],
               "addresses": [["r0", "r1"], ["c0", "c1"]]}
    info = {"hasprivatekeys": False, "isrange": True, "issolvable": True, "checksum": "one"}

    def responses(self):
        return [{"chain": "testnet4"}, self.info, ["r0", "r1"],
                dict(self.info, checksum="two"), ["c0", "c1"]]

    def test_only_read_only_methods_are_called(self):
        with patch.object(checker, "rpc", side_effect=self.responses()) as rpc:
            checker.check("/public-test-data", self.fixture)
        self.assertEqual([call.args[1] for call in rpc.call_args_list],
                         ["getblockchaininfo", "getdescriptorinfo", "deriveaddresses",
                          "getdescriptorinfo", "deriveaddresses"])

    def test_wrong_chain_and_incompatible_responses_fail(self):
        cases = [[{"chain": "main"}]]
        for field, value in [("hasprivatekeys", True), ("isrange", False),
                             ("issolvable", False), ("checksum", "wrong")]:
            cases.append([{"chain": "testnet4"}, dict(self.info, **{field: value})])
        cases.append([{"chain": "testnet4"}, self.info, ["wrong", "r1"]])
        for responses in cases:
            with self.subTest(responses=responses), patch.object(checker, "rpc", side_effect=responses):
                with self.assertRaises(ValueError):
                    checker.check("/public-test-data", self.fixture)

    def test_empty_or_incomplete_fixture_cannot_pass(self):
        for key in self.fixture:
            fixture = copy.deepcopy(self.fixture)
            fixture[key] = []
            with patch.object(checker, "rpc") as rpc, self.assertRaises(ValueError):
                checker.check("/public-test-data", fixture)
            rpc.assert_not_called()

    def test_transport_stays_on_loopback_testnet4_with_timeout(self):
        with patch.object(checker.subprocess, "run") as run:
            run.return_value.stdout = '{"chain":"testnet4"}'
            checker.rpc("/public-test-data", "getblockchaininfo")
        args, kwargs = run.call_args
        self.assertIn("-testnet4", args[0])
        self.assertIn("-rpcconnect=127.0.0.1", args[0])
        self.assertIn("-conf=/dev/null", args[0])
        self.assertEqual(kwargs["timeout"], 15)
        self.assertTrue(kwargs["check"])
