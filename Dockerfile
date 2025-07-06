# syntax=docker/dockerfile:1

ARG RUST_VERSION=1.88.0

################################################################################
# Create a stage for building the application.

FROM rust:${RUST_VERSION}-alpine3.20 AS build
WORKDIR /app

# Install host build dependencies.
RUN apk add --no-cache clang lld musl-dev git

# Copy workspace configuration first for better caching
COPY Cargo.toml ./
# Only copy Cargo.lock if it exists
RUN if [ -f Cargo.lock ]; then cp Cargo.lock .; fi

# Copy all crate directories
COPY crates/ ./crates/
COPY examples/ ./examples/

# Build the horizon crate (main application)
RUN --mount=type=cache,target=/app/target/ \
    --mount=type=cache,target=/usr/local/cargo/git/db \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    if [ -f Cargo.lock ]; then \
      cargo build --locked --release --package horizon; \
    else \
      cargo build --release --package horizon; \
    fi && \
    cp ./target/release/horizon /app/server

################################################################################
# Create a new stage for running the application.

FROM alpine:3.20 AS final

# Install runtime dependencies if needed
RUN apk add --no-cache ca-certificates


# Create a non-privileged user that the app will run under.
ARG UID=10001
RUN addgroup -g ${UID} appgroup && \
    adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    -G appgroup \
    appuser


# Create app directory and set permissions
RUN mkdir -p /app && chown appuser:appgroup /app

# Copy plugins directory if it exists
RUN if [ -d plugins ]; then cp -r plugins /app/plugins; fi
RUN if [ -d /app/plugins ]; then chown -R appuser:appgroup /app/plugins; fi

# Copy config.toml if it exists, otherwise create an empty one
RUN if [ -f config.toml ]; then \
  cp config.toml /app/config.toml && \
  chown appuser:appgroup /app/config.toml && \
  chmod 600 /app/config.toml; \
    else \
  touch /app/config.toml && \
  chown appuser:appgroup /app/config.toml && \
  chmod 600 /app/config.toml; \
    fi

# Copy the executable from the "build" stage.
COPY --from=build /app/server /app/server

# Switch to non-privileged user
USER appuser

# Expose the port that the application listens on.
EXPOSE 8080

WORKDIR /app
ENTRYPOINT [ "/app/server" ]