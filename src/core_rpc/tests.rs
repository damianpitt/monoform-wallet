//! Only synthetic cookies and published fixture keys; never contact a real wallet.
use super::*;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::{PermissionsExt, symlink},
    thread::{self, JoinHandle},
};

const COOKIE: &str = "__cookie__:0000000000000000000000000000000000000000000000000000000000000000";

fn cookie_dir() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap_or_else(|_| panic!("test directory"));
    fs::create_dir(directory.path().join("testnet4"))
        .unwrap_or_else(|_| panic!("testnet directory"));
    let path = directory.path().join("testnet4/.cookie");
    fs::write(&path, COOKIE).unwrap_or_else(|_| panic!("synthetic cookie"));
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .unwrap_or_else(|_| panic!("private cookie"));
    directory
}

pub(super) fn envelope(result: serde_json::Value) -> String {
    json!({"jsonrpc":"2.0", "id":"monoform", "result":result}).to_string()
}

pub(super) fn chain() -> serde_json::Value {
    json!({"chain":"testnet4", "blocks":0, "headers":0, "bestblockhash":GENESIS,
           "time":1714777860_u64, "initialblockdownload":true, "pruned":false})
}

fn request(stream: &mut TcpStream) -> serde_json::Value {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap_or_else(|_| panic!("test read timeout"));
    let mut reader = BufReader::new(stream);
    let mut length = 0;
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .unwrap_or_else(|_| panic!("request line"));
    assert_eq!(line, "POST / HTTP/1.1\r\n");
    let mut authorized = false;
    loop {
        line.clear();
        reader
            .read_line(&mut line)
            .unwrap_or_else(|_| panic!("request header"));
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value
                    .trim()
                    .parse::<usize>()
                    .unwrap_or_else(|_| panic!("request length"));
            }
            if name.eq_ignore_ascii_case("authorization") {
                assert_eq!(value.trim(), format!("Basic {}", STANDARD.encode(COOKIE)));
                authorized = true;
            }
        }
    }
    assert!(authorized);
    assert!(length < 4096);
    let mut bytes = vec![0; length];
    reader
        .read_exact(&mut bytes)
        .unwrap_or_else(|_| panic!("request body"));
    serde_json::from_slice(&bytes).unwrap_or_else(|_| panic!("request JSON"))
}

pub(super) fn mock(
    replies: Vec<(u16, String)>,
) -> (
    CoreTestnet4,
    tempfile::TempDir,
    JoinHandle<Vec<serde_json::Value>>,
) {
    let directory = cookie_dir();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap_or_else(|_| panic!("test listener"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|_| panic!("test address"));
    let mut client = CoreTestnet4::new(directory.path()).unwrap_or_else(|_| panic!("test client"));
    // Only tests can replace the private, fixed production endpoint.
    client.endpoint = format!("http://{address}/");
    let handle = thread::spawn(move || {
        replies.into_iter().map(|(code, body)| {
            // A failed assertion must not leave a mock waiting forever for another RPC.
            listener.set_nonblocking(true).unwrap_or_else(|_|panic!("nonblocking mock"));
            let deadline=std::time::Instant::now()+Duration::from_secs(3);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(stream)=>break stream,
                    Err(error) if error.kind()==std::io::ErrorKind::WouldBlock && std::time::Instant::now()<deadline=>thread::sleep(Duration::from_millis(5)),
                    Err(_)=>panic!("test accept deadline"),
                }
            };
            // macOS can inherit O_NONBLOCK on accepted sockets; timed reads must block.
            stream.set_nonblocking(false).unwrap_or_else(|_|panic!("blocking mock stream"));
            let call = request(&mut stream);
            let response = format!("HTTP/1.1 {code} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}", body.len());
            // Oversized-response tests may close the connection before the write finishes.
            let _ = stream.write_all(response.as_bytes());
            call
        }).collect()
    });
    (client, directory, handle)
}

pub(super) fn calls(handle: JoinHandle<Vec<serde_json::Value>>) -> Vec<serde_json::Value> {
    handle
        .join()
        .unwrap_or_else(|_| panic!("mock server failed"))
}

