use dotenv::dotenv;
use rspotify::{prelude::*, AuthCodeSpotify, Config, Credentials, OAuth};

#[tokio::main]
async fn main() {
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
}
