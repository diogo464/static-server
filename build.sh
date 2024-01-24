#!/usr/bin/sh

mkdir -p dist
cargo build --release --target-dir target --target x86_64-unknown-linux-musl || exit 1
cp target/x86_64-unknown-linux-musl/release/static-server dist/static-server || exit 1
rpm-assembler \
	--name static-server \
	--summary "basic static file server" \
	--arch x86_64 \
	--url https://git.d464.sh/code/static-server \
	dist/static-server:/usr/bin/static-server:0755 || exit 1
mv *.rpm dist/
