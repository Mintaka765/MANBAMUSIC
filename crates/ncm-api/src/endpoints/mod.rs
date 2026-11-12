pub mod album;
pub mod artist;
pub mod like;
pub mod recommend;
pub mod login;
pub mod search;
pub mod song;

pub use album::{album, AlbumDetail};
pub use artist::{artist, artist_albums, ArtistAlbum, ArtistDetail};
pub use like::{like, liked_ids};
pub use recommend::{daily_songs, private_radar_id, radio_get};
pub use login::{account, playlist_tracks, qr_check, qr_unikey, user_playlists, QrStatus};
pub use search::search;
pub use song::{lyric, play_url, play_url_with_fallback};



