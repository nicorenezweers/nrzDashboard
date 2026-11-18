# --- Build ---
FROM rust:1-bookworm AS build
RUN rustup target add wasm32-unknown-unknown \
 && curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
 && cargo binstall -y dioxus-cli@0.7.10
WORKDIR /app
COPY . .
RUN dx bundle --platform web --fullstack --release \
 && mkdir /out && cp -r target/dx/nrz_dashboard/release/web/* /out/

# --- Runtime ---
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates iputils-ping \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /out/ /app/
ENV IP=0.0.0.0 PORT=8080 DATA_DIR=/data
VOLUME /data
EXPOSE 8080
CMD ["/app/server"]
