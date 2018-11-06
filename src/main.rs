extern crate canapi;
extern crate clap;
extern crate dotenv;
extern crate plume_api;
extern crate serde_json;
extern crate reqwest;
extern crate rpassword;

use canapi::*;
use clap::{App, Arg, ArgMatches, SubCommand};
use dotenv::dotenv;
use plume_api::{apps::AppEndpoint, posts::PostEndpoint};
use reqwest::{ClientBuilder, Method};
use std::{io::{self, prelude::*, BufRead, BufReader}, env, fs::{metadata, File, OpenOptions}, str::FromStr, fmt::Display};

fn main() {
    dotenv().ok();
    let mut app = App::new("Amsterdam")
        .bin_name("amsterdam")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Import articles to Plume.")
        .subcommand(SubCommand::with_name("md")
            .arg(Arg::with_name("FILES")
                .takes_value(true)
                .multiple(true)
                .help("The Markdown files to import"))
            .about("Import Markown files")
        );

    if env::var("PLUME_API_TOKEN").is_err() {
        get_token();
    }

    match app.clone().get_matches().subcommand() {
        ("md", Some(args)) => md(args),
        _ => app.print_help().unwrap(),
    }
}

fn write_to_env<A: Display, B: Display>(var: A, val: B) {
    let mut file = OpenOptions::new()
        .write(true)
        .append(true)
        .open(".env")
        .unwrap();

    writeln!(file, "{}={}", var, val).expect("Couldn't write to file");
    dotenv().ok();
}

fn make_client() -> (String, String) {
    println!("Please enter your instance domain (without https:// or http://)");
    let mut url = String::new();
    io::stdin().read_line(&mut url).ok();
    write_to_env("PLUME_API_URL", url);

    let client = AppEndpoint::default().create::<ReqwestFetch>(AppEndpoint {
        name: String::from("Amsterdam"),
        ..AppEndpoint::default()
    });
    if let Ok(client) = client {
        write_to_env("PLUME_CLIENT_ID", client.client_id.clone().unwrap());
        write_to_env("PLUME_CLIENT_SECRET", client.client_secret.clone().unwrap());
        (client.client_id.unwrap(), client.client_secret.unwrap())
    } else {
        panic!("client is err");
    }
}

fn get_token() {
    let (c_id, c_secret) = env::var("PLUME_CLIENT_ID")
        .and_then(|id| env::var("PLUME_CLIENT_SECRET").map(|sec| (id, sec)))
        .unwrap_or_else(|_| make_client());
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

    let json: serde_json::Value = ClientBuilder::new().danger_accept_invalid_certs(true).build().unwrap()
        .get(url.as_str()).send().and_then(|mut r| r.text())
        .map(|t| serde_json::from_str(t.as_str()).unwrap())
        .unwrap();
    write_to_env("PLUME_API_TOKEN", json["token"].as_str().unwrap())
}

enum ParseState {
    Ready,
    FrontMatter(PostEndpoint),
    Body(PostEndpoint),
}

fn md<'a>(args: &'a ArgMatches) {
    for path in args.values_of("FILES").unwrap() {
        if metadata(path).expect("File not found").is_file() {
            println!("Importing {}", path);

            let f = File::open(path).expect("File not found");
            let file = BufReader::new(f);
            let parse_result = file.lines().fold(ParseState::Ready, |state, line| {
                let line = line.expect("Couldn't read line");
                match state {
                    ParseState::Ready => if line.chars().all(|c| c == '-') {
                        ParseState::FrontMatter(PostEndpoint::default())
                    } else {
                        panic!("Your articles should use FrontMatter metadata")
                    },
                    ParseState::FrontMatter(mut post) => {
                        let mut parsed = line.splitn(2, ':');
                        let mut field = parsed.next().expect("No metadata field");
                        if let Some(value) = parsed.next() {
                            let value = value.trim();
                            match field {
                                "title" => post.title = Some(value.to_string()),
                                "subtitle" => post.subtitle = Some(value.to_string()),
                                "tags" => post.tags = Some(value.split(",").map(str::trim).map(String::from).collect()),
                                "date" => post.creation_date = Some(value.to_string()),
                                x => println!("The {} field will be ignored.", x),
                            };
                            ParseState::FrontMatter(post)
                        } else {
                            ParseState::Body(post)
                        }
                    },
                    ParseState::Body(mut post) => {
                        if let Some(source) = post.source {
                            post.source = Some(source + "\n" + line.as_ref());
                        } else {
                            post.source = Some(line);
                        }
                        ParseState::Body(post)
                    }
                }
            });

            if let ParseState::Body(article) = parse_result {
                println!("Publishing {}...", article.title.clone().unwrap());
                PostEndpoint::default().create::<ReqwestFetch>(article).ok();
            } else {
                panic!("Error while parsing article metadata");
            }
        }
    }
}

struct ReqwestFetch;

impl Fetch for ReqwestFetch {
    fn fetch<T: Endpoint>(method: &'static str, url: String, data: Option<T>) -> Result<T, Error> {
        ClientBuilder::new()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap()
            .request(
                Method::from_str(method).expect("Canapi: invalid method"),
                format!("https://{}{}", env::var("PLUME_API_URL").unwrap(), url).as_str()
            )
            .body(serde_json::to_string(&data).expect("Error while serializing post"))
            .header("Content-Type", "application/json")
            .bearer_auth(env::var("PLUME_API_TOKEN").unwrap_or(String::new()))
            .send()
            .and_then(|mut r| r.text())
            .map_err(|e| { println!("err: {}", e.to_string()); Error::Fetch(e.to_string()) })
            .and_then(|t| { println!("got: {}", t); serde_json::from_str(t.as_ref()).map_err(|e| Error::SerDe(e.to_string())) })
    }
}
