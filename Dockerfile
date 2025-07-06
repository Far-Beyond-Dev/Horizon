# syntax=docker/dockerfile:1

ARG RUST_VERSION=1.88.0

################################################################################
# Create a stage for building the application.

FROM rust:${RUST_VERSION}-alpine3.20 AS build
WORKDIR /app

# Install host build dependencies.
RUN apk add --no-cache clang lld musl-dev git

# Copy workspace configuration first for better caching
COPY Cargo.toml Cargo.lock ./

# Copy all crate directories
COPY crates/ ./crates/
COPY examples/ ./examples/

# Build the horizon crate (main application)
RUN --mount=type=cache,target=/app/target/ \
    --mount=type=cache,target=/usr/local/cargo/git/db \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    cargo build --locked --release --package horizon && \
    cp ./target/release/horizon /bin/server

################################################################################
# Create a new stage for running the application.

FROM alpine:3.20 AS final

# Install runtime dependencies if needed
RUN apk add --no-cache ca-certificates


# Create a non-privileged user that the app will run under.
ARG UID=10001
RUN adduser \
--disabled-password \
--gecos "" \
--home "/nonexistent" \
--shell "/sbin/nologin" \
--no-create-home \
--uid "${UID}" \
appuser

# Copy plugins directory if it exists
RUN if [ -d /app/plugins ]; then cp -r /app/plugins /bin/plugins; fi

# Copy the executable from the "build" stage.
COPY --from=build /bin/server /bin/

# Switch to non-privileged user
USER appuser

# Expose the port that the application listens on.
EXPOSE 8080

# What the container