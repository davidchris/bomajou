use chrono::{DateTime, Utc};
use clap::Parser;
use dotenvy::dotenv;
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::time::{Duration, Instant};
use thiserror::Error;

const SORT: &str = "-created";
const PER_PAGE: &str = "20";

#[derive(Debug, Error)]
enum AppError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Deserialize)]
struct RaindropResponse {
    items: Vec<Bookmark>,
}

#[derive(Debug, Deserialize)]
struct Bookmark {
    link: String,
    title: String,
    created: DateTime<Utc>,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = SORT.to_string())]
    sort: String,

    #[arg(short, long, default_value_t = PER_PAGE.to_string())]
    perpage: String,
}

fn parse_existing_file(path: &Path) -> (BTreeMap<String, Vec<String>>, HashSet<String>) {
    let mut bookmarks_by_date: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut existing_urls: HashSet<String> = HashSet::new();

    if !path.exists() {
        return (bookmarks_by_date, existing_urls);
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (bookmarks_by_date, existing_urls),
    };

    let mut current_date: Option<String> = None;

    for line in content.lines() {
        // Parse day headings: #### [[YYYY-MM-DD]] (new format) or ## [[YYYY-MM-DD]] (old format)
        if line.ends_with("]]") {
            let is_day_heading =
                line.starts_with("#### [[") || (line.starts_with("## [[") && line.len() == 17); // ## [[YYYY-MM-DD]] is 17 chars

            if is_day_heading {
                let date = line
                    .trim_start_matches("#### [[")
                    .trim_start_matches("## [[")
                    .trim_end_matches("]]");
                if date.len() == 10 && date.chars().nth(4) == Some('-') {
                    current_date = Some(date.to_string());
                    continue;
                }
            }

            // Skip year and month headings (new format)
            if line.starts_with("## [[") || line.starts_with("### [[") {
                continue;
            }
        }

        // Parse bookmark lines: - [title](url)
        if line.starts_with("- [") {
            if let Some(start) = line.find("](") {
                if let Some(end) = line.rfind(')') {
                    let url = &line[start + 2..end];
                    existing_urls.insert(url.to_string());

                    if let Some(ref date) = current_date {
                        let bookmark_text = line.trim_start_matches("- ").to_string();
                        bookmarks_by_date
                            .entry(date.clone())
                            .or_default()
                            .push(bookmark_text);
                    }
                }
            }
        }
    }

    (bookmarks_by_date, existing_urls)
}

#[tokio::main]
async fn main() -> Result<(), AppError> {
    dotenv().ok();
    let args = Args::parse();

    let access_token = env::var("ACCESS_TOKEN").expect("ACCESS_TOKEN must be set");
    let url_base = env::var("URL_BASE").expect("URL_BASE must be set");
    let md_file_destination =
        env::var("MD_FILE_DESTINATION").expect("MD_FILE_DESTINATION must be set");

    let md_path = Path::new(&md_file_destination);
    let (mut bookmarks_by_date, existing_urls) = parse_existing_file(md_path);
    let existing_count = existing_urls.len();

    println!("Found {} existing bookmark(s) in file", existing_count);

    let client = reqwest::Client::new();
    let perpage: usize = args.perpage.parse().unwrap_or(20);
    let mut page = 0;
    let mut new_bookmarks_count = 0;
    let mut total_fetched = 0;
    let start_time = Instant::now();

    loop {
        let url = format!(
            "{}?sort={}&perpage={}&page={}",
            url_base, args.sort, perpage, page
        );

        let request = client.get(&url).bearer_auth(&access_token).build()?;

        let response = client.execute(request).await?;

        if !response.status().is_success() {
            println!("Request failed with status: {}", response.status());
            break;
        }

        let response_data: RaindropResponse = response.json().await?;

        if response_data.items.is_empty() {
            break;
        }

        let page_count = response_data.items.len();
        total_fetched += page_count;
        print!(
            "\rFetching... page {}, {} bookmarks so far",
            page + 1,
            total_fetched
        );
        io::stdout().flush().ok();

        for bookmark in &response_data.items {
            if existing_urls.contains(&bookmark.link) {
                continue;
            }

            let created_date = bookmark.created.format("%Y-%m-%d").to_string();

            bookmarks_by_date
                .entry(created_date)
                .or_default()
                .push(format!("[{}]({})", bookmark.title, bookmark.link));

            new_bookmarks_count += 1;
        }

        if response_data.items.len() < perpage {
            break;
        }

        page += 1;

        // Rate limit: ~2 requests/sec to stay well under 120/min limit
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let elapsed = start_time.elapsed();
    let throughput = if elapsed.as_secs_f64() > 0.0 {
        total_fetched as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };

    println!();
    println!(
        "Fetched {} bookmarks in {:.2}s ({:.1} bookmarks/sec)",
        total_fetched,
        elapsed.as_secs_f64(),
        throughput
    );
    println!(
        "New: {}, Skipped (existing): {}, Total in file: {}",
        new_bookmarks_count,
        total_fetched - new_bookmarks_count,
        existing_count + new_bookmarks_count
    );

    if bookmarks_by_date.is_empty() {
        println!("No bookmarks to write, keeping existing file unchanged.");
        return Ok(());
    }

    let file = File::create(md_file_destination)?;
    let mut buf_writer = BufWriter::new(file);

    writeln!(buf_writer, "# Bomajou")?;

    let mut current_year: Option<&str> = None;
    let mut current_month: Option<&str> = None;

    for (date, bookmarks) in bookmarks_by_date.iter() {
        // date format: YYYY-MM-DD
        let year = &date[0..4];
        let month = &date[0..7];

        if current_year != Some(year) {
            writeln!(buf_writer, "\n## [[{}]]", year)?;
            current_year = Some(year);
            current_month = None;
        }

        if current_month != Some(month) {
            writeln!(buf_writer, "\n### [[{}]]", month)?;
            current_month = Some(month);
        }

        writeln!(buf_writer, "\n#### [[{}]]\n", date)?;

        for bookmark in bookmarks.iter() {
            writeln!(buf_writer, "- {}", bookmark)?;
        }
    }

    Ok(())
}
