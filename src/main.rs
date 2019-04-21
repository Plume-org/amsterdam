use clap::{App, Arg, ArgMatches, SubCommand};
use dotenv::dotenv;
use plume_api::{apps::NewAppData, posts::NewPostData};
use reqwest::Client;
use std::{
    env,
    fmt::Display,
    fs::{metadata, File, OpenOptions},
    io::{self, prelude::*, BufRead, BufReader},
};

fn main() {
    let client = Client::new();

    if let Err(e) = run(&client) {
        println!("Error: {}", e);
    }
}

fn run(client: &Client) -> Result<(), String> {
    dotenv().ok();
    let mut app = App::new("Amsterdam")
        .bin_name("amsterdam")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Import articles to Plume.")
        .subcommand(
            SubCommand::with_name("md")
                .arg(
                    Arg::with_name("FILES")
                        .takes_value(true)
                        .multiple(true)
                        .help("The Markdown files to import"),
                )
                .about("Import Markown files"),
        );

    if env::var("PLUME_API_TOKEN").is_err() {
        get_token(&client)?;
    }

    match app.clone().get_matches().subcommand() {
        ("md", Some(args)) => md(args, client),
        _ => app.print_help().map_err(|_| "Try 'amsterdam md'".to_string()),
    }
}

fn write_to_env<A: Display, B: Display>(var: A, val: B) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open(".env")
        .map_err(|_| "Couldn't open .env")?;

    writeln!(file, "{}={}", var, val).map_err(|_| "Couldn't write to .env")?;
    dotenv().ok();
    Ok(())
}

fn make_client(req: &Client) -> Result<(String, String), String> {
    println!("Please enter your instance domain (without https:// or http://)");
    let mut url = String::new();
    io::stdin().read_line(&mut url).ok();
    write_to_env("PLUME_API_URL", url)?;

    let client: serde_json::Value = req
        .post(&format!(
            "https://{}/api/v1/apps",
            env::var("PLUME_API_URL").unwrap()
        ))
        .json(&NewAppData {
            name: "Amsterdam".into(),
            website: Some("https://github.com/Plume-org/amsterdam".into()),
            redirect_uri: None,
        })
        .send()
        .and_then(|mut r| r.json())
        .map_err(|_| "Couldn't register the Amsterdam app".to_string())?;

    write_to_env("PLUME_CLIENT_ID", client["client_id"].to_string())?;
    write_to_env("PLUME_CLIENT_SECRET", client["client_secret"].to_string())?;
    Ok((
        env::var("PLUME_CLIENT_ID").unwrap(),
        env::var("PLUME_CLIENT_SECRET").unwrap(),
    ))
}

fn get_token(client: &Client) -> Result<(), String> {
    let (c_id, c_secret) = env::var("PLUME_CLIENT_ID")
        .and_then(|id| env::var("PLUME_CLIENT_SECRET").map(|sec| (id, sec)))
        .or_else(|_| make_client(&client))?;
    println!("What is your username?");
    let mut name = String::new();
    io::stdin().read_line(&mut name).ok();

    println!("What is your password?");
    let password = rpassword::read_password().unwrap();

    let url = format!(
        "https://{}/api/v1/oauth2?username={}&password={}&client_id={}&client_secret={}&scopes=write",
        env::var("PLUME_API_URL").unwrap(),
        name,
        password,
        c_id,
        c_secret,
    );

    let json: serde_json::Value = client
        .get(&url)
        .send()
        .and_then(|mut r| r.json())
        .map_err(|_| "Couldn't obtain an API token")?;
    write_to_env("PLUME_API_TOKEN", json["token"].as_str().unwrap())
}

enum ParseState {
    Ready,
    FrontMatter(NewPostData),
    Body(NewPostData),
}

fn md<'a>(args: &'a ArgMatches, client: &Client) -> Result<(), String> {
    for path in args
        .values_of("FILES")
        .ok_or("Please provide files to upload".to_string())?
    {
        if metadata(path)
            .or(Err(format!("Couldn't read {}", path)))?
            .is_file()
        {
            println!("Importing {}…", path);

            let f = File::open(path).or(Err(format!("Couldn't read {}", path)))?;
            let file = BufReader::new(f);
            let parse_result = file.lines().fold(ParseState::Ready, |state, line| {
                let line = line.map_err(|_| println!("Couldn't read {}", path)).ok();
                if let Some(line) = line {
                    match state {
                        ParseState::Ready => {
                            if line.chars().all(|c| c == '-') {
                                let mut post = NewPostData::default();
                                post.published = Some(true);
                                ParseState::FrontMatter(post)
                            } else {
                                ParseState::Ready
                            }
                        }
                        ParseState::FrontMatter(mut post) => {
                            let mut parsed = line.splitn(2, ':');
                            let field = parsed.next().expect("THIS IS NOT POSSIBLE");
                            if let Some(value) = parsed.next() {
                                let value = value.trim();
                                match field {
                                    "title" => post.title = value.to_string(),
                                    "subtitle" => post.subtitle = Some(value.to_string()),
                                    "tags" => {
                                        post.tags = Some(
                                            value
                                                .split(",")
                                                .map(str::trim)
                                                .map(String::from)
                                                .collect(),
                                        )
                                    }
                                    "date" => post.creation_date = Some(value.to_string()),
                                    "license" => post.license = Some(value.to_string()),
                                    x => println!("WARNING: The {} field will be ignored.", x),
                                };
                                ParseState::FrontMatter(post)
                            } else {
                                ParseState::Body(post)
                            }
                        }
                        ParseState::Body(mut post) => {
                            post.source = post.source + "\n" + line.as_ref();
                            ParseState::Body(post)
                        }
                    }
                } else {
                    state
                }
            });

            if let ParseState::Body(article) = parse_result {
                println!("Publishing {}…", article.title);

                client
                    .post(&format!(
                        "https://{}/api/v1/posts/",
                        env::var("PLUME_API_URL").unwrap()
                    ))
                    .json(&article)
                    .bearer_auth(env::var("PLUME_API_TOKEN").unwrap_or(String::new()))
                    .send()
                    .map_err(|_| "Error during publication")?
                    .json::<serde_json::Value>()
                    .map_err(|_| "Couldn't read response as JSON")?
                    ["error"].as_str().map(|e| println!("ERROR: The API returned: {}\nMake sure you have created a blog on Plume.", e));
            } else {
                println!("ERROR: Couldn't parse article metadata for {}", path);
            }
        }
    }
    println!(""); // Just print a blank line to separate each import
    Ok(())
}
