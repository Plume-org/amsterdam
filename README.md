# Amsterdam

Import tool for Plume.

## Installing

Clone this repository and run:

```
cargo install --path .
```

## Importing Markdown files

```
amsterdam md article1.md article2.md articles/*.md
```

Amsterdam will ask you for your instance URL, username and password on first run.

The following frontmatter fields are supported:

- `title` : the title of your article
- `subtitle`: its subtitle
- `tags`: a comma-separated list of tags
- `date`: the creation date of this article, in the `YEAR-MONTH-DAY` format
