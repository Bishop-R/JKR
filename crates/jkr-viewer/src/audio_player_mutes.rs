//! Local player mutes apply before both immediate and decode-deferred starts.
use super::*;

pub(super) fn blocked(mask: u32, command: &AudioCommand) -> bool {
    match command {
        AudioCommand::Play(_, request) | AudioCommand::SetLoop(_, request, _) => {
            request.source.0 < 32 && mask & (1 << request.source.0) != 0
        }
        _ => false,
    }
}

impl AudioOutput {
    /// Cancel existing channels and reject subsequent starts until unmuted.
    pub(crate) fn set_muted_players(&mut self, mask: u32) {
        let added = mask & !self.muted_players;
        self.muted_players = mask;
        for slot in 0..32 {
            if added & (1 << slot) == 0 {
                continue;
            }
            let source = SourceId(slot);
            // q_shared.h:855-868, CHAN_AUTO through CHAN_MUSIC.
            for channel in 0..14 {
                self.send(AudioCommand::StopChannel(source, ChannelId(channel)));
            }
            self.send(AudioCommand::StopLoops(source));
        }
    }
}