pub(super) fn fixture() -> BitcoinTestnetAccount {
    // Published Trezor test vector; never use these words for funds.
    let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
    crate::OfflineSeed::from_phrase(phrase.to_owned())
        .unwrap_or_else(|_| panic!("public seed"))
        .bitcoin_testnet_account()
        .unwrap_or_else(|_| panic!("public account"))
}

#[test]
fn fixed_endpoint_no_proxy_no_redirects_and_no_library_logging() {
    let directory = cookie_dir();
    let client = CoreTestnet4::new(directory.path()).unwrap_or_else(|_| panic!("client"));
    assert_eq!(client.endpoint, ENDPOINT);
    assert!(client.agent.config().proxy().is_none());
    assert_eq!(client.agent.config().max_redirects(), 0);
    assert_eq!(client.agent.config().timeouts().global, Some(TIMEOUT));
    assert_eq!(log::STATIC_MAX_LEVEL, log::LevelFilter::Off);
    assert!(matches!(
        CoreTestnet4::new(Path::new("relative")),
        Err(CoreError::UnsafeCookie)
    ));
}

#[test]
fn status_classifies_syncing_stale_and_recent_without_balance_claims() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| panic!("test clock"))
        .as_secs();
    for (ibd, headers, time, expected) in [
        (true, 1, now, NodeState::Syncing),
        (false, 2, now, NodeState::Syncing),
        (false, 1, now - TIP_AGE - 60, NodeState::Stale),
        (false, 1, now, NodeState::Recent),
    ] {
        let mut info = chain();
        info["blocks"] = json!(1);
        info["bestblockhash"] = json!("1".repeat(64));
        info["headers"] = json!(headers);
        info["time"] = json!(time);
        info["initialblockdownload"] = json!(ibd);
        let (client, _directory, server) =
            mock(vec![(200, envelope(info)), (200, envelope(json!(GENESIS)))]);
        let status = client.status().unwrap_or_else(|_| panic!("typed status"));
        assert_eq!(status.state, expected);
        assert_eq!(status.blocks, 1);
        assert!(!status.pruned);
        let requests = calls(server);
        assert_eq!(requests[0]["method"], "getblockchaininfo");
        assert_eq!(requests[1]["method"], "getblockhash");
        assert_eq!(requests[1]["params"], json!([0]));
    }
}

#[test]
fn wrong_chain_stops_before_genesis_or_descriptor_disclosure() {
    for name in ["main", "test", "signet", "regtest"] {
        let mut info = chain();
        info["chain"] = json!(name);
        let (client, _directory, server) = mock(vec![(200, envelope(info))]);
        assert_eq!(
            client.verify_account(&fixture()),
            Err(CoreError::WrongChain)
        );
        assert_eq!(calls(server).len(), 1);
    }
    let (client, _directory, server) = mock(vec![
        (200, envelope(chain())),
        (200, envelope(json!("wrong genesis"))),
    ]);
    assert!(matches!(client.status(), Err(CoreError::WrongChain)));
    assert_eq!(calls(server).len(), 2);
}

#[test]
fn rejects_malformed_schema_envelopes_and_oversized_responses() {
    let mut malformed = vec![
        "not JSON".to_owned(),
        "[{}]".to_owned(),
        json!({"jsonrpc":"2.0","id":"wrong","result":chain()}).to_string(),
        json!({"jsonrpc":"1.0","id":"monoform","result":chain()}).to_string(),
        json!({"jsonrpc":"2.0","id":"monoform","result":null}).to_string(),
        " ".repeat(MAX_BODY as usize + 1),
        format!(
            r#"{{"jsonrpc":"2.0","id":"monoform","id":"monoform","result":{}}}"#,
            chain()
        ),
        format!("{} trailing", envelope(chain())),
    ];
    for (field, value) in [
        ("blocks", json!(-1)),
        ("blocks", json!(1.5)),
        ("blocks", json!(u64::MAX)),
        ("headers", json!("0")),
        ("pruned", json!(0)),
        ("initialblockdownload", json!(null)),
    ] {
        let mut info = chain();
        info[field] = value;
        malformed.push(envelope(info));
    }
    let mut missing = chain();
    missing
        .as_object_mut()
        .unwrap_or_else(|| panic!("object"))
        .remove("time");
    malformed.push(envelope(missing));
    for body in malformed {
        let (client, _directory, server) = mock(vec![(200, body)]);
        assert!(matches!(client.status(), Err(CoreError::InvalidResponse)));
        assert_eq!(calls(server).len(), 1);
    }
}

