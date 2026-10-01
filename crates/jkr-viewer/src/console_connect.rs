//! Local console connection-command parsing.

use jkr_client::LegacyServerAddress;

/// A connection action requested by the local console.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    /// Tear down any current session and connect to this normalized address.
    Connect(String),
    /// Cancel an in-progress connection or leave the current server.
    Disconnect,
    /// Connect to the most recently requested address.
    Reconnect,
}

/// Parse commands registered by OpenJK at `codemp/client/cl_main.cpp:2879-2882`.
///
/// `CL_Connect_f` requires one argument and saves it for `CL_Reconnect_f`
/// (`cl_main.cpp:994-1068`). `CL_Disconnect_f` tears down the active session
/// at `cl_main.cpp:976-986`.
pub(crate) fn parse(tokens: &[String]) -> Result<Option<Action>, String> {
    let Some(name) = tokens.first() else {
        return Ok(None);
    };
    if name.eq_ignore_ascii_case("connect") {
        if tokens.len() != 2 {
            return Err("usage: connect <host[:port]>".to_owned());
        }
        return LegacyServerAddress::parse(&tokens[1])
            .map(|address| Some(Action::Connect(address.into_string())))
            .map_err(|error| format!("Bad server address: {error}"));
    }
    if name.eq_ignore_ascii_case("disconnect") {
        return exact_arity(tokens, Action::Disconnect, "usage: disconnect");
    }
    if name.eq_ignore_ascii_case("reconnect") {
        return exact_arity(tokens, Action::Reconnect, "usage: reconnect");
    }
    Ok(None)
}

fn exact_arity(
    tokens: &[String],
    action: Action,
    usage: &'static str,
) -> Result<Option<Action>, String> {
    if tokens.len() == 1 {
        Ok(Some(action))
    } else {
        Err(usage.to_owned())
    }
}
