FROM rust:1.50-alpine as builder
RUN apk add --no-cache musl-dev

WORKDIR /usr/src
COPY Cargo.toml .
COPY Cargo.lock .
# this is really dumb but seems to work (prevents a new fetch on all code changes)
RUN mkdir src && touch src/main.rs
RUN cargo fetch

COPY . .
RUN cargo build --release

FROM alpine:3.13.2
COPY --from=builder /usr/src/target/release/rust-test /usr/app
ENV LOG_JSON=1
CMD ["/usr/app"]