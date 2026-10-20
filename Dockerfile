# Container image for `vapx mcp` over HTTP.
#
#   docker build -t vapx-mcp .
#   docker run -d --name vapx-mcp --user "$(id -u):$(id -g)" -p 127.0.0.1:8080:8080 \
#     -v "$HOME/.config/vapx:/config:ro" -v "$PWD/exports:/exports" -e VAPX_MCP_TOKEN vapx-mcp
#
# See "MCP server" in the README.

FROM rust:1-slim AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:stable-slim
# A fixed non-root user. To read a cameras.yaml that only its owner can read,
# run the container with --user set to that owner instead.
RUN useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin vapx \
    && mkdir /exports && chown 10001 /exports
COPY --from=build /src/target/release/vapx /usr/local/bin/vapx
USER 10001
# The camera list is read from a directory mounted at /config (mount the
# directory, not the file: vapx replaces cameras.yaml atomically). There is
# deliberately no VOLUME for it.
ENV VAPX_CONFIG=/config/cameras.yaml \
    VAPX_MCP_EXPORTS_DIR=/exports
EXPOSE 8080
ENTRYPOINT ["vapx"]
# Listens on all interfaces inside the container; publish the port on a
# specific host address (-p 127.0.0.1:8080:8080). VAPX_MCP_TOKEN is required.
CMD ["mcp", "--transport", "http", "--bind", "0.0.0.0:8080"]
