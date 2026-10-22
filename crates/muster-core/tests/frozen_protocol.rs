//! Frozen encodings of what a client and a daemon of any two versions must
//! always read of each other (`ARC-09`): the hello a client opens with,
//! the daemon's welcome, its refusal that tells a client to upgrade
//! (`Incompatible`), the error that turns away a frame before the hello,
//! and the frame header (`muster_core::frame`) all of them come in. Each
//! protocol version's are kept as whole frames in `frozen/v<N>/`, and this
//! test fails if today's code reads any of them differently or encodes the
//! same message in other bytes: an older UI must always reach a newer
//! daemon and be told clearly to upgrade.
//!
//! Adding a protocol version's set, when `PROTOCOL_VERSION` is raised:
//! 1. run `cargo test -p muster-core --test frozen_protocol -- --ignored`,
//!    which writes `frozen/v<N>/` for the new version and prints its line;
//!    it never touches a set already there;
//! 2. add that line to [`FROZEN`], and commit the files with the change.
//!
//! A set once committed is never edited or regenerated. If one stops
//! matching, the handshake changed shape, which older clients cannot
//! read: put the change in a later message instead. Messages that grow
//! from version to version (the state, commands, replies) are not frozen
//! here; `protocol_round_trip` covers them.

#![expect(
    clippy::unwrap_used,
    reason = "a test: a failed step is the test failing"
)]

use muster_core::frame::{self, FrameDecoder};
use muster_core::protocol::{
    ClientFrame, ErrorBody, ErrorCode, Hello, Incompatible, MIN_PROTOCOL_VERSION, PROTOCOL_VERSION,
    ServerFrame, Welcome,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// One protocol version's frozen frames.
struct Frozen {
    version: u32,
    /// The oldest version that version still spoke.
    min: u32,
    /// The release it came with, which its frames name.
    release: &'static str,
    hello: &'static [u8],
    welcome: &'static [u8],
    incompatible: &'static [u8],
    out_of_order: &'static [u8],
}

macro_rules! frozen {
    ($version:literal, $min:literal, $release:literal) => {
        Frozen {
            version: $version,
            min: $min,
            release: $release,
            hello: include_bytes!(concat!("frozen/v", $version, "/hello.msgpack")),
            welcome: include_bytes!(concat!("frozen/v", $version, "/welcome.msgpack")),
            incompatible: include_bytes!(concat!("frozen/v", $version, "/incompatible.msgpack")),
            out_of_order: include_bytes!(concat!("frozen/v", $version, "/out-of-order.msgpack")),
        }
    };
}

/// Every version's set, oldest first; one is added per protocol version.
const FROZEN: &[Frozen] = &[
    frozen!(28, 16, "0.3.25"),
    frozen!(30, 16, "0.3.25"),
    frozen!(31, 16, "0.3.25"),
    frozen!(32, 16, "0.3.25"),
    frozen!(33, 16, "0.3.25"),
    frozen!(34, 16, "0.3.26"),
    frozen!(35, 16, "0.3.27"),
    frozen!(36, 16, "0.3.29"),
    frozen!(37, 16, "0.3.29"),
    frozen!(38, 16, "0.3.29"),
    frozen!(39, 16, "0.3.29"),
    frozen!(40, 16, "0.3.31"),
    frozen!(41, 16, "0.3.35"),
    frozen!(42, 16, "0.3.35"),
    frozen!(43, 16, "0.3.35"),
    frozen!(44, 16, "0.4.0"),
    frozen!(45, 16, "0.4.1"),
    frozen!(46, 16, "0.5.1"),
    frozen!(47, 16, "0.5.2"),
    frozen!(48, 16, "0.5.4"),
    frozen!(49, 16, "0.5.4"),
    frozen!(50, 16, "0.5.4"),
    frozen!(51, 16, "0.5.6"),
];

/// The process id the frozen replies name.
const PID: u32 = 4242;

/// The messages of one version's set, as that version sent them.
struct Messages {
    hello: ClientFrame,
    welcome: ServerFrame,
    incompatible: ServerFrame,
    out_of_order: ServerFrame,
}

fn messages(version: u32, min: u32, release: &str) -> Messages {
    Messages {
        hello: ClientFrame::Hello(Hello {
            min_version: min,
            max_version: version,
            client: format!("muster {release}"),
            capabilities: Vec::new(),
        }),
        welcome: ServerFrame::Welcome(Welcome {
            version,
            daemon_pid: PID,
            daemon_version: release.to_owned(),
        }),
        incompatible: ServerFrame::Incompatible(Incompatible {
            min_version: min,
            max_version: version,
            daemon_version: release.to_owned(),
            daemon_pid: PID,
        }),
        out_of_order: ServerFrame::Error(ErrorBody::new(ErrorCode::OutOfOrder, "send hello first")),
    }
}

/// `bytes` is one whole frame of `expected`: its header the payload's
/// length, little-endian; read as `expected`; and `expected` encoded today
/// in exactly these bytes.
fn assert_frozen<T>(name: &str, version: u32, bytes: &[u8], expected: &T)
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let (header, payload) = bytes.split_first_chunk::<4>().unwrap();
    assert_eq!(
        u32::from_le_bytes(*header) as usize,
        payload.len(),
        "v{version} {name}: the header is the payload's length"
    );
    let mut decoder = FrameDecoder::new();
    decoder.push(bytes);
    let read: T = decoder.next_frame().unwrap().unwrap();
    assert_eq!(&read, expected, "v{version} {name} reads differently");
    assert!(!decoder.has_partial_frame());
    assert_eq!(
        frame::encode(expected).unwrap(),
        bytes,
        "v{version} {name} is encoded in other bytes: the handshake changed shape"
    );
}

