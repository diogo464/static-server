#!/usr/bin/sh

if [ "$VERSION" = "" ]; then
	echo "Missing VERSION"
	exit 1
fi

IMAGE="git.d464.sh/code/static-server"

mkdir -p dist
cargo build --release --target-dir target --target x86_64-unknown-linux-musl || exit 1
cp target/x86_64-unknown-linux-musl/release/static-server dist/static-server || exit 1
rpm-assembler \
	--name static-server \
	--summary "basic static file server" \
	--version $VERSION \
	--arch x86_64 \
	--url https://git.d464.sh/code/static-server \
	dist/static-server:/usr/bin/static-server:0755 || exit 1
mv *.rpm dist/
docker build -t "$IMAGE:$VERSION" -f Containerfile . || exit 1
docker tag "$IMAGE:$VERSION" "$IMAGE:latest" || exit 1

if [ "$PUSH" = "1" ]; then
	docker push "$IMAGE:$VERSION" || exit 1
	docker push "$IMAGE:latest" || exit 1
fi
