# syntax=docker/dockerfile:1
FROM rust:bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --bin wcr-path

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/wcr-path /usr/local/bin/wcr-path
ENV WCR_PATH_BIND=0.0.0.0:7380
EXPOSE 7380
ENTRYPOINT ["wcr-path"]