#[test]
fn rejects_inconsistent_tip_fields_after_identity_check() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| panic!("clock"))
        .as_secs();
    for changes in [
        json!({"blocks":2,"headers":1}),
        json!({"bestblockhash":"bad"}),
        json!({"bestblockhash":"A".repeat(64)}),
        json!({"time":now + TIP_AGE + 60}),
        json!({"bestblockhash":"0".repeat(64)}),
    ] {
        let mut info = chain();
        for (key, value) in changes.as_object().unwrap_or_else(|| panic!("object")) {
            info[key] = value.clone();
        }
        let (client, _directory, server) =
            mock(vec![(200, envelope(info)), (200, envelope(json!(GENESIS)))]);
        assert!(matches!(client.status(), Err(CoreError::InvalidResponse)));
        assert_eq!(calls(server).len(), 2);
    }
}

#[test]
fn authentication_rpc_failures_and_redirects_are_not_success() {
    for (code, body, expected) in [
        (
            401,
            "secret-error-must-not-escape".to_owned(),
            CoreError::Authentication,
        ),
        (403, String::new(), CoreError::Authentication),
        (503, String::new(), CoreError::Unavailable),
        (302, String::new(), CoreError::Unavailable),
        (
            200,
            json!({"jsonrpc":"2.0","id":"monoform","error":{"code":-28,"message":"private"}})
                .to_string(),
            CoreError::Unavailable,
        ),
        (
            200,
            json!({"jsonrpc":"2.0","id":"monoform","error":{"code":-1,"message":"private"}})
                .to_string(),
            CoreError::Rpc,
        ),
        (
            200,
            json!({"jsonrpc":"2.0","id":"monoform","result":chain(),"error":{"code":-1}})
                .to_string(),
            CoreError::InvalidResponse,
        ),
    ] {
        let (client, _directory, server) = mock(vec![(code, body)]);
        assert!(matches!(client.status(), Err(error) if error == expected));
        assert_eq!(calls(server).len(), 1);
    }
    assert_eq!(format!("{:?}", CoreError::Authentication), "Authentication");
}

#[test]
fn cookies_are_bounded_private_regular_local_files_and_reload_on_rotation() {
    let directory = cookie_dir();
    let path = directory.path().join("testnet4/.cookie");
    assert!(read_cookie(&path).is_ok());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))
        .unwrap_or_else(|_| panic!("mode"));
    assert!(matches!(read_cookie(&path), Err(CoreError::UnsafeCookie)));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .unwrap_or_else(|_| panic!("mode"));
    for invalid in [
        String::new(),
        format!("{COOKIE}\n"),
        COOKIE.replace("__cookie__", "user"),
        COOKIE.replace('0', "g"),
        "0".repeat(257),
    ] {
        fs::write(&path, invalid).unwrap_or_else(|_| panic!("cookie"));
        assert!(read_cookie(&path).is_err());
    }
    let other = directory.path().join("other");
    fs::rename(&path, &other).unwrap_or_else(|_| panic!("rename"));
    symlink(&other, &path).unwrap_or_else(|_| panic!("symlink"));
    assert!(read_cookie(&path).is_err());
    fs::remove_file(&path).unwrap_or_else(|_| panic!("unlink synthetic link"));
    fs::hard_link(&other, &path).unwrap_or_else(|_| panic!("hardlink"));
    assert!(matches!(read_cookie(&path), Err(CoreError::UnsafeCookie)));
    fs::remove_file(&path).unwrap_or_else(|_| panic!("unlink test link"));
    assert!(read_cookie(&path).is_err());
    let client = CoreTestnet4::new(directory.path()).unwrap_or_else(|_| panic!("client"));
    // A restart's replacement is observed without reconstructing the backend.
    fs::write(&path, COOKIE.replace('0', "a")).unwrap_or_else(|_| panic!("rotated cookie"));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .unwrap_or_else(|_| panic!("mode"));
    assert_eq!(
        &*read_cookie(&client.cookie).unwrap_or_else(|_| panic!("rotation")),
        COOKIE.replace('0', "a").as_bytes()
    );
}

