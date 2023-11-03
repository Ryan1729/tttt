#![deny(unused_must_use)]

use twitch_irc::{
    ClientConfig,
    SecureTCPTransport,
    TwitchIRCClient,
    login::StaticLoginCredentials,
    message::ServerMessage,
};

use std::net::{SocketAddr, ToSocketAddrs};
use url::Url;

type Res<A> = Result<A, Box<dyn std::error::Error>>;

const SLEEP_DURATION: std::time::Duration = std::time::Duration::from_millis(16);

#[tokio::main]
pub async fn main() -> Res<()> {
    let mut args = std::env::args();

    args.next(); //exe name

    // TODO allow muliple channel names
    let channel_name = args.next()
        .ok_or_else(|| "Channel name is required as first arg")?;

    let login_name = args.next()
        .ok_or_else(|| "Login name is required as second arg")?;

    // TODO Once we can get the token ourselves, accept either token, or required info to get token
    // Somethig like --token <token>
    // Somethig like --get_token <app ID> [local addr]
    let mut oauth_token = args.next()
        .ok_or_else(|| "OAuth token is required as third arg")?;

    let app_id = args.next()
        .ok_or_else(|| "App ID is (currently) required as fourth arg")?;

    let app_secret = args.next()
        .ok_or_else(|| "App Secret is (currently) required as fifth arg")?;

    let (addr, addr_string) = if let Some(addr_str) = args.next() {
        fn first_addr(to_addrs: impl ToSocketAddrs) -> Option<SocketAddr> {
            to_addrs.to_socket_addrs().ok()?.next()
        }

        let addr_vec = Url::parse(&addr_str)?.socket_addrs(|| None)?;

        if let Some(addr) = first_addr(&*addr_vec) {
            (Some(addr), addr_str)
        } else {
            (first_addr((addr_str.as_str(), 8080)), addr_str)
        }
    } else {
        (None, "".to_string())
    };

    tracing_subscriber::fmt::init();

    if let Some(addr) = addr {
        oauth_token = authorize(AuthSpec {
            addr,
            addr_string,
            app_id,
            app_secret,
        })?;
    } else {
        tracing::info!("Got no server address. Not starting auth server.");
    }

    start_bot(BotSpec {
        channel_names: vec![channel_name],
        login_name,
        oauth_token,
    }).await
}

struct AuthSpec {
    addr: SocketAddr,
    /// The original string passed by the user
    addr_string: String,
    app_id: String,
    app_secret: String,
}