#[test]
fn every_versions_handshake_reads_and_encodes_as_it_was_frozen_arc_09() {
    for set in FROZEN {
        let expected = messages(set.version, set.min, set.release);
        assert_frozen("hello", set.version, set.hello, &expected.hello);
        assert_frozen("welcome", set.version, set.welcome, &expected.welcome);
        assert_frozen(
            "incompatible",
            set.version,
            set.incompatible,
            &expected.incompatible,
        );
        assert_frozen(
            "out-of-order",
            set.version,
            set.out_of_order,
            &expected.out_of_order,
        );
    }
}

#[test]
fn this_protocol_version_has_its_frozen_set_arc_09() {
    let newest = FROZEN.last().unwrap();
    assert_eq!(
        (newest.version, newest.min),
        (PROTOCOL_VERSION, MIN_PROTOCOL_VERSION),
        "protocol {PROTOCOL_VERSION} has no frozen handshake: run \
         `cargo test -p muster-core --test frozen_protocol -- --ignored` and add the line it prints"
    );
    assert!(
        FROZEN
            .windows(2)
            .all(|pair| pair[0].version < pair[1].version),
        "one set per version, oldest first"
    );
}

/// Writes this protocol version's set, if it has none: step 1 above.
#[test]
#[ignore = "run by hand when the protocol version is raised"]
fn write_this_versions_frozen_set() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/frozen")
        .join(format!("v{PROTOCOL_VERSION}"));
    assert!(
        !dir.exists(),
        "{} is there already, and a frozen set is never written again",
        dir.display()
    );
    let release = env!("CARGO_PKG_VERSION");
    let set = messages(PROTOCOL_VERSION, MIN_PROTOCOL_VERSION, release);
    std::fs::create_dir_all(&dir).unwrap();
    let files = [
        ("hello", frame::encode(&set.hello)),
        ("welcome", frame::encode(&set.welcome)),
        ("incompatible", frame::encode(&set.incompatible)),
        ("out-of-order", frame::encode(&set.out_of_order)),
    ];
    for (name, bytes) in files {
        std::fs::write(dir.join(format!("{name}.msgpack")), bytes.unwrap()).unwrap();
    }
    println!(
        "wrote {}; add to FROZEN: frozen!({PROTOCOL_VERSION}, {MIN_PROTOCOL_VERSION}, \"{release}\")",
        dir.display()
    );
}