#[test]
fn descriptor_verification_is_read_only_and_does_not_advance_counters() {
    let mut account = fixture();
    let descriptors = account
        .descriptors()
        .unwrap_or_else(|_| panic!("descriptors"));
    let state_before = account.encode_public_state();
    let receive = [
        account.next_receive_address(),
        account.next_receive_address(),
    ]
    .map(|address| address.unwrap_or_else(|_| panic!("public address")));
    let change = [account.next_change_address(), account.next_change_address()]
        .map(|address| address.unwrap_or_else(|_| panic!("public address")));
    let account = BitcoinTestnetAccount::from_public_state(&state_before)
        .unwrap_or_else(|_| panic!("restore"));
    let mut replies = vec![(200, envelope(chain())), (200, envelope(json!(GENESIS)))];
    for (descriptor, addresses) in [(descriptors.receive, receive), (descriptors.change, change)] {
        let checksum = descriptor
            .rsplit_once('#')
            .unwrap_or_else(|| panic!("checksum"))
            .1;
        replies.push((200, envelope(json!({"checksum":checksum,"isrange":true,"issolvable":true,"hasprivatekeys":false}))));
        replies.push((200, envelope(json!(addresses))));
    }
    let (client, _directory, server) = mock(replies);
    assert_eq!(client.verify_account(&account), Ok(()));
    assert_eq!(account.encode_public_state(), state_before);
    let methods = calls(server)
        .into_iter()
        .map(|call| {
            call["method"]
                .as_str()
                .unwrap_or_else(|| panic!("method"))
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        methods,
        [
            "getblockchaininfo",
            "getblockhash",
            "getdescriptorinfo",
            "deriveaddresses",
            "getdescriptorinfo",
            "deriveaddresses"
        ]
    );
}

#[test]
fn incompatible_descriptors_and_address_arrays_fail_closed() {
    let account = fixture();
    for (field, value) in [
        ("checksum", json!("wrong")),
        ("isrange", json!(false)),
        ("issolvable", json!(false)),
        ("hasprivatekeys", json!(true)),
    ] {
        let mut info =
            json!({"checksum":"hdxns7pj","isrange":true,"issolvable":true,"hasprivatekeys":false});
        info[field] = value;
        let (client, _directory, server) = mock(vec![
            (200, envelope(chain())),
            (200, envelope(json!(GENESIS))),
            (200, envelope(info)),
        ]);
        assert_eq!(
            client.verify_account(&account),
            Err(CoreError::IncompatibleDescriptor)
        );
        assert_eq!(calls(server).len(), 3);
    }
    for (addresses, expected) in [
        (json!([]), CoreError::InvalidResponse),
        (json!(["a"]), CoreError::InvalidResponse),
        (json!(["a", "b", "c"]), CoreError::InvalidResponse),
        (json!(["wrong", "wrong"]), CoreError::IncompatibleDescriptor),
    ] {
        let info =
            json!({"checksum":"hdxns7pj","isrange":true,"issolvable":true,"hasprivatekeys":false});
        let (client, _directory, server) = mock(vec![
            (200, envelope(chain())),
            (200, envelope(json!(GENESIS))),
            (200, envelope(info)),
            (200, envelope(addresses)),
        ]);
        assert_eq!(client.verify_account(&account), Err(expected));
        assert_eq!(calls(server).len(), 4);
    }
}

#[test]
fn no_arbitrary_rpc_method_can_be_invoked() {
    let directory = cookie_dir();
    let client = CoreTestnet4::new(directory.path()).unwrap_or_else(|_| panic!("client"));
    for method in [
        "createwallet",
        "importdescriptors",
        "sendrawtransaction",
        "signrawtransactionwithkey",
        "walletprocesspsbt",
        "submitpackage",
        "stop",
        "scantxoutset",
    ] {
        assert_eq!(client.rpc::<String>(method, json!([])), Err(CoreError::Rpc));
    }
    for method in ["scanblocks", "scantxoutset"] {
        for action in ["abort", "status"] {
            assert_eq!(
                client.rpc::<String>(method, json!([action])),
                Err(CoreError::Rpc)
            );
        }
    }
}

#[test]
fn timeout_and_disconnection_never_return_a_status() {
    let directory = cookie_dir();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap_or_else(|_| panic!("listener"));
    let address = listener.local_addr().unwrap_or_else(|_| panic!("address"));
    let mut client = CoreTestnet4::new(directory.path()).unwrap_or_else(|_| panic!("client"));
    client.endpoint = format!("http://{address}/");
    client.agent = ureq::Agent::config_builder()
        .proxy(None)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_millis(200)))
        .build()
        .into();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap_or_else(|_| panic!("accept"));
        request(&mut stream);
        thread::sleep(Duration::from_millis(400));
    });
    assert!(matches!(client.status(), Err(CoreError::Unavailable)));
    server.join().unwrap_or_else(|_| panic!("server"));
    assert!(matches!(client.status(), Err(CoreError::Unavailable)));
}

