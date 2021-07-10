use std::{
    collections::HashMap,
    fs,
    hash::{Hash, Hasher},
    ops::{Deref, DerefMut},
};

use anyhow::{bail, Context as _, Result};
use dotenv::dotenv;
use futures::prelude::*;
use rspotify::{
    model::{
        idtypes::{Playlist, User},
        FullTrack, Id, IdBuf, PlayableItem,
    },
    prelude::*,
    AuthCodeSpotify, Config, Credentials, OAuth,
};

const ME: &str = "ianlum314";
// const ROHANDLE: &str = "m2ytpjv2g8kwekqgmzszqb7vw";

// const TAYLOR_PLAYLIST: &str = "56HuNeZLQRZN4uqcFm47Fm";
const TEST_PLAYLIST: &str = "7HkiK8ErJKGm9aYcMKLbaC";
// const ROHANDLE_PLAYLIST: &str = "3uidlqZoGHGSbRd9TTcIAZ";

async fn setup() -> Result<AuthCodeSpotify> {
    dotenv().ok();
    let creds = Credentials::from_env().unwrap();
    let oauth = OAuth::from_env(rspotify::scopes!(
        "playlist-modify-public",
        "playlist-read-collaborative"
    ))
    .unwrap();
    let mut spotify = AuthCodeSpotify::with_config(
        creds.clone(),
        oauth.clone(),
        Config {
            token_cached: true,
            ..Default::default()
        },
    );

    let url = spotify.get_authorize_url(false).unwrap();
    spotify
        .prompt_for_token(&url)
        .await
        .expect("Couldn't authenticate successfully");

    let token = spotify
        .read_token_cache()
        .await
        .context("No cached token found")?;

    let refresh_token = token
        .refresh_token
        .as_ref()
        .context("No refresh token present in token cache")?;

    spotify.refresh_token(refresh_token).await?;
    Ok(spotify)
}

async fn create_weight_map(
    spotify: &AuthCodeSpotify,
    source_playlist_id: &Id<Playlist>,
) -> Result<HashMap<HashableTrack, i32>> {
    let mut source_playlist = spotify.playlist_tracks(source_playlist_id, None, None);
    let mut track_weights = HashMap::new();

    while let Some(item) = source_playlist.try_next().await? {
        let track = match item.track.unwrap() {
            PlayableItem::Track(track) => track,
            PlayableItem::Episode(_e) => bail!("is podcast"),
        };
        track_weights.insert(HashableTrack(track), 1);
    }

    for pair in fs::read_to_string("weights.txt")?
        .split("\r\n")
        .map(|line| line.split_once(" = ").unwrap())
    {
        for (track, weight) in track_weights.iter_mut() {
            if pair.0.to_lowercase() == track.name.to_lowercase() {
                // make beter file parser
                // if track.name.to_lowercase().contains(&pair.0.to_lowercase()) {
                *weight = pair.1.parse()?;
            }
        }
    }
    Ok(track_weights)
}

async fn create_weighted_playlist(
    spotify: &AuthCodeSpotify,
    user_id: &Id<User>,
    playlist_name: &str,
) -> Result<IdBuf<Playlist>> {
    let playlist_id_str = loop {
        if let Some(list) = spotify.user_playlists(user_id).try_next().await? {
            if &list.name != playlist_name {
                continue;
            }
            break list.id;
        } else {
            break spotify
                .user_playlist_create(user_id, playlist_name, Some(true), Some(false), Some(""))
                .await
                .expect("Couldn't create playlist")
                .id;
        }
    };
    Ok(Id::from_id(&playlist_id_str)?.to_owned())
    // Ok(())
}

async fn add_weighted_tracks(
    spotify: &AuthCodeSpotify,
    track_weights: HashMap<HashableTrack, i32>,
    playlist_id: IdBuf<Playlist>,
) {
    let mut track_ids = vec![];

    for (track, &weight) in &track_weights {
        for _ in 0..weight {
            let track_id = Id::from_id(track.id.as_ref().unwrap()).unwrap();
            track_ids.push(track_id);
        }
    }

    spotify
        .playlist_replace_tracks(&playlist_id, track_ids)
        .await
        .expect("Couldn't replace tracks");
}

#[derive(Debug, PartialEq, Eq)]
struct HashableTrack(FullTrack);

impl Deref for HashableTrack {
    type Target = FullTrack;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for HashableTrack {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Hash for HashableTrack {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.id.as_ref().unwrap().hash(state)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let spotify = setup().await?;

    let track_weights = create_weight_map(&spotify, Id::from_id(TEST_PLAYLIST)?).await?;
    let weighted_playlist_id =
        create_weighted_playlist(&spotify, Id::from_id(ME)?, "better shuffle").await?;
    add_weighted_tracks(&spotify, track_weights, weighted_playlist_id).await;

    Ok(())
}
