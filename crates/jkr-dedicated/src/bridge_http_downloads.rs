//! TaystJK's HTTP downloads on this server (`sv_httpDownloads`, `sv_httpServerPort`;
//! `sv_init.cpp:755-775`, `sv_main.cpp:578-585`): with the option on, a web server
//! ([`crate::http_downloads`]) serves the pak a client could download over the game's
//! own channel, to the clients in the game, and `getinfo` says where (`mvhttp`). A port
//! setting that is a web address (`http://...`) sends clients there instead
//! (`mvhttpurl`) and runs no server. Like the reference's, the settings take effect with
//! the next map.

use super::*;
use crate::http_downloads::HttpDownloads;

/// `Q_stristr(text, "http://")`.
fn names_web_address(text: &[u8]) -> bool {
    text.windows(7)
        .any(|window| window.eq_ignore_ascii_case(b"http://"))
}

impl NativeGame {
    /// `SV_SpawnServer`'s web server: started, or moved to a newly named port, while the
    /// option is on and the port is not a web address; stopped otherwise. Then told the
    /// new map's pak and the clients in the game. A server that cannot start says so
    /// and the map goes on (the reference drops it with an error).
    pub(super) fn refresh_http_downloads(&mut self) {
        let enabled = self.cvars.integer(b"sv_httpDownloads") != 0;
        let setting = self.cvars.string(b"sv_httpServerPort").to_vec();
        if !enabled || names_web_address(&setting) {
            self.http_downloads = None;
            if enabled {
                println!(
                    "HTTP Downloads: redirecting to {}",
                    String::from_utf8_lossy(&setting)
                );
            }
            return;
        }
        let wanted = u16::try_from(self.cvars.integer(b"sv_httpServerPort")).unwrap_or(0);
        let running = (self.http_downloads.as_ref())
            .is_some_and(|server| wanted == 0 || server.port() == wanted);
        if !running {
            self.http_downloads = None;
            match HttpDownloads::start(wanted) {
                Ok(server) => {
                    println!(
                        "HTTP Downloads: webserver running on port {}...",
                        server.port()
                    );
                    self.http_downloads = Some(server);
                }
                Err(error) => println!("HTTP Downloads: webserver startup failed: {error}"),
            }
        }
        self.publish_http_downloads();
    }

    /// What the web server may serve and to whom: the map's downloadable pak, and the
    /// addresses of the clients in the game (`NET_HTTP_AllowClient`).
    pub(super) fn publish_http_downloads(&self) {
        let Some(server) = &self.http_downloads else {
            return;
        };
        server.set_files(
            bridge_maps::downloadable_pak(self.map.as_ref())
                .map(|pak| (format!("{}.pk3", pak.name), pak.path.clone())),
        );
        server.set_clients((0..self.players.places()).filter_map(|client| {
            let userinfo = &self.peer(client)?.userinfo;
            let address = jkr_protocol::info_value(userinfo, b"ip")?;
            let address = String::from_utf8_lossy(address);
            address.split(':').next()?.parse().ok()
        }));
    }

    /// What `getinfo` advertises: the web address (`mvhttpurl`) or the port of the running
    /// server (`mvhttp`), each only while the option is on.
    pub(super) fn http_advert(&self) -> (u16, &[u8]) {
        if self.cvars.integer(b"sv_httpDownloads") == 0 {
            return (0, &[]);
        }
        let setting = self.cvars.string(b"sv_httpServerPort");
        if names_web_address(setting) {
            return (0, setting);
        }
        (self.http_downloads.as_ref().map_or(0, HttpDownloads::port), &[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_web_address_anywhere_in_the_setting_redirects() {
        assert!(names_web_address(b"http://dl.example.org/"));
        assert!(names_web_address(b"x HTTP://y"));
        assert!(!names_web_address(b"18200"));
        assert!(!names_web_address(b"https://dl.example.org"));
    }
}
