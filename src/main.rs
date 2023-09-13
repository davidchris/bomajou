use chrono::{DateTime, TimeZone, Utc};
use clap::Parser;
use dotenv::dotenv;
use reqwest::Error as ReqwestError;
use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::fs::File;
use std::io;
use std::io::{BufWriter, Write};

const SORT: &str = "-created";
const PER_PAGE: &str = "20";

#[derive(Debug)]
enum CustomError {
    ReqwestError(reqwest::Error),
    IoError(io::Error),
}

impl From<ReqwestError> for CustomError {
    fn from(err: ReqwestError) -> Self {
        CustomError::ReqwestError(err)
    }
}

impl From<io::Error> for CustomError {
    fn from(err: io::Error) -> Self {
        CustomError::IoError(err)
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = SORT.to_string())]
    sort: String,

    #[arg(short, long, default_value_t = PER_PAGE.to_string())]
    perpage: String,
}

#[tokio::main]
async fn main() -> Result<(), CustomError> {
    dotenv().ok();
    let args = Args::parse();

    let access_token = env::var("ACCESS_TOKEN").expect("ACCESS_TOKEN must be set");
    let url_base = env::var("URL_BASE").expect("URL_BASE must be set");
    let md_file_destination =
        env::var("MD_FILE_DESTINATION").expect("MD_FILE_DESTINATION must be set");
    let url = format!("{}?sort={}&perpage={}", url_base, args.sort, args.perpage,);

    let client = reqwest::Client::new();
    let request = client.get(url).bearer_auth(access_token).build()?;

    let response = client.execute(request).await?;

    if response.status().is_success() {
        let json: Value = response.json().await?;
        if let Some(bookmark_items) = json.get("items") {
            if let Some(bookmark_array) = bookmark_items.as_array() {
                let mut bookmarks_by_date: BTreeMap<String, Vec<String>> = BTreeMap::new();

                for bookmark in bookmark_array.iter() {
                    if let Some(title) = bookmark.get("title") {
                        if let Some(created_str) = bookmark.get("created").and_then(Value::as_str) {
                            let created_dt: DateTime<Utc> = Utc
                                .datetime_from_str(created_str, "%Y-%m-%dT%H:%M:%S%.fZ")
                                .unwrap_or_else(|_| {
                                    println!("Unable to parse timestamp: {}", created_str);
                                    Utc::now()
                                });

                            let created_date = created_dt.format("%Y-%m-%d").to_string();

                            let link = bookmark.get("link").and_then(Value::as_str).unwrap();

                            bookmarks_by_date
                                .entry(created_date)
                                .or_insert_with(Vec::new)
                                .push(format!("[{}]({})", title, link));
                        } else {
                            println!("'created' field not found in JSON object");
                        }
                    }
                }
                // println!("Bookmarks by date hashmap: {}", bookmark_items);
                let file = File::create(md_file_destination)?;
                let mut buf_writer = BufWriter::new(file);

                writeln!(buf_writer, "# Bomajou")?;

                for (date, titles) in bookmarks_by_date.iter() {
                    writeln!(buf_writer, "\n## [[{}]]\n", date)?;

                    for title in titles.iter() {
                        writeln!(buf_writer, "- {}", title)?;
                    }
                }
            } else {
                println!("The 'items' field is not an array.");
            }
        } else {
            println!("'items' field not found in JSON object.");
        }
    } else {
        println!("Request failed with status: {}", response.status());
    }

    Ok(())
}
