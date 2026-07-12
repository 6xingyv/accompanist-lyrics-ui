//! Linux desktop media-session integration through MPRIS2/D-Bus.

use super::{decode_artwork, media_identity_key_from_parts, Artwork, PlaybackSnapshot};
use mpris::{PlaybackStatus, Player, PlayerFinder};
use std::cell::RefCell;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

const DBUS_TIMEOUT_MS: i32 = 400;
const MAX_ARTWORK_BYTES: u64 = 32 * 1024 * 1024;
const ARTWORK_HTTP_TIMEOUT: Duration = Duration::from_secs(3);

thread_local! {
    // The polling and seek workers each get one long-lived session-bus
    // connection. Players are still rediscovered for every operation so a
    // stopped player cannot leave a stale handle behind.
    static PLAYER_FINDER: RefCell<Option<PlayerFinder>> = const { RefCell::new(None) };
}

pub(super) fn current_playback_snapshot(
    cached_media_key: &str,
    cached_artwork: Option<Arc<Artwork>>,
) -> Result<PlaybackSnapshot, String> {
    with_active_player(|player| {
        let metadata = player.get_metadata().map_err(|error| error.to_string())?;

        let title = metadata.title().unwrap_or_default().to_owned();
        let artist = metadata.artists().unwrap_or_default().join(", ");
        let duration_ms = duration_to_ms(metadata.length());
        let media_key = media_identity_key_from_parts(&artist, &title, duration_ms);
        let artwork = if media_key == cached_media_key && cached_artwork.is_some() {
            cached_artwork
        } else {
            metadata
                .art_url()
                .and_then(load_artwork_url)
                .and_then(|bytes| decode_artwork(&bytes))
                .map(Arc::new)
        };

        let is_playing = player
            .get_playback_status()
            .map_err(|error| error.to_string())?
            == PlaybackStatus::Playing;
        let position_ms = player
            .checked_get_position()
            .map_err(|error| error.to_string())?
            .map(|position| duration_to_ms(Some(position)))
            .unwrap_or_default();

        Ok(PlaybackSnapshot {
            title,
            artist,
            position_ms,
            duration_ms,
            is_playing,
            // Prefix the stable bus identity so it can never be mistaken for the
            // Windows Apple Music publisher by the platform-neutral clock adapter.
            source_app_id: format!("mpris:{}", player.bus_name_trimmed()),
            smtc_update_ticks: unix_time_ms(),
            artwork,
        })
    })
}

pub(super) fn seek_position_ms(position_ms: i32) -> Result<(), String> {
    with_active_player(|player| {
        let metadata = player.get_metadata().map_err(|error| error.to_string())?;
        let track_id = metadata
            .track_id()
            .ok_or_else(|| "active player did not publish a track id".to_string())?;
        let accepted = player
            .checked_set_position(track_id, &Duration::from_millis(position_ms.max(0) as u64))
            .map_err(|error| error.to_string())?;
        if !accepted {
            return Err("active player does not currently accept seeking".to_string());
        }
        Ok(())
    })
}

fn with_active_player<T>(
    operation: impl FnOnce(&Player) -> Result<T, String>,
) -> Result<T, String> {
    PLAYER_FINDER.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            let mut finder = PlayerFinder::new().map_err(|error| error.to_string())?;
            finder.set_player_timeout_ms(DBUS_TIMEOUT_MS);
            *slot = Some(finder);
        }
        let player = slot
            .as_ref()
            .expect("MPRIS finder was initialized")
            .find_active()
            .map_err(|error| error.to_string())?;
        operation(&player)
    })
}

fn duration_to_ms(value: Option<Duration>) -> i32 {
    value
        .map(|duration| duration.as_millis().min(i32::MAX as u128) as i32)
        .unwrap_or_default()
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

fn load_artwork_url(value: &str) -> Option<Vec<u8>> {
    let url = Url::parse(value).ok()?;
    match url.scheme() {
        "file" => {
            let path = url.to_file_path().ok()?;
            let size = fs::metadata(&path).ok()?.len();
            (size <= MAX_ARTWORK_BYTES)
                .then(|| fs::read(path).ok())
                .flatten()
        }
        "http" | "https" => {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(ARTWORK_HTTP_TIMEOUT))
                .build()
                .into();
            let mut response = agent.get(value).call().ok()?;
            response
                .body_mut()
                .with_config()
                .limit(MAX_ARTWORK_BYTES)
                .read_to_vec()
                .ok()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_conversion_is_millisecond_based_and_saturating() {
        assert_eq!(
            duration_to_ms(Some(Duration::from_micros(1_234_999))),
            1_234
        );
        assert_eq!(duration_to_ms(None), 0);
        assert_eq!(
            duration_to_ms(Some(Duration::from_secs(u64::MAX))),
            i32::MAX
        );
    }
}
