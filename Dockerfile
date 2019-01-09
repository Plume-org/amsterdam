# Build: https://github.com/Plume-org/amsterdam
# Run: docker run --rm -it -v $PWD:/conversion amsterdam:latest md /conversion/*.md
#  $PWD is the current directory, assumed to contain the markdown files for conversion. Change if using a different directory.

FROM rust:1-stretch as builder

# Prep
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    gettext \
    git \
    curl \
    gcc \
    make \
    openssl \
    libssl-dev

# Install rust tools and dependencies ahead of build to improve layer caching
#   No deps only in cargo so cheat a little
WORKDIR /scratch/amsterdam-deps
COPY Cargo.toml Cargo.lock rust-toolchain ./
RUN mkdir src && touch src/lib.rs && cargo build

# Build amsterdam
WORKDIR /scratch/amsterdam
COPY . .
RUN cargo install --path .

# Prep final container with amsterdam as the entrypoint
FROM debian:stretch-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl1.1

COPY --from=builder /usr/local/cargo/bin/amsterdam /bin/
WORKDIR /conversion

CMD ["help"]
ENTRYPOINT ["/bin/bash", "-l", "-c", "amsterdam"]
