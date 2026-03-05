use std::path::PathBuf;

use mpris::{PlaybackStatus, PlayerFinder};

use crate::window::PlaybackState;

pub fn playback_state_from_player(player: &mpris::Player) -> PlaybackState {
    match player.get_playback_status() {
        Ok(PlaybackStatus::Playing) => PlaybackState::Playing,
        Ok(PlaybackStatus::Paused) => PlaybackState::Paused,
        Ok(PlaybackStatus::Stopped) => PlaybackState::Stopped,
        Err(_) => PlaybackState::Unknown,
    }
}

pub fn album_art_path_from_metadata(meta: &mpris::Metadata) -> Option<PathBuf> {
    let art_url = meta.art_url()?;
    art_url.strip_prefix("file://").map(PathBuf::from)
}

pub fn with_active_player<F>(f: F)
where
    F: FnOnce(&mpris::Player),
{
    if let Ok(finder) = PlayerFinder::new() {
        if let Ok(player) = finder.find_active() {
            f(&player);
        }
    }
}
