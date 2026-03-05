use std::path::PathBuf;

use mpris::PlayerFinder;

use crate::player::{album_art_path_from_metadata, playback_state_from_player};
use crate::window::PlaybackState;

#[derive(Clone, Debug)]
pub struct NowPlayingData {
    pub text: String,
    pub title: String,
    pub artist: String,
    pub state: PlaybackState,
    pub album_art_path: Option<PathBuf>,
}

pub fn now_playing_snapshot() -> NowPlayingData {
    let finder = PlayerFinder::new();

    if let Ok(finder) = finder {
        if let Ok(player) = finder.find_active() {
            return now_playing_from_player(&player);
        }
    }

    NowPlayingData {
        text: "Nothing playing".to_string(),
        title: "Nothing playing".to_string(),
        artist: String::new(),
        state: PlaybackState::Stopped,
        album_art_path: None,
    }
}

pub fn now_playing_from_player(player: &mpris::Player) -> NowPlayingData {
    let playback_state = playback_state_from_player(player);

    if let Ok(meta) = player.get_metadata() {
        let title = meta.title().unwrap_or("Unknown");
        let artist = meta
            .artists()
            .and_then(|a| a.first().copied())
            .unwrap_or("Unknown");
        let album_art_path = album_art_path_from_metadata(&meta);

        return NowPlayingData {
            text: format!("{} - {}", title, artist),
            title: title.to_string(),
            artist: artist.to_string(),
            state: playback_state,
            album_art_path,
        };
    }

    NowPlayingData {
        text: "Nothing playing".to_string(),
        title: "Nothing playing".to_string(),
        artist: String::new(),
        state: playback_state,
        album_art_path: None,
    }
}
