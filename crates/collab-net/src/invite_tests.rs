//! Invite links: both forms read back, bad links are refused without echoing them, and the
//! parser survives anything.

use proptest::prelude::*;

use crate::invite::Invite;

const TOKEN: &str = "AbCdEfGhIjKlMnOpQrStUv";

fn invite() -> Invite {
    Invite { host: "192.168.1.20:47900".into(), file: "0123456789abcdef".into(), token: TOKEN.into() }
}

#[test]
fn both_forms_read_back() {
    let i = invite();
    assert_eq!(i.link(), format!("figcraft://join?host=192.168.1.20:47900&file=0123456789abcdef&token={TOKEN}"));
    assert_eq!(Invite::parse(&i.link()), Ok(i.clone()));
    assert_eq!(Invite::parse(&i.url()), Ok(i.clone()));
    // Pasted with spaces, quotes or angle brackets, or with escapes.
    assert_eq!(Invite::parse(&format!("  <{}>\n", i.link())), Ok(i.clone()));
    assert_eq!(Invite::parse(&format!("\"{}\"", i.url())), Ok(i.clone()));
    assert_eq!(Invite::parse(&format!("figcraft://join?host=192.168.1.20%3A47900&file=0123456789abcdef&token={TOKEN}")), Ok(i.clone()));
    // The secret never travels in the WebSocket request.
    assert_eq!(i.endpoint(), "ws://192.168.1.20:47900/");
}

#[test]
fn hosts_can_be_v6_or_names() {
    let v6 = Invite { host: "[::1]:47900".into(), ..invite() };
    assert_eq!(Invite::parse(&v6.link()), Ok(v6.clone()));
    assert_eq!(Invite::parse(&v6.url()), Ok(v6));
    let name = Invite { host: "studio.local:47900".into(), ..invite() };
    assert_eq!(Invite::parse(&name.link()), Ok(name));
}

#[test]
fn bad_links_are_refused_without_echoing_them() {
    let secret = "SuperSecretToken123456";
    for bad in [
        String::new(),
        "hello".into(),
        format!("https://example.com/?token={secret}"),
        format!("figcraft://node/12?token={secret}"),
        format!("figcraft://join?file=abc&token={secret}"),
        format!("figcraft://join?host=10.0.0.1&file=abc&token={secret}"),
        format!("figcraft://join?host=10.0.0.1:0&file=abc&token={secret}"),
        format!("figcraft://join?host=10.0.0.1:47900&token={secret}"),
        "figcraft://join?host=10.0.0.1:47900&file=abc&token=short".into(),
        format!("figcraft://join?host=10.0.0.1:47900&file=a/b&token={secret}"),
        format!("figcraft://join?host=10.0.0.1:47900&file=abc&token={secret}%zz"),
        format!("wss://10.0.0.1:47900/?file=abc&token={secret}"),
        format!("figcraft://join?host=10.0.0.1:47900&file=abc&token={}", "x".repeat(3000)),
    ] {
        let e = Invite::parse(&bad).expect_err(&bad);
        assert!(!e.contains(secret), "{e}");
    }
}

proptest! {
    #[test]
    fn the_parser_survives_anything(s in ".{0,300}") {
        let _ = Invite::parse(&s);
        let _ = Invite::parse(&format!("figcraft://join?{s}"));
        let _ = Invite::parse(&format!("ws://{s}"));
    }

    #[test]
    fn valid_invites_round_trip(a in 1u8..255, port in 1u16.., file in "[a-f0-9]{1,64}", token in "[A-Za-z0-9_-]{16,128}") {
        let i = Invite { host: format!("10.0.0.{a}:{port}"), file, token };
        prop_assert_eq!(Invite::parse(&i.link()), Ok(i.clone()));
        prop_assert_eq!(Invite::parse(&i.url()), Ok(i));
    }
}
