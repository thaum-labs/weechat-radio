# syntax=docker/dockerfile:1
FROM rust:1.85-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock* ./
COPY crates ./crates
COPY docs ./docs
COPY README.md LICENSE NOTICE ./
RUN cargo build --release -p wcr

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/wcr /usr/local/bin/wcr
ENV HOME=/data
VOLUME ["/data"]
ENTRYPOINT ["wcr"]
CMD ["hub", "--bind", "0.0.0.0:7373"]
