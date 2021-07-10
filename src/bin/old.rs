use std::{
    collections::HashMap,
    fs,
    hash::{Hash, Hasher},
    ops::{Deref, DerefMut},
    time::Duration,
};

use anyhow::{bail, Context as _, Result};
use dotenv::dotenv;
use futures::prelude::*;
use rand::{seq::SliceRandom, thread_rng};
use rspotify::{
    model::{idtypes::Track, playing, track, FullTrack, Id, PlayableItem},
    prelude::*,
    AuthCodeSpotify, Config, Credentials, OAuth,
};
use tokio::time::sleep;

// const ME: &str = "da652980ecfd4672";
// const ROHANDLE: &str = "m2ytpjv2g8kwekqgmzszqb7vw";

const TAYLOR_PLAYLIST: &str = "56HuNeZLQRZN4uqcFm47Fm";
// const TEST_PLAYLIST: &str = "7HkiK8ErJKGm9aYcMKLbaC";
// const ROHANDLE_PLAYLIST: &str = "3uidlqZoGHGSbRd9TTcIAZ";

const QUEUE_SIZE: i32 = 5;
const REQUEUE_DEPTH: usize = 3;

async fn setup() -> Result<AuthCodeSpotify> {
    dotenv().ok();
    let creds = Credentials::from_env().unwrap();
    let oauth = OAuth::from_env(rspotify::scopes!(
        "user-library-read",
        "user-modify-playback-state",
        "user-read-currently-playing",
        "user-read-playback-state",
        "user-read-recently-played"
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

async fn queue_from_playlist(spotify: AuthCodeSpotify) -> Result<()> {
    let user_id = Id::from_id("1ac1dae7aa0140b5")?;
    let playlist_id = Id::from_id("7HkiK8ErJKGm9aYcMKLbaC")?;

    let playlist = spotify
        .user_playlist(user_id, Some(playlist_id), None)
        .await
        .expect("Couldn't get playlist");

    let song = match playlist.tracks.items[0].track.as_ref().unwrap() {
        PlayableItem::Track(track) => track,
        PlayableItem::Episode(_e) => bail!("is podcast"),
    };

    let song_id = Id::<Track>::from_id(song.id.as_ref().unwrap())?;

    spotify
        .add_item_to_queue(song_id, None)
        .await
        .expect("Couldn't add song to queue");

    Ok(())
}

async fn queue_weighted(
    spotify: &AuthCodeSpotify,
    track_weights: &HashMap<HashableTrack, i32>,
) -> Result<FullTrack> {
    let weights_vec = track_weights.iter().collect::<Vec<_>>();
    let (chosen_track, _w) = weights_vec
        .choose_weighted(&mut thread_rng(), |item| *item.1)
        .unwrap();

    spotify
        .add_item_to_queue(
            Id::<Track>::from_id(chosen_track.id.as_ref().unwrap())?,
            None,
        )
        .await
        .expect("Couldn't add song to queue");
    Ok(chosen_track.0.clone())
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
    let playlist_id = Id::from_id(TAYLOR_PLAYLIST)?;
    let mut playlist = spotify.playlist_tracks(playlist_id, None, None);
    let mut track_weights = HashMap::new();

    while let Some(item) = playlist.try_next().await? {
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

    let mut recently_queued = vec![];

    for _ in 0..QUEUE_SIZE {
        recently_queued.push(queue_weighted(&spotify, &track_weights).await?);
    }

    // let mut queued = queue_weighted(&spotify, &track_weights).await?;

    loop {
        // let recently_played = spotify
        //     .current_user_recently_played(Some(10))
        //     .await
        //     .expect("Couldn't get recently played tracks");
        // let mut tracker = 0;
        // for track in recently_played.items.iter().rev() {
        //     println!("{}", track.track.name);
        //     if tracker == QUEUE_SIZE {
        //         println!("has recently queued");
        //         break;
        //     }
        //     if track.track == recently_queued[tracker as usize] {
        //         tracker += 1;
        //     }
        // }
        // if tracker == QUEUE_SIZE {
        //     break;
        // } else {
        //     println!("{}", tracker);
        // }

        let current_playable = spotify
            .current_playing(None, Option::<Vec<_>>::None)
            .await?
            .expect("no")
            .item
            .unwrap();
        let current_track = match current_playable {
            PlayableItem::Track(track) => track,
            PlayableItem::Episode(_) => bail!("no")
        };

        if recently_queued[recently_queued.len()-REQUEUE_DEPTH..].contains(&current_track) {
            recently_queued.clear();
            for _ in 0..QUEUE_SIZE {
                recently_queued.push(queue_weighted(&spotify, &track_weights).await?);
            }
        }

        // if queued == current_track {
        //     queued = queue_weighted(&spotify, &track_weights).await?;
        //     println!("queued {}", queued.name);
        // }

        sleep(Duration::from_secs(3)).await;
    }
    Ok(())
}