fn authorize(AuthSpec {
    addr,
    addr_string,
    app_id,
    app_secret,
}: AuthSpec) -> Res<String> {

    use rand::{Rng, thread_rng};
    use rouille::{Server, try_or_400, Request, Response};
    use std::sync::{Arc, Mutex};

    tracing::info!("got addr {addr:?}");

    let auth_state_key = thread_rng().gen::<u128>();

    #[derive(Debug, Default)]
    struct AuthState {
        user_token: String,
        // TODO? replace these bools with an enum.
        // Or are most of the 8 states valid?
        server_running: bool,
        can_close: bool,
        is_closed: bool,
    }

    let mut auth_state: Arc<Mutex<AuthState>> = Arc::new(
        Mutex::new(
            AuthState::default()
        )
    );

    // Start webserver in background thread
    {
        let auth_state = Arc::clone(&auth_state);
        let auth = Arc::clone(&auth_state);
        tokio::spawn(async move {
            tracing::info!("starting server at {addr:?}");

            let server = Server::new(addr, move |request| {
                tracing::info!("{request:?}");

                let expected = auth_state_key.to_string();
                let actual = request.get_param("state");

                if Some(expected) != actual {
                    let expected = auth_state_key.to_string();
                    tracing::info!("{expected} != {actual:?}");
                    return Response::text("Invalid state!".to_string())
                        .with_status_code(401);
                }

                if let Some(user_token) = request.get_param("code") {
                    tracing::info!("user_token: {user_token:?}");
                    auth.lock().expect("should not be poisoned").user_token = user_token;
                    let document: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <style type="text/css">body{
    margin:40px auto;
    max-width:650px;
    line-height:1.6;
    font-size:18px;
    color:#888;
    background-color:#111;
    padding:0 10px
    }
    h1{line-height:1.2}
    </style>
    <title>'ardly OAuth</title>
</head>
<body>
    <h1>Thanks for Authenticating with 'ardly OAuth!</h1>
You may now close this page.
</body>
</html>"#;
                    Response::html(document)
                } else {
                    Response::text("must provide code").with_status_code(400)
                }
            });
            let auth = Arc::clone(&auth_state);
            auth.lock().expect("should not be poisoned").server_running = true;
            let server = server.expect("server startup error:");

            while !auth.lock().expect("should not be poisoned").can_close {
                server.poll();
                std::thread::sleep(SLEEP_DURATION);
            }

            auth.lock().expect("should not be poisoned").is_closed = true;
        });
    }

    let auth = Arc::clone(&auth_state);

    while !auth.lock().expect("should not be poisoned").server_running {
        std::thread::sleep(SLEEP_DURATION);
    }
    tracing::info!("Done waiting for server to start.");

    const TWITCH_AUTH_BASE_URL: &str = "https://id.twitch.tv/oauth2/";

    let auth_state_key_string = auth_state_key.to_string();

    let mut auth_url = Url::parse(
        TWITCH_AUTH_BASE_URL
    )?;
    auth_url = auth_url.join("authorize")?;
    auth_url.query_pairs_mut()
        .append_pair("client_id", &app_id)
        .append_pair("redirect_uri", &addr_string)
        .append_pair("response_type", "code")
        .append_pair("scope", "chat:read chat:edit")
        .append_pair("force_verify", "true")
        .append_pair("state", &auth_state_key_string)
        ;

    tracing::info!("{}", auth_url.as_str());

    webbrowser::open(auth_url.as_str())?;

    tracing::info!("Waiting for auth confirmation.");

    while auth.lock().expect("should not be poisoned").user_token.is_empty() {
        std::thread::sleep(SLEEP_DURATION);
    }
    tracing::info!("Done waiting for auth confirmation.");

    let user_token = auth.lock().expect("should not be poisoned").user_token.clone();

    let mut token_url = Url::parse(
        TWITCH_AUTH_BASE_URL
    )?;
    token_url = token_url.join("token")?;
    token_url.query_pairs_mut()
        .append_pair("client_id", &app_id)
        .append_pair("client_secret", &app_secret)
        .append_pair("redirect_uri", &addr_string)
        .append_pair("code", &user_token)
        .append_pair("grant_type", "authorization_code")
        ;

    #[derive(serde::Deserialize)]
    struct Resp {
        access_token: String,
        refresh_token: String,
    }

    let Resp {
        access_token,
        refresh_token,
    }: Resp = ureq::post(token_url.as_str())
        .call()?
        .into_json::<Resp>()?;

    auth.lock().expect("should not be poisoned").can_close = true;

    tracing::info!("Waiting for server to close.");
    while !auth.lock().expect("should not be poisoned").is_closed {
        std::thread::sleep(SLEEP_DURATION);
    }
    tracing::info!("Done waiting for server to close.");

    if access_token.is_empty() {
        return Err("access_token was empty!".into());
    }

    tracing::info!("access_token: {access_token}");
    // TODO? use refresh token after a while?
    tracing::info!("refresh_token: {refresh_token}");

    Ok(access_token)
}

struct BotSpec {
    channel_names: Vec<String>,
    login_name: String,
    oauth_token: String,
}

async fn start_bot(
    BotSpec {
        channel_names,
        login_name,
        oauth_token,
    }: BotSpec
) -> Res<()> {
    tracing::info!("Attempting to connect to {channel_names:?} as {login_name}");

    // default configuration is to join chat as anonymous.
    let config = ClientConfig::new_simple(
        StaticLoginCredentials::new(login_name, Some(oauth_token))
    );
    let (mut incoming_messages, client) =
        TwitchIRCClient::<SecureTCPTransport, StaticLoginCredentials>::new(config);

    let join_handle = tokio::spawn({
        let client = client.clone();

        async move {
            // It is important to be consuming incoming messages,
            // otherwise they will back up.
            while let Some(server_message) = incoming_messages.recv().await {
                use ServerMessage::*;

                match server_message {
                    Ping(_) | Pong(_) => {
                        tracing::debug!("Received: {server_message:?}");
                    }
                    _ => tracing::info!("Received: {server_message:?}"),
                }

                match server_message {
                    Privmsg(message) => {
                        tracing::info!("Received Privmsg");

                        if let Some(response) = ardly_bot::response(
                            message.message_text.clone()
                        ) {
                            let reply_result = client.say_in_reply_to(
                                &message,
                                format!("'ardly-bot sez: {}", &response),
                            ).await;

                            if let Err(err) = reply_result {
                                tracing::error!("say_in_reply_to error: {err}");
                            } else {
                                tracing::info!("Replied with \"{response}\"!");
                            }
                        }
                    }
                    Ping(_) | Pong(_) => {}
                    _ => {
                        tracing::info!("Unhandled mesage type");
                    }
                }
            }
        }
    });

    for channel_name in channel_names {
        client.join(channel_name)?;
    }

    // keep the tokio executor alive.
    // If you return instead of waiting the background task will exit.
    join_handle.await?;

    Ok(())
}