#[test]
fn redirect_target_never_receives_a_connection_or_cookie() {
    let directory = cookie_dir();
    let target = TcpListener::bind("127.0.0.1:0").unwrap_or_else(|_| panic!("redirect target"));
    target
        .set_nonblocking(true)
        .unwrap_or_else(|_| panic!("nonblocking target"));
    let destination = target
        .local_addr()
        .unwrap_or_else(|_| panic!("target address"));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap_or_else(|_| panic!("listener"));
    let address = listener.local_addr().unwrap_or_else(|_| panic!("address"));
    let mut client = CoreTestnet4::new(directory.path()).unwrap_or_else(|_| panic!("client"));
    client.endpoint = format!("http://{address}/");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap_or_else(|_| panic!("accept"));
        request(&mut stream);
        write!(stream, "HTTP/1.1 302 Found\r\nLocation: http://{destination}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .unwrap_or_else(|_| panic!("redirect response"));
    });
    assert!(matches!(client.status(), Err(CoreError::Unavailable)));
    server.join().unwrap_or_else(|_| panic!("redirect server"));
    assert!(
        matches!(target.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[test]
fn header_limit_and_body_timeout_are_enforced() {
    for oversized in [true, false] {
        let directory = cookie_dir();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap_or_else(|_| panic!("listener"));
        let address = listener.local_addr().unwrap_or_else(|_| panic!("address"));
        let mut client = CoreTestnet4::new(directory.path()).unwrap_or_else(|_| panic!("client"));
        client.endpoint = format!("http://{address}/");
        client.agent = ureq::Agent::config_builder()
            .proxy(None)
            .max_redirects(0)
            .max_response_header_size(4096)
            .timeout_global(Some(Duration::from_millis(200)))
            .build()
            .into();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap_or_else(|_| panic!("accept"));
            request(&mut stream);
            if oversized {
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nX-Oversized: {}\r\nContent-Length: 0\r\n\r\n",
                    "a".repeat(5000)
                );
            } else {
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n"
                )
                .unwrap_or_else(|_| panic!("headers"));
                stream.flush().unwrap_or_else(|_| panic!("flush"));
                // The global deadline must cover body reads, not just connection setup.
                thread::sleep(Duration::from_millis(400));
            }
        });
        assert!(matches!(client.status(), Err(CoreError::Unavailable)));
        server.join().unwrap_or_else(|_| panic!("server"));
    }
}
