# Bomajou

Learning Rust with this simple CLI tool to capture recent bookmarks from [raindrop.io](https://raindrop.io/) into a markdown file.

Name was derived from "Bookmark journal".

## How to run

### Requirements

Have a `.env` file with the following information.

```
ACCESS_TOKEN=<raindrop.io access token>
URL_BASE=<raindrop.io api url>
MD_FILE_DESTINATION=<path where output .md file should be stored>
```

### How to get the Access Token for Raindrop.io

https://developer.raindrop.io/

- https://developer.raindrop.io/v1/authentication


### Then run

`cargo run`


## Output structure

```markdown
# Bomajou

## [[YYYY-MM-DD]]

- [Bookmark title](<link to bookmark>)

```
