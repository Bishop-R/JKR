//! `svrecord` and `svstoprecord` (`SV_Record_f`, `SV_StopRecord_f`, `SV_RecordDemo`,
//! `SV_StopRecordDemo`).
use super::{Arguments, LegacyConsoleHost};
use crate::LegacyClientPhase;

/// `PROTOCOL_VERSION`, the demo file's extension.
const PROTOCOL: i32 = 26;
/// `MAX_OSPATH`: a demo's name is cut to this, less one.
const OSPATH_BYTES: usize = 256;

fn atoi(text: &[u8]) -> i32 {
    crate::server_bans::atoi_text(text)
}

/// `SV_Record_f`: a demo of the client named by number, or of the first in the game not
/// recorded yet, under the name given or `demo<date and time>`.
pub(super) fn record(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) {
    let count = arguments.count();
    if count > 3 {
        print(b"record <demoname> <clientnum>\n");
        return;
    }
    let client = if count == 3 {
        let index = atoi(arguments.get(2));
        match usize::try_from(index)
            .ok()
            .filter(|&client| client < host.client_count())
        {
            Some(client) => client,
            None => {
                print(format!("Unknown client number {index}.\n").as_bytes());
                return;
            }
        }
    } else {
        let found = (0..host.client_count()).find(|&client| {
            host.phase(client) != LegacyClientPhase::Free
                && !host.demo_recording(client)
                && host.phase(client) == LegacyClientPhase::Active
        });
        let Some(client) = found else {
            print(b"No active client could be found.\n");
            return;
        };
        client
    };
    if host.demo_recording(client) {
        print(b"Already recording.\n");
        return;
    }
    if host.phase(client) != LegacyClientPhase::Active {
        print(b"Client is not active.\n");
        return;
    }
    let name = if count >= 2 {
        let name = arguments.get(1);
        name[..name.len().min(OSPATH_BYTES - 1)].to_vec()
    } else {
        let name = format!("demo{}", host.timestamp()).into_bytes();
        if host.file_exists(&path(&name)) {
            print(b"Record: Couldn't create a file\n");
            return;
        }
        name
    };
    // `SV_RecordDemo`.
    let path = path(&name);
    print(format!("recording to {path}.\n").as_bytes());
    if !host.start_demo(client, &name, &path) {
        print(b"ERROR: couldn't open.\n");
    }
}

/// `demos/<name>.dm_26`.
fn path(name: &[u8]) -> String {
    format!("demos/{}.dm_{PROTOCOL}", String::from_utf8_lossy(name))
}

/// `SV_StopRecord_f`: the demo of the client named, or of the first recorded.
pub(super) fn stop(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) {
    let client = if arguments.count() == 2 {
        let index = atoi(arguments.get(1));
        match usize::try_from(index)
            .ok()
            .filter(|&client| client < host.client_count())
        {
            Some(client) => client,
            None => {
                print(format!("Unknown client number {index}.\n").as_bytes());
                return;
            }
        }
    } else {
        let Some(client) = (0..host.client_count()).find(|&client| host.demo_recording(client))
        else {
            print(b"No demo being recorded.\n");
            return;
        };
        client
    };
    // `SV_StopRecordDemo`.
    if !host.demo_recording(client) {
        print(format!("Client {client} is not recording a demo.\n").as_bytes());
        return;
    }
    host.stop_demo(client);
    print(format!("Stopped demo for client {client}.\n").as_bytes());
}

/// The client a TaystJK demo command names (`atoi`), or `Unknown client number N.`.
fn named_client(
    host: &impl LegacyConsoleHost,
    text: &[u8],
    print: &mut dyn FnMut(&[u8]),
) -> Option<usize> {
    let index = atoi(text);
    let client = usize::try_from(index)
        .ok()
        .filter(|&client| client < host.client_count());
    if client.is_none() {
        print(format!("Unknown client number {index}.\n").as_bytes());
    }
    client
}

/// TaystJK's `svdemometa <clientnum> <metakey> [metadata]` (`SV_DemoMeta_f`): a key of
/// letters only, set for the client's next demos, or taken out without a value. TaystJK's
/// own test leaves out `a` and `z` (it compares with `>` and `<`); every letter is let in.
pub(super) fn meta(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) {
    let count = arguments.count();
    if count != 3 && count != 4 {
        print(b"svdemometa <clientnum> <metakey> <metadata>, for example svdemometa 2 runstarttime 235254. Leave out <metadata> to remove key.\n");
        return;
    }
    let Some(client) = named_client(host, arguments.get(1), print) else {
        return;
    };
    let key = arguments.get(2);
    if key.is_empty() {
        print(b"Metadata key must not be empty\n");
        return;
    }
    if !key.iter().all(u8::is_ascii_alphabetic) {
        print(b"Metadata key must only contain a-z (lower or upper case)\n");
        return;
    }
    let value = if count == 4 { arguments.get(3) } else { b"" };
    host.set_demo_meta(
        client,
        &String::from_utf8_lossy(key),
        &String::from_utf8_lossy(value),
    );
}

/// TaystJK's `svdemoclearmeta <clientnum>` (`SV_DemoClearMeta_f`).
pub(super) fn clear_meta(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) {
    if arguments.count() != 2 {
        print(b"svdemoclearmeta <clientnum>\n");
        return;
    }
    if let Some(client) = named_client(host, arguments.get(1), print) {
        host.clear_demo_meta(client);
    }
}

/// TaystJK's `svdemoclearprerecord <clientnum>` (`SV_DemoClearPreRecord_f`).
pub(super) fn clear_pre_record(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) {
    if arguments.count() != 2 {
        print(b"svdemoclearprerecord <clientnum>\n");
        return;
    }
    if let Some(client) = named_client(host, arguments.get(1), print) {
        host.clear_demo_pre_record(client);
    }
}

/// TaystJK's `svrenamedemo <from> <to>` (`SV_RenameDemo_f`): both under `demos/`, neither
/// climbing out of it (`FS_CheckDirTraversal`); silent either way.
pub(super) fn rename(host: &mut impl LegacyConsoleHost, arguments: &Arguments) {
    if arguments.count() != 3 {
        return;
    }
    let (from, to) = (path(arguments.get(1)), path(arguments.get(2)));
    let climbs = |path: &str| path.contains("../") || path.contains("..\\");
    if climbs(&from) || climbs(&to) {
        return;
    }
    host.rename_file(&from, &to);
}

/// TaystJK's `sv_listrecording` (`SV_ListRecording_f`).
pub(super) fn list(host: &impl LegacyConsoleHost, print: &mut dyn FnMut(&[u8])) {
    print(b"Demos currently being recorded:\n");
    for client in 0..host.client_count() {
        if let Some(name) = host.demo_name(client) {
            print(format!("Client {client} ({})\n", String::from_utf8_lossy(name)).as_bytes());
        }
    }
}