mod ardly_bot {
    use regex::Regex;

    pub fn response(mut input: String) -> Option<String> {
        input.make_ascii_lowercase();

        // avoid self replies
        if input.contains("know 'er") {
            return None;
        }

        if input.contains("liquor")
        || input.contains("liqueur") {
            return Some("lick 'er? I 'ardly know 'er!".to_owned());
        }

        if input.contains("parappa")
        || input.contains("rappa") {
            return Some("Kick, punch, block, it's all in the mind.".to_owned());
        }

        // TODO make this a lazy static.
        let er_regex = Regex::new(r"(\P{White_Space}+)er(s|ed|ing)?(\p{White_Space}|[\.!?,]|$)").unwrap();

        let mut best_word = "";
        for captures in er_regex.captures_iter(&input) {
            // 0 is the whole match
            let Some(mut erless_word) = captures.get(1).map(|c| c.as_str()) else {
                continue
            };

            // This seems easier than changing the regex to handle "ererer"
            while erless_word.ends_with("er") {
                erless_word = &erless_word[0..erless_word.len() - 2];
            }

            // TODO? Better bestness criteria?
            if best_word.len() <= erless_word.len() {
                best_word = erless_word;
            }
        }

        if best_word.is_empty() {
            return None;
        }

        if best_word == "rap" {
            return Some("No means no.".to_owned());
        }

        if best_word == "rapp" {
            return Some("What is this? Friday Night Funkin'?".to_owned());
        }

        if best_word == "wrapp" {
            return Some("Like in a blanket? Is she cold?".to_owned());
        }

        if best_word == "jamm" {
            return Some("Um Jammer Lammy? My guitar is in my mind!".to_owned());
        }

        if best_word == "bon" {
            return Some("I'd rather leave 'er bones where they are.".to_owned());
        }

        let mut best_word = best_word.to_owned();

        if // For example, "collid-er"
           best_word.ends_with("id")
        || // For example, "bon-er"
           best_word.ends_with("id")
        || (
            // pok-er => poke 'er
            best_word.ends_with("ok")
            // but, book-er => book 'er
            && !best_word.ends_with("ook")
        )
        {
            best_word.push('e');
        }

        best_word.push_str(" 'er? I 'ardly know 'er!");

        Some(best_word)
    }

    #[test]
    fn response_works_on_these_examples() {
        macro_rules! a {
            ($input: literal, $expected: expr) => {
                let expected: Option<&str> = $expected;
                let expected: Option<String> = expected.map(|s| s.to_owned());
                assert_eq!(response($input.to_owned()), expected);
            }
        }
        a!("", None);
        a!("booker", Some("book 'er? I 'ardly know 'er!"));
        a!("liquor", Some("lick 'er? I 'ardly know 'er!"));
        a!("Large Hadron Collider", Some("collide 'er? I 'ardly know 'er!"));
        a!("cupholder", Some("cuphold 'er? I 'ardly know 'er!"));
        a!("poker", Some("poke 'er? I 'ardly know 'er!"));
        a!("hand me the fish boner", Some("I'd rather leave 'er bones where they are."));
        a!("raper", Some("No means no."));
        a!("rapper", Some("What is this? Friday Night Funkin'?"));
        a!("I'm gonna work as a present wrapper", Some("Like in a blanket? Is she cold?"));
        a!("PaRappa the Rappa", Some("Kick, punch, block, it's all in the mind."));
        a!("They turned on the radio jammer", Some("Um Jammer Lammy? My guitar is in my mind!"));
    }
}