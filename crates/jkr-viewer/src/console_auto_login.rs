//! EternalJK's `autoLogin` (`CG_Autologin_f`, `cg_consolecmds.c:840-888`): on a server
//! named by `cg_autoLoginServer1`..`3` (port 29070 when none is given), `amLogin` with its
//! `cg_autoLoginPass`. A listed name is also looked up, so a host name matches the address
//! the client is connected to.

use super::*;
use jkr_client::ClientSession;
use std::net::ToSocketAddrs;

/// The three servers and their passwords, archived, empty.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), jkr_shell::CvarError> {
    for slot in 1..=3 {
        cvars.register(CvarDefinition::new(
            &format!("cg_autoLoginServer{slot}"),
            "",
            CvarFlags::ARCHIVE,
            "EternalJK: a server autoLogin logs in to (address, port 29070 if none)",
        ))?;
        cvars.register(CvarDefinition::new(
            &format!("cg_autoLoginPass{slot}"),
            "",
            CvarFlags::ARCHIVE,
            "EternalJK: the amLogin password autoLogin sends to the server of the same number",
        ))?;
    }
    Ok(())
}

impl ViewerConsole {
    /// `autoLogin`: `amLogin` with the password of the first listed server this one is.
    pub(super) fn auto_login(
        &mut self,
        session: Option<&mut ClientSession>,
    ) -> Result<Vec<String>, String> {
        let session = session.ok_or("Not connected to a server.")?;
        let current = session.server();
        for slot in 1..=3 {
            let listed = self
                .text_value(&format!("cg_autoLoginServer{slot}"))
                .unwrap_or("")
                .trim()
                .to_owned();
            if listed.is_empty() {
                continue;
            }
            let listed = if listed.contains(':') {
                listed
            } else {
                format!("{listed}:29070")
            };
            let matches = listed.eq_ignore_ascii_case(&current.to_string())
                || listed
                    .to_socket_addrs()
                    .is_ok_and(|mut addresses| addresses.any(|address| address == current));
            if matches {
                let password = self
                    .text_value(&format!("cg_autoLoginPass{slot}"))
                    .unwrap_or("")
                    .to_owned();
                session
                    .send_reliable_command(format!("amLogin {password}").as_bytes())
                    .map_err(|error| error.to_string())?;
                return Ok(Vec::new());
            }
        }
        Ok(Vec::new())
    }
}
