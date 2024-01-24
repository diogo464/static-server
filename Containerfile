FROM docker.io/alpine:latest
COPY dist/static-server /usr/bin/static-server
WORKDIR /data
ENV RUST_LOG=info
ENTRYPOINT ["/usr/bin/static-server"]
