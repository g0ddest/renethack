use nh_protocol::{EngineMsg, PROTOCOL_VERSION, WinCall};

use crate::LinkError;

/// Ordering rules for the start of a session: hello first (in our protocol
/// version), then the catalog before any window call or request. Only
/// `raw_print` may come early (the engine's own fatal messages).
#[derive(Debug, Default)]
pub(crate) struct Handshake {
    hello: bool,
    catalog: bool,
    count: usize,
}

impl Handshake {
    pub(crate) fn new() -> Handshake {
        Handshake::default()
    }

    /// Check the next message in order.
    pub(crate) fn check(&mut self, msg: &EngineMsg) -> Result<(), LinkError> {
        self.count += 1;
        match msg {
            EngineMsg::Hello(h) => {
                if self.count != 1 || self.hello {
                    return Err(LinkError::Handshake(
                        "hello is not the first message".into(),
                    ));
                }
                if h.protocol != PROTOCOL_VERSION {
                    return Err(LinkError::Handshake(format!(
                        "engine speaks protocol {}, client {}",
                        h.protocol, PROTOCOL_VERSION
                    )));
                }
                self.hello = true;
            }
            _ if !self.hello => {
                return Err(LinkError::Handshake("first message is not hello".into()));
            }
            EngineMsg::Catalog(_) => self.catalog = true,
            EngineMsg::Win(WinCall::RawPrint { .. }) => {}
            EngineMsg::Win(_) | EngineMsg::Req { .. } if !self.catalog => {
                return Err(LinkError::Handshake(
                    "window call before the catalog".into(),
                ));
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nh_protocol::{Hello, Request};

    fn hello(protocol: u32) -> EngineMsg {
        EngineMsg::Hello(Hello {
            protocol,
            engine: "5.0.0".into(),
            patchset: String::new(),
        })
    }

    #[test]
    fn hello_must_come_first_and_speak_our_version() {
        assert!(Handshake::new().check(&EngineMsg::Bye).is_err());
        assert!(
            Handshake::new()
                .check(&hello(PROTOCOL_VERSION + 1))
                .is_err()
        );
        let mut hs = Handshake::new();
        hs.check(&hello(PROTOCOL_VERSION)).unwrap();
        assert!(hs.check(&hello(PROTOCOL_VERSION)).is_err());
    }

    #[test]
    fn window_calls_wait_for_the_catalog_except_raw_print() {
        let mut hs = Handshake::new();
        hs.check(&hello(PROTOCOL_VERSION)).unwrap();
        let raw = EngineMsg::Win(WinCall::RawPrint {
            text: "early".into(),
            bold: false,
        });
        hs.check(&raw).unwrap();
        hs.check(&EngineMsg::Error { msg: "x".into() }).unwrap();
        let req = EngineMsg::Req {
            id: 1,
            req: Request::Askname,
        };
        assert!(hs.check(&req).is_err());
        assert!(hs.check(&EngineMsg::Win(WinCall::InitNhwindows)).is_err());
    }
}
